//! Normalising captures into an attack surface.
//!
//! A raw capture is a list of requests whose exact shape changes between two
//! runs of the same build: page-load timing moves requests between steps,
//! bundlers rename hashed chunks on every build, and timer-driven calls come
//! and go. REA's own comparison reports all of that as "changed". The
//! surface keeps only what matters for testing — which endpoints exist, with
//! which parameters, statuses and security headers, and which third parties
//! the page talks to — so two surfaces of the same build compare equal.

use crate::capture::ScenarioCapture;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

/// Resource types counted but never listed: they are not attack surface and
/// they dominate every raw diff.
const ASSET_TYPES: &[&str] = &["image", "font", "stylesheet", "media"];

/// Response headers worth tracking per endpoint.
pub const SECURITY_HEADERS: &[&str] = &[
    "content-security-policy",
    "strict-transport-security",
    "x-frame-options",
    "x-content-type-options",
    "referrer-policy",
    "permissions-policy",
    "feature-policy",
    "access-control-allow-origin",
    "access-control-allow-credentials",
    "set-cookie",
    "x-powered-by",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EndpointKey {
    pub method: String,
    /// Path template: numeric and long-hex segments replaced, query dropped.
    pub path: String,
}

impl std::fmt::Display for EndpointKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.method, self.path)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub params: BTreeSet<String>,
    pub statuses: BTreeSet<u16>,
    pub security_headers: BTreeSet<String>,
    /// Request body field names seen in HAR imports (never values).
    #[serde(default)]
    pub body_fields: BTreeSet<String>,
    /// Request body kinds seen: `json`, `form`, `multipart`, `other`.
    #[serde(default)]
    pub body_kinds: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub origin: String,
    pub endpoints: BTreeMap<EndpointKey, Endpoint>,
    /// Script paths with build hashes collapsed.
    pub scripts: BTreeSet<String>,
    pub third_party_origins: BTreeSet<String>,
    pub assets_ignored: usize,
    pub events_dropped: u64,
    /// HTML page loads seen, and the security headers any of them sent.
    /// Page-level headers such as CSP matter on documents, not API calls.
    pub documents_seen: usize,
    pub page_security_headers: BTreeSet<String>,
    /// Endpoints seen in only one of two captures of the same build. They are
    /// left in `endpoints` and excluded from diffs.
    pub unstable: BTreeSet<EndpointKey>,
}

impl Surface {
    /// Builds the surface of `origin` from one or more capture batches.
    pub fn build(origin: &str, captures: &[ScenarioCapture]) -> Self {
        let origin = origin.trim_end_matches('/').to_string();
        let mut s = Surface {
            origin: origin.clone(),
            ..Default::default()
        };

        for capture in captures {
            s.events_dropped += capture.events.dropped;
            let responses: HashMap<&str, _> = capture
                .events
                .items
                .iter()
                .filter(|e| e.kind == "response")
                .map(|e| (e.transaction_id.as_str(), e))
                .collect();

            for req in capture.events.items.iter().filter(|e| e.kind == "request") {
                let req_origin = req.url.origin.clone().unwrap_or_default();
                if req_origin != origin {
                    if !req_origin.is_empty() {
                        s.third_party_origins.insert(req_origin);
                    }
                    continue;
                }
                let path = req.url.url.strip_prefix(&origin).unwrap_or(&req.url.url);
                match req.resource_type.as_deref().unwrap_or("other") {
                    t if ASSET_TYPES.contains(&t) => s.assets_ignored += 1,
                    "script" => {
                        s.scripts.insert(normalize_path(path));
                    }
                    "document" => {
                        s.documents_seen += 1;
                        if let Some(resp) = responses.get(req.transaction_id.as_str()) {
                            s.page_security_headers
                                .extend(security_headers(&resp.header_names));
                        }
                    }
                    _ => {
                        let key = EndpointKey {
                            method: req.method.clone(),
                            path: normalize_path(path),
                        };
                        let ep = s.endpoints.entry(key).or_default();
                        ep.params
                            .extend(req.url.query_parameter_names.iter().cloned());
                        ep.body_fields.extend(req.body_field_names.iter().cloned());
                        ep.body_kinds.extend(req.body_kind.iter().cloned());
                        if let Some(resp) = responses.get(req.transaction_id.as_str()) {
                            if let Some(status) = resp.status {
                                ep.statuses.insert(status);
                            }
                            ep.security_headers
                                .extend(security_headers(&resp.header_names));
                        }
                    }
                }
            }
        }
        s
    }

    /// Marks endpoints that differ between this surface and `repeat`, a
    /// second capture of the same build, as unstable.
    pub fn mark_unstable(&mut self, repeat: &Surface) {
        let mine: BTreeSet<_> = self.endpoints.keys().cloned().collect();
        let theirs: BTreeSet<_> = repeat.endpoints.keys().cloned().collect();
        self.unstable
            .extend(mine.symmetric_difference(&theirs).cloned());
    }
}

/// The tracked security headers among `names`, lowercased.
fn security_headers(names: &[String]) -> impl Iterator<Item = String> + '_ {
    names
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .filter(|h| SECURITY_HEADERS.contains(&h.as_str()))
}

/// Path template for grouping requests into endpoints.
///
/// `/rest/track-order/5267` → `/rest/track-order/:id`, query and fragment
/// dropped, and a build hash before `.js` collapsed so
/// `/chunk-DAJ4olp_.js` and `/chunk-1qQQGal7.js` are the same script.
// The patterns are constants: a mistake in one fails the tests, not a user's run.
#[allow(clippy::unwrap_used)]
pub fn normalize_path(path: &str) -> String {
    static RES: OnceLock<[Regex; 3]> = OnceLock::new();
    let [num, hex, hash] = RES.get_or_init(|| {
        [
            Regex::new(r"/\d+(/|$)").unwrap(),
            Regex::new(r"/[0-9a-fA-F]{16,}(/|$)").unwrap(),
            Regex::new(r"[-.]([A-Za-z0-9_-]{8})\.js$").unwrap(),
        ]
    });
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let path = if path.is_empty() { "/" } else { path };
    // Applied twice so adjacent numeric segments (`/1/2`) are both replaced:
    // the first match consumes the slash the second one needs.
    let path = num.replace_all(path, "/:id$1");
    let path = num.replace_all(&path, "/:id$1");
    let path = hex.replace_all(&path, "/:hash$1");
    // An eight-letter lowercase word (`-polyfill.js`) is a name, not a hash:
    // build hashes carry a digit or a capital.
    hash.replace_all(&path, |c: &regex::Captures| {
        if c[1]
            .chars()
            .any(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase())
        {
            "-*.js".to_string()
        } else {
            c[0].to_string()
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::parse;

    #[test]
    fn ids_become_placeholders() {
        assert_eq!(
            normalize_path("/rest/track-order/5267"),
            "/rest/track-order/:id"
        );
        assert_eq!(normalize_path("/api/a/1/2"), "/api/a/:id/:id");
        assert_eq!(
            normalize_path("/api/Challenges/?name=Score%20Board"),
            "/api/Challenges/"
        );
    }

    #[test]
    fn build_hashes_collapse() {
        assert_eq!(normalize_path("/chunk-DAJ4olp_.js"), "/chunk-*.js");
        assert_eq!(normalize_path("/chunk-1qQQGal7.js"), "/chunk-*.js");
        assert_eq!(normalize_path("/main.js"), "/main.js");
        assert_eq!(normalize_path("/vendor-polyfill.js"), "/vendor-polyfill.js");
    }

    fn capture(events: &str) -> ScenarioCapture {
        parse(&format!(
            r#"{{"events":{{"dropped":0,"items":[{events}]}}}}"#
        ))
        .unwrap()
    }

    fn req(id: &str, method: &str, url: &str, origin: &str, rtype: &str, params: &str) -> String {
        format!(
            r#"{{"kind":"request","transaction_id":"{id}","method":"{method}",
                "url":{{"url":"{url}","origin":"{origin}","query_parameter_names":[{params}]}},
                "resource_type":"{rtype}","header_names":[]}}"#
        )
    }

    fn resp(id: &str, status: u16, headers: &str) -> String {
        format!(
            r#"{{"kind":"response","transaction_id":"{id}","method":"GET",
                "url":{{"url":"x","origin":"x","query_parameter_names":[]}},
                "header_names":[{headers}],"status":{status}}}"#
        )
    }

    #[test]
    fn requests_sort_into_endpoints_scripts_assets_and_third_parties() {
        let o = "http://127.0.0.1:3012";
        let c = capture(
            &[
                req(
                    "1",
                    "GET",
                    &format!("{o}/rest/user/whoami?fields=email"),
                    o,
                    "fetch",
                    r#""fields""#,
                ),
                resp("1", 200, r#""X-Frame-Options","etag""#),
                req("2", "GET", &format!("{o}/assets/a.jpg"), o, "image", ""),
                req(
                    "3",
                    "GET",
                    &format!("{o}/chunk-DAJ4olp_.js"),
                    o,
                    "script",
                    "",
                ),
                req(
                    "4",
                    "GET",
                    "https://cdnjs.cloudflare.com/x.js",
                    "https://cdnjs.cloudflare.com",
                    "script",
                    "",
                ),
            ]
            .join(","),
        );
        let s = Surface::build(o, &[c]);

        let key = EndpointKey {
            method: "GET".into(),
            path: "/rest/user/whoami".into(),
        };
        let ep = &s.endpoints[&key];
        assert_eq!(ep.params, BTreeSet::from(["fields".to_string()]));
        assert_eq!(ep.statuses, BTreeSet::from([200]));
        assert_eq!(
            ep.security_headers,
            BTreeSet::from(["x-frame-options".to_string()])
        );
        assert_eq!(s.assets_ignored, 1);
        assert_eq!(s.scripts, BTreeSet::from(["/chunk-*.js".to_string()]));
        assert_eq!(
            s.third_party_origins,
            BTreeSet::from(["https://cdnjs.cloudflare.com".to_string()])
        );
    }

    #[test]
    fn an_endpoint_seen_in_only_one_repeat_is_unstable() {
        let o = "http://h";
        let a = Surface::build(
            o,
            &[capture(&req(
                "1",
                "GET",
                "http://h/rest/continue-code",
                o,
                "fetch",
                "",
            ))],
        );
        let mut b = Surface::build(o, &[capture("")]);
        b.mark_unstable(&a);
        assert!(b.unstable.contains(&EndpointKey {
            method: "GET".into(),
            path: "/rest/continue-code".into()
        }));
    }
}

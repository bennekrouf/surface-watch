//! HAR recordings as captures.
//!
//! A crawl only sees what pages request on load. A HAR recorded while a
//! tester logs in, submits forms or sends a chat message covers the rest.
//! Browsers' DevTools, Caido, ZAP and mitmproxy all export HAR 1.2.
//!
//! A HAR is full of secrets: cookies, `Authorization` headers, request and
//! response bodies, tokens in query strings. None of that is needed for the
//! surface, so the conversion keeps only what [`crate::surface`] reads —
//! method, URL without its query, query parameter *names*, resource type,
//! status and header *names* — and the stored capture holds nothing else.
//! The original file stays where the user keeps it; the capture records its
//! SHA-256 so the report can cite exactly which recording it came from.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Har {
    log: Log,
}

#[derive(Deserialize)]
struct Log {
    #[serde(default)]
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(default)]
    started_date_time: Option<String>,
    request: Request,
    response: Response,
    /// Chrome DevTools and Playwright record the resource type here.
    #[serde(default, rename = "_resourceType")]
    resource_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<NameOnly>,
    #[serde(default)]
    query_string: Vec<NameOnly>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    status: i64,
    #[serde(default)]
    headers: Vec<NameOnly>,
    #[serde(default)]
    content: Option<Content>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    #[serde(default)]
    mime_type: Option<String>,
}

/// Only a header's or parameter's name is ever read: values are not even
/// deserialised, so they cannot leak into what is stored.
#[derive(Deserialize)]
struct NameOnly {
    name: String,
}

/// A HAR converted into Surface Watch's capture format.
#[derive(Debug, Clone)]
pub struct Converted {
    /// `{"source":"har","events":{...}}`, readable by [`crate::capture::parse`].
    pub capture: Value,
    pub sha256: String,
    pub entries: usize,
    /// When the recording started, if the HAR says.
    pub recorded_at: Option<DateTime<Utc>>,
}

pub fn convert(text: &str) -> Result<Converted, String> {
    let har: Har = serde_json::from_str(text).map_err(|e| format!("not a HAR file: {e}"))?;
    if har.log.entries.is_empty() {
        return Err("the HAR file has no requests".into());
    }
    let sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    let recorded_at = har
        .log
        .entries
        .iter()
        .filter_map(|e| e.started_date_time.as_deref())
        .filter_map(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
        .min();

    let mut items = vec![];
    for (i, e) in har.log.entries.iter().enumerate() {
        let id = format!("har-{i}");
        let (url, origin) = split_url(&e.request.url);
        let mut params: Vec<String> = e
            .request
            .query_string
            .iter()
            .map(|q| q.name.clone())
            .collect();
        if params.is_empty() {
            params = query_names(&e.request.url);
        }
        params.sort();
        params.dedup();
        let mime = e
            .response
            .content
            .as_ref()
            .and_then(|c| c.mime_type.as_deref())
            .unwrap_or_default();
        let rtype = resource_type(e.resource_type.as_deref(), mime, &url);
        let url_obj = json!({ "url": url, "origin": origin, "query_parameter_names": params });
        items.push(json!({
            "kind": "request", "transaction_id": id, "method": e.request.method,
            "url": url_obj, "resource_type": rtype,
            "header_names": names(&e.request.headers), "status": null
        }));
        // Status 0 means the request never got an answer (blocked, aborted).
        if e.response.status > 0 {
            items.push(json!({
                "kind": "response", "transaction_id": id, "method": e.request.method,
                "url": url_obj, "resource_type": rtype,
                "header_names": names(&e.response.headers), "status": e.response.status
            }));
        }
    }

    Ok(Converted {
        capture: json!({ "source": "har", "events": { "dropped": 0, "items": items } }),
        sha256,
        entries: har.log.entries.len(),
        recorded_at,
    })
}

fn names(headers: &[NameOnly]) -> Vec<String> {
    headers
        .iter()
        .map(|h| h.name.to_ascii_lowercase())
        .collect()
}

/// `https://h:8443/a/b?x=1#f` → (`https://h:8443/a/b`, `https://h:8443`).
/// The query is dropped: its values are where tokens live.
fn split_url(url: &str) -> (String, String) {
    let no_query = url.split(['?', '#']).next().unwrap_or_default().to_string();
    let origin = match no_query.find("://") {
        Some(i) => {
            let rest = &no_query[i + 3..];
            let host_end = rest.find('/').unwrap_or(rest.len());
            // Strip userinfo (`user:pass@host`) from the origin too.
            let host = &rest[..host_end];
            let host = host.rsplit('@').next().unwrap_or(host);
            format!("{}://{}", &no_query[..i], host.to_ascii_lowercase())
        }
        None => String::new(),
    };
    let path = no_query
        .find("://")
        .and_then(|i| no_query[i + 3..].find('/').map(|j| i + 3 + j))
        .map(|j| no_query[j..].to_string())
        .unwrap_or_else(|| "/".into());
    (format!("{origin}{path}"), origin)
}

fn query_names(url: &str) -> Vec<String> {
    let Some((_, query)) = url.split('#').next().unwrap_or_default().split_once('?') else {
        return vec![];
    };
    query
        .split('&')
        .filter_map(|kv| kv.split('=').next())
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .collect()
}

/// The HAR's own resource type when present, otherwise inferred from the
/// response MIME type, then the file extension.
fn resource_type(recorded: Option<&str>, mime: &str, url: &str) -> String {
    if let Some(t) = recorded.filter(|t| !t.is_empty()) {
        return t.to_ascii_lowercase();
    }
    let mime = mime.to_ascii_lowercase();
    let path = url.to_ascii_lowercase();
    let t = if mime.contains("html") {
        "document"
    } else if mime.contains("javascript") || mime.contains("ecmascript") || path.ends_with(".js") {
        "script"
    } else if mime.contains("css") || path.ends_with(".css") {
        "stylesheet"
    } else if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("font/") || mime.contains("font") {
        "font"
    } else if mime.starts_with("audio/") || mime.starts_with("video/") {
        "media"
    } else {
        "fetch"
    };
    t.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture;
    use crate::surface::{EndpointKey, Surface};

    const HAR: &str = r#"{"log":{"version":"1.2","entries":[
      {"startedDateTime":"2026-10-13T09:00:00.000Z",
       "request":{"method":"GET","url":"https://shop.example/","headers":[{"name":"Cookie","value":"session=SECRET"}],"queryString":[]},
       "response":{"status":200,"headers":[{"name":"Content-Security-Policy","value":"default-src 'self'"}],"content":{"mimeType":"text/html"}}},
      {"startedDateTime":"2026-10-13T09:00:01.000Z",
       "request":{"method":"POST","url":"https://shop.example/rest/chat?token=SECRET2","headers":[{"name":"Authorization","value":"Bearer SECRET3"}],"queryString":[{"name":"token","value":"SECRET2"}],"postData":{"text":"{\"password\":\"SECRET4\"}"}},
       "response":{"status":200,"headers":[{"name":"X-Frame-Options","value":"DENY"}],"content":{"mimeType":"application/json","text":"SECRET5"}}},
      {"request":{"method":"GET","url":"https://shop.example/chunk-DAJ4olp_.js"},"response":{"status":200,"content":{"mimeType":"application/javascript"}}},
      {"request":{"method":"GET","url":"https://shop.example/a.png"},"response":{"status":200,"content":{"mimeType":"image/png"}}},
      {"request":{"method":"GET","url":"https://cdn.other.example/lib.js"},"response":{"status":200,"content":{"mimeType":"text/javascript"}}},
      {"request":{"method":"GET","url":"https://shop.example/rest/blocked"},"response":{"status":0}}
    ]}}"#;

    #[test]
    fn a_har_becomes_the_same_surface_a_crawl_would() {
        let c = convert(HAR).unwrap();
        let cap = capture::parse(&c.capture.to_string()).unwrap();
        let s = Surface::build("https://shop.example", &[cap]);

        let chat = EndpointKey {
            method: "POST".into(),
            path: "/rest/chat".into(),
        };
        assert_eq!(s.endpoints[&chat].params, ["token".to_string()].into());
        assert!(s.endpoints[&chat]
            .security_headers
            .contains("x-frame-options"));
        assert_eq!(s.documents_seen, 1);
        assert!(s.page_security_headers.contains("content-security-policy"));
        assert!(s.scripts.contains("/chunk-*.js"));
        assert_eq!(s.assets_ignored, 1);
        assert!(s.third_party_origins.contains("https://cdn.other.example"));
        // A request that never got an answer is surface, with no status.
        let blocked = EndpointKey {
            method: "GET".into(),
            path: "/rest/blocked".into(),
        };
        assert!(s.endpoints[&blocked].statuses.is_empty());
        assert_eq!(c.entries, 6);
        assert_eq!(
            c.recorded_at.unwrap().to_rfc3339(),
            "2026-10-13T09:00:00+00:00"
        );
    }

    #[test]
    fn no_secret_from_the_har_survives_the_conversion() {
        let stored = convert(HAR).unwrap().capture.to_string();
        for secret in ["SECRET", "Bearer", "password", "session="] {
            assert!(!stored.contains(secret), "{secret} leaked into: {stored}");
        }
    }

    #[test]
    fn urls_lose_their_query_and_userinfo() {
        assert_eq!(
            split_url("https://user:pw@Shop.example:8443/a/b?x=1#f"),
            (
                "https://shop.example:8443/a/b".to_string(),
                "https://shop.example:8443".to_string()
            )
        );
        assert_eq!(query_names("https://h/p?a=1&b&=x#c=2"), vec!["a", "b"]);
    }

    #[test]
    fn something_that_is_not_a_har_is_refused() {
        assert!(convert(r#"{"events":[]}"#)
            .unwrap_err()
            .contains("not a HAR"));
        assert!(convert(r#"{"log":{"entries":[]}}"#)
            .unwrap_err()
            .contains("no requests"));
    }

    #[test]
    fn the_same_bytes_give_the_same_hash() {
        assert_eq!(convert(HAR).unwrap().sha256, convert(HAR).unwrap().sha256);
        assert_eq!(convert(HAR).unwrap().sha256.len(), 64);
    }
}

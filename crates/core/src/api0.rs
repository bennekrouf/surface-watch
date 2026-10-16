//! Exporting a target's observed API calls as an api0 catalog.
//!
//! api0 (<https://api0.ai>) turns API endpoints into MCP tools. Its import
//! format is `{ "api_groups": [ { name, base, endpoints: [...] } ] }`, the
//! `ApiStorage` type of `api0-types`. This module writes one group per target
//! from what the frontend was seen calling: its latest crawl merged with every
//! HAR import.
//!
//! The shapes follow how api0's gateway calls a tool:
//!
//! * path parameters are `{name}` placeholders, so `:id` becomes `{id}`;
//! * on GET and DELETE, arguments not used in the path become the query
//!   string, so query parameter names become optional parameters;
//! * on POST, PUT and PATCH the arguments are sent as a JSON body, so the
//!   request body field names a HAR recorded become optional parameters.
//!   Form and multipart bodies cannot be reproduced that way, and their
//!   endpoints say so in their description;
//! * a tool is named after the group name and the endpoint `text`, so `text`
//!   is the verb and path, unique within the group.
//!
//! Identity forwarding is off by default: api0's own headers must not reach a
//! backend that is not the user's.

use crate::model::{Capture, CaptureSource, Engagement, Target};
use crate::store;
use crate::surface::{Endpoint, EndpointKey, Surface};
use crate::watch;
use chrono::Utc;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Send api0's identity headers to this backend. Only for the user's own
    /// first-party backends.
    pub forward_identity: bool,
}

/// An endpoint left out of the export, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub endpoint: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
pub struct Export {
    /// The api0 `ApiStorage` document.
    pub document: Value,
    pub endpoints: usize,
    pub with_body_fields: usize,
    pub skipped: Vec<Skipped>,
}

/// Why an observed request is not an API endpoint worth exporting, if it isn't.
fn skip_reason(key: &EndpointKey, unstable: bool) -> Option<&'static str> {
    let path = key.path.to_ascii_lowercase();
    let last = path.rsplit('/').next().unwrap_or_default();
    const STATIC_DIRS: &[&str] = &["/assets/", "/static/", "/_next/", "/public/", "/favicon"];
    const STATIC_EXT: &[&str] = &[
        ".js", ".mjs", ".css", ".map", ".json", ".ico", ".png", ".jpg", ".jpeg", ".gif", ".svg",
        ".webp", ".avif", ".woff", ".woff2", ".ttf", ".txt", ".xml", ".html",
    ];
    if matches!(key.method.as_str(), "OPTIONS" | "HEAD") {
        Some("preflight or HEAD request")
    } else if path.starts_with("/socket.io")
        || path.starts_with("/engine.io")
        || path.contains("/ws") && path.ends_with("/ws")
    {
        Some("websocket transport")
    } else if STATIC_DIRS.iter().any(|d| path.contains(d))
        || STATIC_EXT.iter().any(|e| last.ends_with(e))
    {
        Some("static file")
    } else if unstable {
        Some("unstable between runs of the same build")
    } else {
        None
    }
}

/// `/rest/track-order/:id` → (`/rest/track-order/{id}`, ["id"]). Repeated
/// names are numbered, `{id}` then `{id2}`, since each placeholder must map
/// to its own argument.
fn api0_path(path: &str) -> (String, Vec<String>) {
    let mut names: Vec<String> = vec![];
    let segments: Vec<String> = path
        .split('/')
        .map(|seg| match seg.strip_prefix(':') {
            Some(name) if !name.is_empty() => {
                let mut n = name.to_string();
                let mut i = 2;
                while names.contains(&n) {
                    n = format!("{name}{i}");
                    i += 1;
                }
                names.push(n.clone());
                format!("{{{n}}}")
            }
            _ => seg.to_string(),
        })
        .collect();
    (segments.join("/"), names)
}

fn param(name: &str, description: String, required: bool) -> Value {
    json!({ "name": name, "description": description, "required": required.to_string() })
}

fn endpoint_json(key: &EndpointKey, ep: &Endpoint, origin: &str) -> Value {
    let (path, path_params) = api0_path(&key.path);
    let has_body = matches!(key.method.as_str(), "POST" | "PUT" | "PATCH");
    let mut params: Vec<Value> = path_params
        .iter()
        .map(|n| param(n, format!("Path segment `{n}`."), true))
        .collect();
    let mut notes: Vec<String> = vec![];

    if has_body {
        for f in &ep.body_fields {
            if !path_params.contains(f) {
                params.push(param(
                    f,
                    "Request body field (observed name; type unknown).".into(),
                    false,
                ));
            }
        }
        if !ep.params.is_empty() {
            notes.push(format!(
                "The frontend also sent query parameters ({}), which api0 would put in the body instead; add them to the path if the backend needs them.",
                ep.params.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if ep
            .body_kinds
            .iter()
            .any(|k| k == "form" || k == "multipart")
        {
            notes.push("The frontend sent a form, not JSON; api0 sends JSON, so the backend may reject it.".into());
        }
        if ep.body_fields.is_empty() {
            notes.push(
                "No request body was recorded: import a HAR of this call to get its fields.".into(),
            );
        }
    } else {
        for q in &ep.params {
            if !path_params.contains(q) {
                params.push(param(
                    q,
                    "Query string parameter (observed name).".into(),
                    false,
                ));
            }
        }
    }

    let statuses: Vec<String> = ep.statuses.iter().map(u16::to_string).collect();
    let mut description = format!(
        "{} {} on {origin}, as called by its frontend.",
        key.method, key.path
    );
    if !statuses.is_empty() {
        description += &format!(" Answered {}.", statuses.join(", "));
    }
    if ep.statuses.iter().any(|s| *s == 401 || *s == 403) {
        description += " Needs credentials: it refused a call without them.";
    }
    for n in notes {
        description += &format!(" {n}");
    }
    description += " Generated by Surface Watch from observed traffic; review before use.";

    json!({
        "text": format!("{} {}", key.method, key.path),
        "description": description,
        "verb": key.method,
        "path": path,
        "parameters": params,
    })
}

/// Builds the api0 document for one target from its surface.
pub fn build(target: &Target, surface: &Surface, captures: &[Capture], opts: Options) -> Export {
    let mut endpoints = vec![];
    let mut skipped = vec![];
    let mut with_body_fields = 0;
    for (key, ep) in &surface.endpoints {
        if let Some(reason) = skip_reason(key, surface.unstable.contains(key)) {
            skipped.push(Skipped {
                endpoint: key.to_string(),
                reason,
            });
            continue;
        }
        if !ep.body_fields.is_empty() {
            with_body_fields += 1;
        }
        endpoints.push(endpoint_json(key, ep, &surface.origin));
    }

    let crawls = captures
        .iter()
        .filter(|c| c.source == CaptureSource::ReaScenario)
        .count();
    let hars = captures.len() - crawls;
    let host = surface
        .origin
        .split_once("://")
        .map(|(_, h)| h)
        .unwrap_or(&surface.origin);
    let name = if target.label.trim().is_empty() || target.label == target.origin {
        host.to_string()
    } else {
        target.label.clone()
    };
    let ids: Vec<&str> = captures.iter().map(|c| c.id.as_str()).collect();
    let n = endpoints.len();
    let document = json!({
        "api_groups": [{
            "name": name,
            "description": format!(
                "API calls the {} frontend was seen making, from {crawls} crawl(s) and {hars} HAR import(s) in Surface Watch (captures {}). Paths, parameter names and statuses are observed; types and descriptions are not. Review before use.",
                surface.origin,
                ids.join(", ")
            ),
            "base": surface.origin,
            "forward_identity": opts.forward_identity,
            "endpoints": endpoints,
        }]
    });
    Export {
        document,
        endpoints: n,
        with_body_fields,
        skipped,
    }
}

/// Builds the export for a stored target and writes it to the engagement's
/// `exports` folder as JSON. Returns the file and the export.
pub fn export(
    engagement: &Engagement,
    target: &Target,
    opts: Options,
) -> Result<(PathBuf, Export), String> {
    let dir = watch::target_dir(&engagement.id, &target.id);
    let (surface, captures) = watch::merged_surface(&dir, &target.origin)?;
    if captures.is_empty() {
        return Err(format!(
            "{} has no capture yet: capture it or import a HAR first.",
            target.label
        ));
    }
    let out = build(target, &surface, &captures, opts);
    if out.endpoints == 0 {
        return Err("No API endpoint to export: the captures only saw static files.".into());
    }
    let path = store::engagement_dir(&engagement.id)
        .join("exports")
        .join(format!(
            "api0-{}-{}.json",
            target.id,
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
    let text = serde_json::to_string_pretty(&out.document).map_err(|e| e.to_string())?;
    store::write_private(&path, &text).map_err(|e| e.to_string())?;
    Ok((path, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CaptureProfile;

    fn key(m: &str, p: &str) -> EndpointKey {
        EndpointKey {
            method: m.into(),
            path: p.into(),
        }
    }

    fn target() -> Target {
        Target {
            id: "shop".into(),
            origin: "https://shop.example".into(),
            label: "https://shop.example".into(),
            profile: CaptureProfile::default(),
        }
    }

    fn ep(statuses: &[u16], params: &[&str], body: &[&str], kinds: &[&str]) -> Endpoint {
        Endpoint {
            statuses: statuses.iter().copied().collect(),
            params: params.iter().map(|s| s.to_string()).collect(),
            body_fields: body.iter().map(|s| s.to_string()).collect(),
            body_kinds: kinds.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn surface(eps: Vec<(EndpointKey, Endpoint)>) -> Surface {
        Surface {
            origin: "https://shop.example".into(),
            endpoints: eps.into_iter().collect(),
            ..Default::default()
        }
    }

    fn endpoints(e: &Export) -> Vec<Value> {
        e.document["api_groups"][0]["endpoints"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn path_parameters_become_numbered_placeholders() {
        assert_eq!(
            api0_path("/rest/track-order/:id"),
            ("/rest/track-order/{id}".into(), vec!["id".into()])
        );
        assert_eq!(
            api0_path("/api/a/:id/b/:id"),
            (
                "/api/a/{id}/b/{id2}".into(),
                vec!["id".into(), "id2".into()]
            )
        );
        assert_eq!(
            api0_path("/rest/user/whoami"),
            ("/rest/user/whoami".into(), vec![])
        );
    }

    #[test]
    fn get_query_names_and_post_body_fields_become_parameters() {
        let e = build(
            &target(),
            &surface(vec![
                (
                    key("GET", "/rest/products/search"),
                    ep(&[200], &["q"], &[], &[]),
                ),
                (
                    key("POST", "/rest/user/login"),
                    ep(&[401], &[], &["email", "password"], &["json"]),
                ),
                (
                    key("GET", "/rest/track-order/:id"),
                    ep(&[200], &[], &[], &[]),
                ),
            ]),
            &[],
            Options::default(),
        );
        let eps = endpoints(&e);
        let by_text = |t: &str| eps.iter().find(|x| x["text"] == t).unwrap().clone();

        let search = by_text("GET /rest/products/search");
        assert_eq!(search["parameters"][0]["name"], "q");
        assert_eq!(search["parameters"][0]["required"], "false");

        let login = by_text("POST /rest/user/login");
        let names: Vec<&str> = login["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["email", "password"]);
        assert!(login["description"]
            .as_str()
            .unwrap()
            .contains("Needs credentials"));

        let track = by_text("GET /rest/track-order/:id");
        assert_eq!(track["path"], "/rest/track-order/{id}");
        assert_eq!(track["parameters"][0]["required"], "true");
        assert_eq!(e.with_body_fields, 1);
    }

    #[test]
    fn static_files_sockets_preflights_and_unstable_calls_are_skipped() {
        let mut s = surface(vec![
            (
                key("GET", "/assets/i18n/en.json"),
                ep(&[200], &[], &[], &[]),
            ),
            (key("GET", "/socket.io/"), ep(&[200], &["EIO"], &[], &[])),
            (key("OPTIONS", "/rest/x"), ep(&[204], &[], &[], &[])),
            (key("GET", "/rest/continue-code"), ep(&[200], &[], &[], &[])),
            (key("GET", "/rest/languages"), ep(&[200], &[], &[], &[])),
        ]);
        s.unstable.insert(key("GET", "/rest/continue-code"));
        let e = build(&target(), &s, &[], Options::default());
        assert_eq!(e.endpoints, 1);
        let reasons: Vec<&str> = e.skipped.iter().map(|x| x.reason).collect();
        assert!(reasons.contains(&"static file"));
        assert!(reasons.contains(&"websocket transport"));
        assert!(reasons.contains(&"preflight or HEAD request"));
        assert!(reasons.contains(&"unstable between runs of the same build"));
    }

    #[test]
    fn form_bodies_and_query_on_post_are_called_out() {
        let e = build(
            &target(),
            &surface(vec![(
                key("POST", "/login"),
                ep(&[302], &["next"], &["user"], &["form"]),
            )]),
            &[],
            Options::default(),
        );
        let d = endpoints(&e)[0]["description"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(d.contains("sent a form, not JSON"), "{d}");
        assert!(d.contains("query parameters (next)"), "{d}");
    }

    #[test]
    fn identity_forwarding_is_off_unless_asked_and_the_group_is_named_after_the_host() {
        let s = surface(vec![(key("GET", "/api/me"), ep(&[200], &[], &[], &[]))]);
        let off = build(&target(), &s, &[], Options::default());
        assert_eq!(off.document["api_groups"][0]["forward_identity"], false);
        assert_eq!(off.document["api_groups"][0]["name"], "shop.example");
        assert_eq!(
            off.document["api_groups"][0]["base"],
            "https://shop.example"
        );
        let on = build(
            &target(),
            &s,
            &[],
            Options {
                forward_identity: true,
            },
        );
        assert_eq!(on.document["api_groups"][0]["forward_identity"], true);
    }
}

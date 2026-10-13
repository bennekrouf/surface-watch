//! Reading REA browser-scenario captures.
//!
//! `rea capture-browser-scenario --format json` prints a complete Evidence
//! record; the capture itself sits under `normalized_result`. REA's own
//! `compare-web-captures` only accepts that inner object, which is easy to
//! get wrong, so every reader here accepts either shape.
//!
//! Only the fields the surface needs are typed. REA's schemas are strict and
//! change often, so unknown fields are ignored rather than rejected: a newer
//! REA adding a field must not make old captures unreadable.

use serde::Deserialize;
use std::path::Path;

/// One browser scenario capture, reduced to its network events.
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioCapture {
    pub events: EventLog,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventLog {
    #[serde(default)]
    pub dropped: u64,
    #[serde(default)]
    pub items: Vec<NetworkEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkEvent {
    /// `request`, `response`, `request-finished`, `request-failed`, …
    pub kind: String,
    pub transaction_id: String,
    #[serde(default)]
    pub method: String,
    pub url: EventUrl,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub header_names: Vec<String>,
    #[serde(default)]
    pub status: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventUrl {
    pub url: String,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub query_parameter_names: Vec<String>,
}

/// Parses a capture from REA's JSON output, with or without the Evidence
/// envelope.
pub fn parse(json: &str) -> Result<ScenarioCapture, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
    let inner = value.get("normalized_result").unwrap_or(&value);
    if inner.get("error").is_some() {
        return Err(format!(
            "REA reported an error instead of a capture: {}",
            inner["error"]
        ));
    }
    serde_json::from_value(inner.clone())
        .map_err(|e| format!("not a browser scenario capture: {e}"))
}

pub fn read(path: &Path) -> Result<ScenarioCapture, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads every `*.json` capture in `dir`, in file-name order. A capture split
/// into batches is one directory, one file per batch.
pub fn read_dir(dir: &Path) -> Result<Vec<ScenarioCapture>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{}: no capture files (*.json)", dir.display()));
    }
    paths.iter().map(|p| read(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INNER: &str = r#"{"events":{"retained":1,"dropped":0,"items":[
        {"kind":"request","transaction_id":"request-1","method":"GET",
         "url":{"url":"http://127.0.0.1:3012/","origin":"http://127.0.0.1:3012","query_parameter_names":[]},
         "resource_type":"document","header_names":[],"status":null,"sequence":1}]}}"#;

    #[test]
    fn the_inner_capture_parses() {
        let c = parse(INNER).unwrap();
        assert_eq!(c.events.items.len(), 1);
        assert_eq!(c.events.items[0].method, "GET");
    }

    #[test]
    fn the_evidence_envelope_is_unwrapped() {
        let wrapped = format!(r#"{{"evidence_id":"ev_x","normalized_result":{INNER}}}"#);
        assert_eq!(parse(&wrapped).unwrap().events.items.len(), 1);
    }

    #[test]
    fn an_rea_error_is_reported_as_such() {
        let err =
            parse(r#"{"error":"Browser scenario capture failed","code":"cleanup_incomplete"}"#)
                .unwrap_err();
        assert!(err.contains("REA reported an error"), "got {err}");
    }
}

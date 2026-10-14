//! Running the REA CLI.
//!
//! REA is invoked as a subprocess, the way AIS Monitor drives `az`: no
//! linking, no MCP session, just its documented CLI and `--format json`
//! output. A user-installed `rea` wins; otherwise `npx` fetches the version
//! this app was tested against.

use crate::model::CaptureProfile;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

/// The REA release the capture format and workarounds were verified with.
pub const TESTED_REA_VERSION: &str = "6.4.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rea {
    program: String,
    prefix: Vec<String>,
}

impl Rea {
    /// `SURFACE_WATCH_REA` if set (e.g. `node /path/to/rea/scripts/rea.mjs`,
    /// to try an unreleased REA), else `rea` from PATH, else a pinned `npx`
    /// invocation.
    pub fn detect() -> Self {
        if let Ok(custom) = std::env::var("SURFACE_WATCH_REA") {
            let mut parts = custom.split_whitespace().map(str::to_string);
            if let Some(program) = parts.next() {
                return Rea {
                    program,
                    prefix: parts.collect(),
                };
            }
        }
        if on_path("rea") {
            Rea {
                program: "rea".into(),
                prefix: vec![],
            }
        } else {
            Rea {
                program: "npx".into(),
                prefix: vec!["-y".into(), format!("rea-agents@{TESTED_REA_VERSION}")],
            }
        }
    }

    pub fn describe(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.prefix.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix);
        cmd
    }

    pub fn version(&self) -> Result<String, String> {
        let out = self
            .command()
            .arg("--version")
            .output()
            .map_err(|e| format!("could not run {}: {e}", self.describe()))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Runs an REA command with `--format json` and returns its output
    /// unchanged, or REA's own error message.
    fn run_json(&self, args: &[&str]) -> Result<String, String> {
        let out = self
            .command()
            .args(args)
            .args(["--format", "json"])
            .output()
            .map_err(|e| format!("could not run {}: {e}", self.describe()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            return Ok(stdout);
        }
        Err(rea_error_message(&stdout)
            .unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }

    /// Page targets of a browser exposing CDP at `endpoint`.
    pub fn list_browser_targets(&self, endpoint: &str) -> Result<String, String> {
        self.run_json(&["list-browser-targets", endpoint])
    }

    /// Static analysis of the scripts a page loaded: routes, chunks, and
    /// endpoint candidates.
    pub fn analyze_web_bundle(&self, endpoint: &str, target_id: &str) -> Result<String, String> {
        self.run_json(&[
            "analyze-web-bundle",
            endpoint,
            target_id,
            "--observation-ms",
            "3000",
        ])
    }

    /// Runs one browser scenario and returns REA's JSON output unchanged, so
    /// the caller can store the Evidence byte for byte.
    ///
    /// REA exits 1 with a JSON error on stdout when a capture fails; that
    /// error is returned as `Err` with REA's own message.
    pub fn capture_scenario(&self, scenario_file: &Path) -> Result<String, String> {
        let out = self
            .command()
            .arg("capture-browser-scenario")
            .arg(scenario_file)
            .args(["--format", "json"])
            .output()
            .map_err(|e| format!("could not run {}: {e}", self.describe()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            return Ok(stdout);
        }
        Err(rea_error_message(&stdout)
            .unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

/// REA's `message` and `code` from a JSON error, if the output is one.
pub fn rea_error_message(stdout: &str) -> Option<String> {
    let v: Value = serde_json::from_str(stdout).ok()?;
    let msg = v.get("message")?.as_str()?;
    let code = v.get("code").and_then(Value::as_str).unwrap_or("error");
    Some(format!("REA {code}: {msg}"))
}

/// The Evidence ID of a successful REA output.
pub fn evidence_id(stdout: &str) -> Option<String> {
    let v: Value = serde_json::from_str(stdout).ok()?;
    Some(v.get("evidence_id")?.as_str()?.to_string())
}

/// Builds one REA browser scenario per batch of `profile.pages`.
///
/// Every batch starts at the origin and settles before visiting its pages,
/// so batches are independent and step IDs are stable across captures —
/// REA aligns steps by ID when comparing.
pub fn scenario_batches(origin: &str, profile: &CaptureProfile, browser: &Path) -> Vec<Value> {
    let origin = origin.trim_end_matches('/');
    let batch = profile.batch_size.max(1);
    profile
        .pages
        .chunks(batch)
        .map(|pages| {
            let mut actions = vec![json!({
                "step_id": "settle", "action": "wait_for_timeout", "duration_ms": 3000
            })];
            for page in pages {
                let id = step_id(page);
                let url = format!("{origin}/{}", page.trim_start_matches('/'));
                actions.push(json!({
                    "step_id": format!("goto_{id}"), "action": "goto",
                    "destination": { "url": url }, "wait_until": "load", "timeout_ms": 15000
                }));
                actions.push(json!({
                    "step_id": format!("wait_{id}"), "action": "wait_for_timeout",
                    "duration_ms": profile.settle_ms
                }));
            }
            json!({
                "browser": { "mode": "launch", "executable_path": browser },
                "start_url": { "url": format!("{origin}/") },
                "actions": actions,
                "capture": { "events": ["network"] }
            })
        })
        .collect()
}

/// `#/track-result?id=1` → `track-result`.
fn step_id(page: &str) -> String {
    let p = page.trim_start_matches(['/', '#']).trim_start_matches('/');
    let p = p.split('?').next().unwrap_or_default();
    let id: String = p
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if id.is_empty() {
        "root".into()
    } else {
        id
    }
}

pub(crate) fn on_path(program: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file() || (cfg!(windows) && candidate.with_extension("cmd").is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(pages: &[&str], batch: usize) -> CaptureProfile {
        CaptureProfile {
            pages: pages.iter().map(|p| p.to_string()).collect(),
            batch_size: batch,
            settle_ms: 1500,
        }
    }

    #[test]
    fn pages_are_split_into_batches() {
        let b = scenario_batches(
            "http://h",
            &profile(&["#/a", "#/b", "#/c"], 2),
            Path::new("/chrome"),
        );
        assert_eq!(b.len(), 2);
        // settle + goto/wait per page
        assert_eq!(b[0]["actions"].as_array().unwrap().len(), 5);
        assert_eq!(b[1]["actions"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn urls_and_step_ids_come_from_the_page() {
        let b = scenario_batches(
            "http://h/",
            &profile(&["#/track-result?id=1"], 6),
            Path::new("/c"),
        );
        let goto = &b[0]["actions"][1];
        assert_eq!(goto["destination"]["url"], "http://h/#/track-result?id=1");
        assert_eq!(goto["step_id"], "goto_track-result");
        assert_eq!(goto["wait_until"], "load");
    }

    #[test]
    fn rea_json_errors_are_readable() {
        let msg = rea_error_message(
            r#"{"error":"x","code":"cleanup_incomplete","message":"Browser scenario cleanup could not be confirmed"}"#,
        );
        assert_eq!(
            msg.unwrap(),
            "REA cleanup_incomplete: Browser scenario cleanup could not be confirmed"
        );
    }
}

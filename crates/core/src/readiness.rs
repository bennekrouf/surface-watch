//! Is this machine ready to capture?
//!
//! The equivalent of AIS Monitor's sign-in check: before a watch runs, find
//! REA, Node.js and a Chrome-family browser, and record their versions for
//! the report. Known REA issues are listed with the workaround the app
//! applies, so a user is not surprised by them.

use crate::rea::{Rea, TESTED_REA_VERSION};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Ready,
    Warning,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub state: CheckState,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
    pub browser: Option<PathBuf>,
}

impl Report {
    pub fn ready(&self) -> bool {
        self.checks.iter().all(|c| c.state != CheckState::Missing)
    }
}

/// Known REA issues and the workaround applied for each.
pub const KNOWN_ISSUES: &[(&str, &str)] = &[
    (
        "Long crawls fail during browser cleanup",
        "Crawls are split into batches (6 pages by default).",
    ),
    (
        "REA 6.4.0 misses routes written as template literals (fixed upstream in morluto/rea#2805, unreleased)",
        "Suggest pages works with any REA but finds few routes until the fix ships; set SURFACE_WATCH_REA to try a newer build.",
    ),
    (
        "REA's static endpoint candidates are low-confidence guesses",
        "Only routes are used from the code; endpoints come from crawls and HAR imports.",
    ),
    (
        "compare-web-captures rejects REA's own output file",
        "Surface Watch reads either shape and does its own comparison.",
    ),
];

/// Runs every check. Blocking: spawns `node` and REA; call it off the UI thread.
pub fn check() -> Report {
    let mut checks = vec![];

    checks.push(match version_of("node", &["--version"]) {
        Some(v) if node_supported(&v) => ready("Node.js", v),
        Some(v) => Check {
            name: "Node.js",
            state: CheckState::Missing,
            detail: format!("{v} is too old: REA needs 22.19+, 24.11+ or 26+"),
        },
        None => missing(
            "Node.js",
            "not found on PATH: install Node.js 22.19 or newer",
        ),
    });

    let rea = Rea::detect();
    checks.push(match rea.version() {
        Ok(v) if v == TESTED_REA_VERSION => ready("REA", format!("{v} via {}", rea.describe())),
        Ok(v) => Check {
            name: "REA",
            state: CheckState::Warning,
            detail: format!(
                "{v} via {}; tested with {TESTED_REA_VERSION}",
                rea.describe()
            ),
        },
        Err(e) => missing("REA", &e),
    });

    let browser = find_browser();
    checks.push(match &browser {
        Some(p) => ready("Chrome-family browser", p.display().to_string()),
        None => missing(
            "Chrome-family browser",
            "install Google Chrome, Chromium or Edge",
        ),
    });

    Report { checks, browser }
}

fn ready(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        state: CheckState::Ready,
        detail: detail.into(),
    }
}

fn missing(name: &'static str, detail: &str) -> Check {
    Check {
        name,
        state: CheckState::Missing,
        detail: detail.to_string(),
    }
}

pub(crate) fn version_of(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// REA requires Node 22.x (>=22.19), 24.x (>=24.11), or 26+.
pub fn node_supported(version: &str) -> bool {
    let mut parts = version
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    match major {
        22 => minor >= 19,
        24 => minor >= 11,
        m => m >= 26,
    }
}

/// The first installed Chrome-family browser, by the usual install paths.
pub fn find_browser() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    } else if cfg!(windows) {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ]
    } else {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/microsoft-edge",
        ]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_versions_follow_reas_requirements() {
        assert!(node_supported("v22.19.0"));
        assert!(!node_supported("v22.18.1"));
        assert!(!node_supported("v23.9.0"));
        assert!(node_supported("v24.11.0"));
        assert!(!node_supported("v24.10.0"));
        assert!(!node_supported("v25.1.0"));
        assert!(node_supported("v26.7.0"));
    }
}

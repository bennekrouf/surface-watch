//! Engagement records: what the app persists, one folder per engagement.
//!
//! Raw REA Evidence is stored beside these records and never edited — its
//! IDs are content hashes, and a finding that cites `ev_…` must still be
//! able to point at the bytes that produced it. Surfaces and diffs are
//! derived from that Evidence and can always be rebuilt.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Engagement {
    pub id: String,
    pub client: String,
    pub name: String,
    pub window_start: Option<NaiveDate>,
    pub window_end: Option<NaiveDate>,
    /// Who authorised the test and where that is recorded. Shown before
    /// every capture, so nobody watches a target they may not test.
    pub authorisation: String,
    pub targets: Vec<Target>,
}

impl Engagement {
    /// Captures are refused outside the test window: repeated automated
    /// requests after the window closes can breach the engagement's scope.
    pub fn window_allows(&self, day: NaiveDate) -> bool {
        self.window_start.is_none_or(|s| day >= s) && self.window_end.is_none_or(|e| day <= e)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    /// Exact origin in scope, e.g. `https://shop.example.com`.
    pub origin: String,
    pub label: String,
    pub profile: CaptureProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureProfile {
    /// Paths or hash routes to visit, relative to the origin.
    pub pages: Vec<String>,
    /// Pages per REA scenario. REA's browser cleanup times out on long
    /// scenarios (about 35s in REA 6.4.0), so crawls are split.
    pub batch_size: usize,
    /// How long to let each page settle before moving on.
    pub settle_ms: u64,
}

impl Default for CaptureProfile {
    fn default() -> Self {
        CaptureProfile {
            pages: vec![],
            batch_size: 6,
            settle_ms: 1500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureSource {
    ReaScenario,
    Har,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: String,
    pub target_id: String,
    pub source: CaptureSource,
    pub taken_at: DateTime<Utc>,
    /// REA Evidence IDs, one per batch.
    pub evidence_ids: Vec<String>,
    /// Batch files, relative to the capture folder.
    pub files: Vec<String>,
    pub toolchain: Toolchain,
    /// Set on a second capture of the same build, taken right after
    /// `repeat_of` to find endpoints that come and go between runs.
    #[serde(default)]
    pub repeat_of: Option<String>,
    /// Where an imported capture came from, e.g. the HAR file name.
    #[serde(default)]
    pub label: Option<String>,
}

/// Exact versions behind a capture, for reproducibility in the report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Toolchain {
    pub rea: Option<String>,
    pub browser: Option<String>,
    pub node: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub target_id: String,
    pub title: String,
    pub severity: Severity,
    pub cvss_vector: Option<String>,
    pub description: String,
    pub remediation: String,
    /// The REA Evidence the finding rests on.
    pub evidence_ids: Vec<String>,
    /// Diff items it was promoted from, as `<capture-id>/<subject>`.
    pub diff_refs: Vec<String>,
    /// What a retest looks for in a capture. `None` for findings no capture
    /// can confirm or rule out; their status is set by hand.
    #[serde(default)]
    pub check: Option<Check>,
    pub created_at: DateTime<Utc>,
    pub retests: Vec<Retest>,
}

/// A condition a capture can confirm or rule out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Check {
    /// The endpoint is requested, optionally answering with `status`.
    Endpoint {
        method: String,
        path: String,
        status: Option<u16>,
    },
    /// The endpoint is requested with query parameter `param`.
    Param {
        method: String,
        path: String,
        param: String,
    },
    /// The endpoint answers without security header `header`.
    HeaderMissing {
        method: String,
        path: String,
        header: String,
    },
    /// The page loads from `origin`.
    ThirdParty { origin: String },
    /// HTML pages are served without security header `header`.
    PageHeaderMissing { header: String },
}

impl Finding {
    pub fn status(&self) -> RetestStatus {
        self.retests
            .last()
            .map(|r| r.status)
            .unwrap_or(RetestStatus::Open)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RetestStatus {
    /// Not retested yet.
    Open,
    Fixed,
    StillPresent,
    /// The retest capture did not cover the finding: neither fixed nor present.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Retest {
    pub capture_id: String,
    pub at: DateTime<Utc>,
    pub status: RetestStatus,
    /// What the capture showed, in words.
    #[serde(default)]
    pub note: String,
    /// Set by hand rather than evaluated from the capture.
    #[serde(default)]
    pub manual: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_bounds_are_inclusive() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let e = Engagement {
            id: "e".into(),
            client: "c".into(),
            name: "n".into(),
            window_start: Some(d("2026-10-01")),
            window_end: Some(d("2026-10-17")),
            authorisation: String::new(),
            targets: vec![],
        };
        assert!(e.window_allows(d("2026-10-01")));
        assert!(e.window_allows(d("2026-10-17")));
        assert!(!e.window_allows(d("2026-10-18")));
    }
}

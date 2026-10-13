//! Captures per target, and comparing two of them.
//!
//! On disk, inside the engagement folder:
//!
//! ```text
//! targets/<target-id>/captures/<capture-id>/capture.json      the Capture record
//! targets/<target-id>/captures/<capture-id>/batches/batch-N.json   REA output, unchanged
//! ```
//!
//! A capture can have repeats: a second run of the same build, taken right
//! after it. When either side of a comparison has a repeat, endpoints that
//! differ between the runs are marked unstable and left out of the diff.

use crate::capture;
use crate::diff::{self, SurfaceDiff};
use crate::har;
use crate::model::{Capture, CaptureSource, Engagement, Target, Toolchain};
use crate::rea::{self, Rea};
use crate::readiness;
use crate::store;
use crate::surface::Surface;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// The folder holding every capture of one target.
pub fn target_dir(engagement_id: &str, target_id: &str) -> PathBuf {
    store::engagement_dir(engagement_id)
        .join("targets")
        .join(target_id)
}

/// The folder holding one capture: its record and raw batches.
pub fn capture_dir(target_dir: &Path, capture_id: &str) -> PathBuf {
    target_dir.join("captures").join(capture_id)
}

fn batches_dir(target_dir: &Path, capture_id: &str) -> PathBuf {
    capture_dir(target_dir, capture_id).join("batches")
}

/// Every capture of the target, oldest first. Unreadable records are skipped.
pub fn list_captures(target_dir: &Path) -> Vec<Capture> {
    let Ok(entries) = std::fs::read_dir(target_dir.join("captures")) else {
        return vec![];
    };
    let mut out: Vec<Capture> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_to_string(e.path().join("capture.json")).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    out.sort_by(|a, b| (a.taken_at, &a.id).cmp(&(b.taken_at, &b.id)));
    out
}

/// The captures a user picks between: everything that is not a repeat.
pub fn primary_captures(captures: &[Capture]) -> Vec<&Capture> {
    captures.iter().filter(|c| c.repeat_of.is_none()).collect()
}

/// Default comparison: the baseline against the latest capture, both from
/// the same source.
///
/// Crawls are preferred: they repeat the same pages, so their differences
/// are changes in the app. A HAR covers whatever flow was recorded, so it is
/// only compared when the user picks it, or when there is nothing else.
pub fn default_pair(captures: &[Capture]) -> Option<(String, String)> {
    let primary = primary_captures(captures);
    for source in [CaptureSource::ReaScenario, CaptureSource::Har] {
        let same: Vec<&&Capture> = primary.iter().filter(|c| c.source == source).collect();
        if let [first, .., last] = same.as_slice() {
            return Some((first.id.clone(), last.id.clone()));
        }
    }
    match primary.as_slice() {
        [first, .., last] => Some((first.id.clone(), last.id.clone())),
        _ => None,
    }
}

/// The capture the Surface tab opens on: the latest crawl, since a HAR
/// usually covers one flow; the latest capture of any kind otherwise.
pub fn default_view(captures: &[Capture]) -> Option<&Capture> {
    let primary = primary_captures(captures);
    primary
        .iter()
        .rev()
        .find(|c| c.source == CaptureSource::ReaScenario)
        .or(primary.last())
        .copied()
}

/// The capture a stored target is compared against: its first capture from
/// `source`.
pub fn baseline_for(captures: &[Capture], source: CaptureSource) -> Option<&Capture> {
    primary_captures(captures)
        .into_iter()
        .find(|c| c.source == source)
}

/// The surface of one capture, with endpoints marked unstable against any
/// repeat of it.
pub fn load_surface(target_dir: &Path, origin: &str, capture_id: &str) -> Result<Surface, String> {
    let read = |id: &str| -> Result<Surface, String> {
        Ok(Surface::build(
            origin,
            &capture::read_dir(&batches_dir(target_dir, id))?,
        ))
    };
    let mut surface = read(capture_id)?;
    for repeat in list_captures(target_dir)
        .iter()
        .filter(|c| c.repeat_of.as_deref() == Some(capture_id))
    {
        surface.mark_unstable(&read(&repeat.id)?);
    }
    Ok(surface)
}

pub fn compare(
    target_dir: &Path,
    origin: &str,
    before_id: &str,
    after_id: &str,
) -> Result<SurfaceDiff, String> {
    let before = load_surface(target_dir, origin, before_id)?;
    let after = load_surface(target_dir, origin, after_id)?;
    Ok(diff::diff(&before, &after))
}

/// A capture ID from its time, unique within the target.
fn new_capture_id(target_dir: &Path, at: DateTime<Utc>, repeat: bool) -> String {
    let base = format!(
        "{}{}",
        at.format("%Y%m%d-%H%M%S"),
        if repeat { "-r" } else { "" }
    );
    let mut id = base.clone();
    let mut n = 2;
    while capture_dir(target_dir, &id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

fn save_capture(target_dir: &Path, capture: &Capture) -> Result<(), String> {
    let json = serde_json::to_string_pretty(capture).map_err(|e| e.to_string())?;
    store::write_private(
        &capture_dir(target_dir, &capture.id).join("capture.json"),
        &json,
    )
    .map_err(|e| e.to_string())
}

/// Copies an existing folder of REA capture batches in as a capture of
/// `target_id`. Files are copied byte for byte so their Evidence IDs stay
/// valid.
pub fn import_capture(
    target_dir: &Path,
    target_id: &str,
    source_dir: &Path,
    repeat_of: Option<String>,
    taken_at: DateTime<Utc>,
) -> Result<Capture, String> {
    // Validate before copying anything: a folder that is not a capture must
    // not leave a half-imported capture behind.
    capture::read_dir(source_dir)?;
    let mut sources: Vec<PathBuf> = std::fs::read_dir(source_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    sources.sort();

    let id = new_capture_id(target_dir, taken_at, repeat_of.is_some());
    let mut files = vec![];
    let mut evidence_ids = vec![];
    for (i, src) in sources.iter().enumerate() {
        let text = std::fs::read_to_string(src).map_err(|e| e.to_string())?;
        let name = format!("batch-{i}.json");
        store::write_private(&batches_dir(target_dir, &id).join(&name), &text)
            .map_err(|e| e.to_string())?;
        if let Some(ev) = rea::evidence_id(&text) {
            evidence_ids.push(ev);
        }
        files.push(format!("batches/{name}"));
    }
    let capture = Capture {
        id,
        target_id: target_id.to_string(),
        source: CaptureSource::ReaScenario,
        taken_at,
        evidence_ids,
        files,
        toolchain: Toolchain::default(),
        repeat_of,
        label: None,
    };
    save_capture(target_dir, &capture)?;
    Ok(capture)
}

/// Imports a HAR recording as a capture of `target_id`.
///
/// Only the redacted conversion is stored (see [`crate::har`]); the capture
/// cites the original file by its SHA-256. It is dated by when the HAR was
/// recorded, so it sorts among crawls by when the traffic happened.
pub fn import_har(target_dir: &Path, target_id: &str, har_path: &Path) -> Result<Capture, String> {
    let text =
        std::fs::read_to_string(har_path).map_err(|e| format!("{}: {e}", har_path.display()))?;
    let converted = har::convert(&text).map_err(|e| format!("{}: {e}", har_path.display()))?;
    let taken_at = converted.recorded_at.unwrap_or_else(Utc::now);
    let id = new_capture_id(target_dir, taken_at, false);
    let name = "batch-0.json";
    store::write_private(
        &batches_dir(target_dir, &id).join(name),
        &converted.capture.to_string(),
    )
    .map_err(|e| e.to_string())?;
    let capture = Capture {
        id,
        target_id: target_id.to_string(),
        source: CaptureSource::Har,
        taken_at,
        evidence_ids: vec![format!("har-sha256:{}", converted.sha256)],
        files: vec![format!("batches/{name}")],
        toolchain: Toolchain::default(),
        repeat_of: None,
        label: har_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
    };
    save_capture(target_dir, &capture)?;
    Ok(capture)
}

/// Captures that a deletion of `capture_id` removes: the capture itself and
/// its repeats, which mean nothing without it.
pub fn deletion_set(captures: &[Capture], capture_id: &str) -> Vec<String> {
    captures
        .iter()
        .filter(|c| c.id == capture_id || c.repeat_of.as_deref() == Some(capture_id))
        .map(|c| c.id.clone())
        .collect()
}

/// Deletes a capture and its repeats from disk. Returns the IDs removed.
///
/// Permanent: the raw REA output behind any finding that cites these
/// captures goes with them. The caller confirms with the user first.
pub fn delete_capture(target_dir: &Path, capture_id: &str) -> Result<Vec<String>, String> {
    let ids = deletion_set(&list_captures(target_dir), capture_id);
    if ids.is_empty() {
        return Err(format!("no capture {capture_id}"));
    }
    for id in &ids {
        // Never follow a path out of the captures folder.
        if id.contains(['/', '\\']) || id.starts_with('.') {
            return Err(format!("refusing an unexpected capture ID {id:?}"));
        }
        std::fs::remove_dir_all(capture_dir(target_dir, id))
            .map_err(|e| format!("could not delete capture {id}: {e}"))?;
    }
    Ok(ids)
}

/// True when two captures came from different sources, so their
/// differences may reflect what each covered rather than a change in the app.
pub fn mixed_sources(a: &Capture, b: &Capture) -> bool {
    a.source != b.source
}

/// Captures `target` through REA now. Blocking, and slow: one REA browser
/// run per batch of pages. Refused outside the engagement's test window.
pub fn capture_target(
    engagement: &Engagement,
    target: &Target,
    repeat_of: Option<String>,
) -> Result<Capture, String> {
    let now = Utc::now();
    if !engagement.window_allows(now.date_naive()) {
        return Err(format!(
            "{} is outside the test window of {}",
            now.date_naive(),
            engagement.name
        ));
    }
    if target.profile.pages.is_empty() {
        return Err(format!("{} has no pages to visit", target.label));
    }
    let browser = readiness::find_browser().ok_or("no Chrome-family browser found")?;
    let rea = Rea::detect();
    let dir = target_dir(&engagement.id, &target.id);
    let id = new_capture_id(&dir, now, repeat_of.is_some());

    let mut files = vec![];
    let mut evidence_ids = vec![];
    let mut browser_version = None;
    let scenarios = rea::scenario_batches(&target.origin, &target.profile, &browser);
    for (i, scenario) in scenarios.iter().enumerate() {
        let scenario_path = capture_dir(&dir, &id).join(format!("scenario-{i}.json.in"));
        store::write_private(&scenario_path, &scenario.to_string()).map_err(|e| e.to_string())?;
        let output = rea
            .capture_scenario(&scenario_path)
            .map_err(|e| format!("batch {} of {}: {e}", i + 1, scenarios.len()))?;
        let name = format!("batch-{i}.json");
        store::write_private(&batches_dir(&dir, &id).join(&name), &output)
            .map_err(|e| e.to_string())?;
        evidence_ids.extend(rea::evidence_id(&output));
        browser_version = browser_version.or_else(|| browser_version_of(&output));
        files.push(format!("batches/{name}"));
    }

    let capture = Capture {
        id,
        target_id: target.id.clone(),
        source: CaptureSource::ReaScenario,
        taken_at: now,
        evidence_ids,
        files,
        toolchain: Toolchain {
            rea: rea.version().ok(),
            browser: browser_version,
            node: readiness::version_of("node", &["--version"]),
        },
        repeat_of,
        label: None,
    };
    save_capture(&dir, &capture)?;
    Ok(capture)
}

fn browser_version_of(output: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(output).ok()?;
    let b = &v["normalized_result"]["browser"];
    Some(format!(
        "{} {}",
        b["product"].as_str()?,
        b["version"].as_str()?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::ChangeKind;
    use chrono::TimeZone;

    const ORIGIN: &str = "http://127.0.0.1:3012";

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/juice-shop")
            .join(name)
    }

    fn tmp_target(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sw-watch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn at(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 13, h, 0, 0).unwrap()
    }

    #[test]
    fn imported_captures_list_oldest_first_and_pair_baseline_with_latest() {
        let dir = tmp_target("list");
        let a = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        let b = import_capture(&dir, "t", &fixture("v20.3.0"), None, at(10)).unwrap();
        import_capture(
            &dir,
            "t",
            &fixture("v20.3.0-repeat"),
            Some(b.id.clone()),
            at(10),
        )
        .unwrap();

        let all = list_captures(&dir);
        assert_eq!(all.len(), 3);
        assert_eq!(primary_captures(&all).len(), 2);
        assert_eq!(default_pair(&all), Some((a.id, b.id)));
        assert_eq!(all[0].files.len(), 4);
    }

    #[test]
    fn a_repeat_suppresses_unstable_endpoints_in_the_comparison() {
        let dir = tmp_target("compare");
        let a = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        let b = import_capture(&dir, "t", &fixture("v20.3.0"), None, at(10)).unwrap();
        import_capture(
            &dir,
            "t",
            &fixture("v20.3.0-repeat"),
            Some(b.id.clone()),
            at(10),
        )
        .unwrap();

        let d = compare(&dir, ORIGIN, &a.id, &b.id).unwrap();
        assert_eq!(d.suppressed.len(), 1);
        assert_eq!(
            d.significant()
                .filter(|c| c.kind == ChangeKind::EndpointRemoved)
                .count(),
            2
        );
    }

    #[test]
    fn an_older_har_does_not_become_the_baseline_of_crawls() {
        let dir = tmp_target("baseline");
        let src = tmp_target("baseline-har");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("early.har"),
            r#"{"log":{"entries":[{"startedDateTime":"2026-10-13T07:00:00Z",
              "request":{"method":"GET","url":"http://127.0.0.1:3012/"},"response":{"status":200}}]}}"#,
        )
        .unwrap();
        import_har(&dir, "t", &src.join("early.har")).unwrap();
        let a = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        let b = import_capture(&dir, "t", &fixture("v20.3.0"), None, at(10)).unwrap();
        assert_eq!(
            default_pair(&list_captures(&dir)),
            Some((a.id.clone(), b.id.clone()))
        );

        // A HAR newer than every crawl does not take the "after" side either.
        std::fs::write(
            src.join("late.har"),
            r#"{"log":{"entries":[{"startedDateTime":"2026-10-13T23:00:00Z",
              "request":{"method":"POST","url":"http://127.0.0.1:3012/rest/chat"},"response":{"status":200}}]}}"#,
        )
        .unwrap();
        import_har(&dir, "t", &src.join("late.har")).unwrap();
        assert_eq!(default_pair(&list_captures(&dir)), Some((a.id, b.id)));
    }

    #[test]
    fn two_captures_in_the_same_second_get_distinct_ids() {
        let dir = tmp_target("ids");
        let a = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        let b = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn a_folder_that_is_not_a_capture_is_refused_without_leftovers() {
        let dir = tmp_target("refuse");
        let bogus = tmp_target("bogus-src");
        std::fs::create_dir_all(&bogus).unwrap();
        std::fs::write(bogus.join("x.json"), "{}").unwrap();
        assert!(import_capture(&dir, "t", &bogus, None, at(9)).is_err());
        assert!(list_captures(&dir).is_empty());
        assert!(!dir.join("captures").exists());
    }

    #[test]
    fn a_har_is_stored_redacted_and_dated_by_its_recording() {
        let dir = tmp_target("har");
        let src = tmp_target("har-src");
        std::fs::create_dir_all(&src).unwrap();
        let har = src.join("login-flow.har");
        std::fs::write(
            &har,
            r#"{"log":{"entries":[{"startedDateTime":"2026-10-12T08:00:00Z",
              "request":{"method":"POST","url":"http://127.0.0.1:3012/rest/user/login","headers":[{"name":"Cookie","value":"token=SECRET"}]},
              "response":{"status":401,"headers":[],"content":{"mimeType":"application/json","text":"SECRET"}}}]}}"#,
        )
        .unwrap();
        let c = import_har(&dir, "t", &har).unwrap();
        assert_eq!(c.source, CaptureSource::Har);
        assert_eq!(c.label.as_deref(), Some("login-flow.har"));
        assert!(c.evidence_ids[0].starts_with("har-sha256:"));
        assert_eq!(c.taken_at, at(8) - chrono::Duration::days(1));

        let stored = std::fs::read_to_string(
            dir.join("captures")
                .join(&c.id)
                .join("batches/batch-0.json"),
        )
        .unwrap();
        assert!(!stored.contains("SECRET"));
        let s = load_surface(&dir, ORIGIN, &c.id).unwrap();
        assert_eq!(s.endpoints.len(), 1);
    }

    #[test]
    fn deleting_a_capture_takes_its_repeats_and_nothing_else() {
        let dir = tmp_target("delete");
        let a = import_capture(&dir, "t", &fixture("v19.0.0"), None, at(9)).unwrap();
        let b = import_capture(&dir, "t", &fixture("v20.3.0"), None, at(10)).unwrap();
        let r = import_capture(
            &dir,
            "t",
            &fixture("v20.3.0-repeat"),
            Some(b.id.clone()),
            at(10),
        )
        .unwrap();

        let removed = delete_capture(&dir, &b.id).unwrap();
        assert_eq!(removed, vec![b.id.clone(), r.id.clone()]);
        let left: Vec<_> = list_captures(&dir).into_iter().map(|c| c.id).collect();
        assert_eq!(left, vec![a.id]);
        assert!(delete_capture(&dir, &b.id).is_err());
    }

    #[test]
    fn capturing_outside_the_test_window_is_refused() {
        let e = Engagement {
            id: "e".into(),
            client: "c".into(),
            name: "Closed".into(),
            window_start: None,
            window_end: Some(chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
            authorisation: String::new(),
            targets: vec![],
        };
        let t = Target {
            id: "t".into(),
            origin: ORIGIN.into(),
            label: "lab".into(),
            profile: Default::default(),
        };
        let err = capture_target(&e, &t, None).unwrap_err();
        assert!(err.contains("outside the test window"), "got {err}");
    }
}

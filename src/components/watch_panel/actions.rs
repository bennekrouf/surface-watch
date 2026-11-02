//! What the Watch panel's buttons do: capture the target, import a HAR recording, and promote
//! a change to a finding.

use super::*;
use std::path::PathBuf;

/// Captures the target, twice when `rep` asks for the stability check, then selects the new
/// capture as the later side of the comparison.
#[allow(clippy::too_many_arguments)]
pub(super) fn start_capture(
    e: Engagement,
    t: Target,
    rep: bool,
    batches: usize,
    mut run: Signal<Run>,
    mut after: Signal<Option<String>>,
    mut refresh: Signal<u32>,
) {
    let runs = if rep { 2 } else { 1 };
    // Measured on Juice Shop: about 14 seconds per batch of 6 pages.
    let minutes = (batches * runs * 15).div_ceil(60).max(1);
    run.set(Run::Running(format!(
        "Capturing {batches} batch{} of pages{}. This takes about {minutes} minute{}.",
        if batches == 1 { "" } else { "es" },
        if rep {
            ", twice for the stability check"
        } else {
            ""
        },
        if minutes > 1 { "s" } else { "" },
    )));
    spawn(async move {
        let res = tokio::task::spawn_blocking(move || {
            let c = watch::capture_target(&e, &t, None)?;
            if rep {
                watch::capture_target(&e, &t, Some(c.id.clone()))?;
            }
            Ok::<_, String>(c)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        match res {
            Ok(c) => {
                run.set(Run::Idle);
                after.set(Some(c.id));
                refresh += 1;
            }
            Err(e) => run.set(Run::Failed(e)),
        }
    });
}

/// Picks a HAR file, stores its redacted conversion as a capture, and shows it.
pub(super) fn import_har(
    dir: PathBuf,
    tid: String,
    mut run: Signal<Run>,
    mut after: Signal<Option<String>>,
    mut refresh: Signal<u32>,
) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Import a HAR recording")
            .add_filter("HAR", &["har", "json"])
            .pick_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        run.set(Run::Running(format!("Importing {}…", path.display())));
        let res = tokio::task::spawn_blocking(move || watch::import_har(&dir, &tid, &path))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        match res {
            Ok(c) => {
                run.set(Run::Done(format!(
                    "Imported {} as a capture recorded {}. Request body field names were kept; no cookie, header value, body value or query value was. The original file stays where it is.",
                    c.label.clone().unwrap_or_default(),
                    c.taken_at.with_timezone(&Local).format("%d %b %H:%M")
                )));
                after.set(Some(c.id));
                refresh += 1;
            }
            Err(e) => run.set(Run::Failed(e)),
        }
    });
}

/// Makes a finding of `change` between captures `b` and `a`, and says which.
pub(super) fn promote(
    eid: &str,
    tid: &str,
    change: Change,
    b: Capture,
    a: Capture,
    mut promote_msg: Signal<Option<Result<String, String>>>,
    mut findings_rev: Signal<u32>,
) {
    let res = findings::update(eid, |list| {
        let f = findings::promote(&change, tid, &b, &a, list);
        let id = f.id.clone();
        list.push(f);
        id
    });
    promote_msg.set(Some(res.map(|id| {
        format!(
            "{id} created from {}. Edit it in the Findings tab.",
            change.subject
        )
    })));
    findings_rev += 1;
}

use crate::components::suggest_panel::SuggestPanel;
use chrono::{Local, Utc};
use dioxus::prelude::*;
use surface_watch_core::{
    diff::{Change, ChangeKind, SurfaceDiff},
    findings,
    model::{Capture, CaptureSource, Engagement, Target},
    watch,
};

#[derive(Clone, PartialEq)]
enum Run {
    Idle,
    Running(String),
    Done(String),
    Failed(String),
}

/// Watch: capture a target, then compare any two of its captures.
///
/// Mounted with a key per target, so switching targets starts from fresh
/// state instead of carrying one target's selection into another.
#[component]
pub fn WatchPanel(
    engagement: Engagement,
    target: Target,
    /// The target's settings changed (pages added): reload engagements.
    on_changed: EventHandler<()>,
) -> Element {
    let dir = watch::target_dir(&engagement.id, &target.id);
    let mut refresh = use_signal(|| 0u32);
    let mut before = use_signal(|| Option::<String>::None);
    let mut after = use_signal(|| Option::<String>::None);
    let mut run = use_signal(|| Run::Idle);
    let mut twice = use_signal(|| true);
    let mut suggesting = use_signal(|| false);
    let mut findings_rev = use_signal(|| 0u32);
    let mut promote_msg = use_signal(|| Option::<Result<String, String>>::None);

    // Diff items already promoted, so their button reads "Promoted".
    let promoted = use_resource({
        let eid = engagement.id.clone();
        move || {
            let eid = eid.clone();
            let _ = findings_rev();
            async move {
                tokio::task::spawn_blocking(move || {
                    findings::load(&eid)
                        .into_iter()
                        .flat_map(|f| f.diff_refs)
                        .collect::<Vec<String>>()
                })
                .await
                .unwrap_or_default()
            }
        }
    });

    let captures = use_resource({
        let dir = dir.clone();
        move || {
            let dir = dir.clone();
            let _ = refresh();
            async move {
                tokio::task::spawn_blocking(move || watch::list_captures(&dir))
                    .await
                    .unwrap_or_default()
            }
        }
    });

    // The pair being compared: the user's choice while it still exists,
    // otherwise baseline against latest.
    let pair = use_memo(move || {
        let list = captures.read().clone().unwrap_or_default();
        let default = watch::default_pair(&list);
        let valid = |id: Option<String>| id.filter(|id| list.iter().any(|c| &c.id == id));
        let b = valid(before()).or(default.as_ref().map(|p| p.0.clone()));
        let a = valid(after()).or(default.as_ref().map(|p| p.1.clone()));
        b.zip(a)
    });

    let diff = use_resource({
        let dir = dir.clone();
        let origin = target.origin.clone();
        move || {
            let dir = dir.clone();
            let origin = origin.clone();
            let pair = pair();
            async move {
                let (b, a) = pair.filter(|(b, a)| b != a)?;
                Some(
                    tokio::task::spawn_blocking(move || watch::compare(&dir, &origin, &b, &a))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r),
                )
            }
        }
    });

    let window_open = engagement.window_allows(Utc::now().date_naive());
    let batches = target
        .profile
        .pages
        .len()
        .div_ceil(target.profile.batch_size.max(1));
    let list = captures.read().clone().unwrap_or_default();
    let primary: Vec<Capture> = watch::primary_captures(&list)
        .into_iter()
        .cloned()
        .collect();
    let has_repeat = |id: &str| list.iter().any(|c| c.repeat_of.as_deref() == Some(id));
    let running = matches!(run(), Run::Running(_));

    let start_capture = {
        let engagement = engagement.clone();
        let target = target.clone();
        move |_| {
            let (e, t, rep) = (engagement.clone(), target.clone(), twice());
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
    };

    // HAR import: pick a file, store its redacted conversion, show it.
    let import_har = {
        let dir = dir.clone();
        let tid = target.id.clone();
        move |_| {
            let (dir, tid) = (dir.clone(), tid.clone());
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
    };

    let (sel_before, sel_after) = pair().unzip();
    let find = |id: &Option<String>| list.iter().find(|c| Some(&c.id) == id.as_ref()).cloned();
    let (cap_before, cap_after) = (find(&sel_before), find(&sel_after));

    let on_promote = {
        let eid = engagement.id.clone();
        let tid = target.id.clone();
        let (b, a) = (cap_before.clone(), cap_after.clone());
        move |change: Change| {
            let (Some(b), Some(a)) = (b.clone(), a.clone()) else {
                return;
            };
            let res = findings::update(&eid, |list| {
                let f = findings::promote(&change, &tid, &b, &a, list);
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
    };
    let promoted_refs = promoted.read().clone().unwrap_or_default();
    let after_id = cap_after.as_ref().map(|c| c.id.clone()).unwrap_or_default();

    rsx! {
        section { class: "watch",
            div { class: "hd",
                div { class: "titles",
                    h2 { class: "mono", "{target.label}" }
                    div { class: "row",
                        span { class: "muted",
                            "{engagement.client} · {engagement.name} · {target.profile.pages.len()} pages in {batches} batches"
                        }
                        if window_open {
                            span { class: "pill ok", "test window open" }
                        } else {
                            span { class: "pill bad", "outside test window" }
                        }
                    }
                }
                div { class: "row",
                    button {
                        class: "btn",
                        disabled: running || suggesting() || !window_open,
                        title: "Find pages declared in the target's code, including ones no link points to",
                        onclick: move |_| suggesting.set(true),
                        "Suggest pages"
                    }
                    button {
                        class: "btn",
                        disabled: running,
                        title: "Add a HAR recorded in your browser, Caido, ZAP or mitmproxy, for flows the crawl cannot reach",
                        onclick: import_har,
                        "Import HAR"
                    }
                    label { class: "check-label",
                        input {
                            r#type: "checkbox",
                            id: "stability",
                            checked: twice(),
                            disabled: running,
                            onchange: move |e| twice.set(e.checked()),
                        }
                        "Stability check"
                    }
                    button {
                        class: "btn pri",
                        disabled: running || !window_open,
                        title: if window_open { "" } else { "Captures are refused outside the engagement's test window" },
                        onclick: start_capture,
                        if running {
                            "Capturing…"
                        } else {
                            "Capture now"
                        }
                    }
                }
            }

            if suggesting() {
                SuggestPanel {
                    engagement: engagement.clone(),
                    target: target.clone(),
                    on_close: move |_| suggesting.set(false),
                    on_added: move |n: usize| {
                        suggesting.set(false);
                        run.set(
                            Run::Done(
                                format!(
                                    "Added {n} page{} to the crawl. The next capture visits them.",
                                    if n == 1 { "" } else { "s" },
                                ),
                            ),
                        );
                        on_changed.call(());
                    },
                }
            }

            match run() {
                Run::Running(msg) | Run::Done(msg) => rsx! {
                    div { class: "notice", "{msg}" }
                },
                Run::Failed(err) => rsx! {
                    div { class: "notice bad-notice",
                        strong { "That did not work. " }
                        "{err}"
                    }
                },
                Run::Idle => rsx! {},
            }

            if captures.read().is_none() {
                p { class: "muted", "Loading captures…" }
            } else if primary.is_empty() {
                div { class: "placeholder",
                    p { "No captures yet." }
                    p { class: "muted",
                        "Capture now to take the baseline. Every later capture is compared with it."
                    }
                }
            } else {
                div { class: "pickers",
                    CapturePicker {
                        label: "Before",
                        captures: primary.clone(),
                        selected: sel_before.clone(),
                        baseline: watch::default_pair(&list).map(|p| p.0),
                        on_pick: move |id| before.set(Some(id)),
                    }
                    span { class: "arrow", "→" }
                    CapturePicker {
                        label: "After",
                        captures: primary.clone(),
                        selected: sel_after.clone(),
                        baseline: watch::default_pair(&list).map(|p| p.0),
                        on_pick: move |id| after.set(Some(id)),
                    }
                    div { class: "muted small",
                        if sel_before.as_deref().is_some_and(has_repeat)
                            || sel_after.as_deref().is_some_and(has_repeat)
                        {
                            "Stability check available: unstable endpoints are set aside."
                        } else {
                            "No stability check for these captures: timer-driven calls may show as changes."
                        }
                    }
                    if let (Some(b), Some(a)) = (&cap_before, &cap_after) {
                        if watch::mixed_sources(b, a) {
                            div { class: "notice warn-notice small",
                                "One of these captures is a HAR import and the other a crawl. Differences may come from what each covered, not from a change in the app."
                            }
                        }
                    }
                }

                if primary.len() == 1 {
                    div { class: "placeholder",
                        p { "Baseline taken." }
                        p { class: "muted",
                            "Capture again after the next deployment to see what changed."
                        }
                    }
                } else if sel_before == sel_after {
                    p { class: "muted",
                        "Before and after are the same capture. Pick two different captures to compare."
                    }
                } else {
                    match &*diff.read() {
                        None | Some(None) => rsx! {
                            p { class: "muted", "Comparing…" }
                        },
                        Some(Some(Err(e))) => rsx! {
                            div { class: "notice bad-notice", "Could not compare these captures: {e}" }
                        },
                        Some(Some(Ok(d))) => rsx! {
                            if let Some(msg) = promote_msg() {
                                match msg {
                                    Ok(m) => rsx! {
                                        div { class: "notice", "{m}" }
                                    },
                                    Err(e) => rsx! {
                                        div { class: "notice bad-notice", "Could not save the finding: {e}" }
                                    },
                                }
                            }
                            DiffView {
                                diff: d.clone(),
                                after_id: after_id.clone(),
                                promoted: promoted_refs.clone(),
                                on_promote,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn CapturePicker(
    label: &'static str,
    captures: Vec<Capture>,
    selected: Option<String>,
    baseline: Option<String>,
    on_pick: EventHandler<String>,
) -> Element {
    let id = format!("pick-{}", label.to_lowercase());
    rsx! {
        label { class: "picker", r#for: "{id}",
            span { class: "k", "{label}" }
            select { id: "{id}", onchange: move |e| on_pick.call(e.value()),
                for c in captures.iter().rev() {
                    option {
                        value: "{c.id}",
                        selected: selected.as_deref() == Some(c.id.as_str()),
                        {capture_label(c, baseline.as_deref() == Some(c.id.as_str()))}
                    }
                }
            }
        }
    }
}

pub fn capture_label(c: &Capture, is_baseline: bool) -> String {
    let when = c.taken_at.with_timezone(&Local).format("%d %b %H:%M:%S");
    let what = match c.source {
        CaptureSource::Har => format!("HAR {}", c.label.clone().unwrap_or_default()),
        CaptureSource::ReaScenario => format!("crawl, {} batches", c.files.len()),
    };
    let tag = if is_baseline { " · baseline" } else { "" };
    format!("{when} · {what}{tag}")
}

#[component]
fn DiffView(
    diff: SurfaceDiff,
    after_id: String,
    promoted: Vec<String>,
    on_promote: EventHandler<Change>,
) -> Element {
    let significant: Vec<Change> = diff.significant().cloned().collect();
    let info: Vec<Change> = diff
        .changes
        .iter()
        .filter(|c| c.kind.is_informational())
        .cloned()
        .collect();
    let mut kinds: Vec<ChangeKind> = significant.iter().map(|c| c.kind).collect();
    kinds.dedup();

    rsx! {
        div { class: "diff",
            div { class: "summary",
                span { class: "big", "{significant.len()}" }
                span {
                    " change{plural(significant.len())} · API endpoints {diff.endpoints_before} → {diff.endpoints_after}"
                }
            }
            if significant.is_empty() {
                div { class: "nochange",
                    span { class: "pill ok", "no attack-surface change" }
                    span { class: "muted",
                        "Same endpoints, parameters, statuses, security headers and third-party origins."
                    }
                }
            }
            for kind in kinds {
                {
                    let items: Vec<&Change> = significant
                        .iter()
                        .filter(|c| c.kind == kind)
                        .collect();
                    rsx! {
                        div { class: "diffgrp", key: "{kind.label()}",
                            div { class: "gh",
                                span { "{kind.label()}" }
                                span { class: "pill {tone(kind)}", "{items.len()}" }
                            }
                            for c in items {
                                div { class: "li", key: "{c.subject}",
                                    span { class: "sg {sign_class(kind)}", "{sign(kind)}" }
                                    div { class: "w",
                                        span { class: "mono", "{c.subject}" }
                                        span { class: "note", {detail(c)} }
                                    }
                                    {
                                        let done = promoted.contains(&format!("{after_id}/{}", c.subject));
                                        let change = (*c).clone();
                                        rsx! {
                                            button {
                                                class: "btn",
                                                disabled: done,
                                                onclick: move |_| on_promote.call(change.clone()),
                                                if done {
                                                    "Promoted"
                                                } else {
                                                    "Promote to finding"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !diff.suppressed.is_empty() {
                div { class: "suppressed",
                    span { class: "pill info", "suppressed" }
                    for k in diff.suppressed.iter() {
                        span { class: "mono", key: "{k}", "{k}" }
                    }
                    span { class: "muted", "seen in only one of two captures of the same build" }
                }
            }
            for c in info.iter() {
                details { class: "info", key: "{c.subject}",
                    summary {
                        "Scripts changed: {c.added.len()} added, {c.removed.len()} removed (build hashes ignored)"
                    }
                    div { class: "mono small",
                        for s in c.added.iter() {
                            div { key: "+{s}", "+ {s}" }
                        }
                        for s in c.removed.iter() {
                            div { key: "-{s}", "− {s}" }
                        }
                    }
                }
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn sign(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::EndpointAdded | ChangeKind::ThirdPartyAdded => "+",
        ChangeKind::EndpointRemoved | ChangeKind::ThirdPartyRemoved => "−",
        _ => "~",
    }
}

fn sign_class(kind: ChangeKind) -> &'static str {
    match sign(kind) {
        "+" => "add",
        "−" => "del",
        _ => "chg",
    }
}

/// New surface needs testing first; removals are usually good news.
fn tone(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::EndpointAdded | ChangeKind::ThirdPartyAdded => "bad",
        ChangeKind::EndpointRemoved | ChangeKind::ThirdPartyRemoved => "info",
        _ => "warn",
    }
}

/// What the change means for the tester, plus the exact values.
fn detail(c: &Change) -> String {
    let values = |label: &str, v: &[String]| {
        if v.is_empty() {
            String::new()
        } else {
            format!(" {label}: {}.", v.join(", "))
        }
    };
    let (what, extra) = match c.kind {
        ChangeKind::EndpointAdded => (
            "Requested in the after capture, not in the before one: test it like any new endpoint.",
            values("Status", &c.added),
        ),
        ChangeKind::EndpointRemoved => (
            "Not requested in the after capture. It may still answer on the server: check it directly.",
            String::new(),
        ),
        ChangeKind::StatusChanged => (
            "The response status changed.",
            values("Now", &c.added) + &values("Was", &c.removed),
        ),
        ChangeKind::ParamsChanged => (
            "Query parameters changed: test new ones for injection and data exposure.",
            values("Added", &c.added) + &values("Removed", &c.removed),
        ),
        ChangeKind::SecurityHeadersChanged => (
            "Security headers on this response changed.",
            values("Added", &c.added) + &values("Removed", &c.removed),
        ),
        ChangeKind::ThirdPartyAdded => (
            "Loaded in the after capture: review what it serves and why.",
            String::new(),
        ),
        ChangeKind::ThirdPartyRemoved => ("Not loaded in the after capture.", String::new()),
        ChangeKind::ScriptsChanged => ("", String::new()),
    };
    format!("{what}{extra}")
}

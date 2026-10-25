use crate::components::os::reveal;
use chrono::{Local, Utc};
use dioxus::prelude::*;
use std::path::PathBuf;
use surfacewatch_core::{
    findings::{self, severity_label, status_label},
    model::{Check, Engagement, Finding, Retest, RetestStatus, Severity},
    watch,
};

const SEVERITIES: [Severity; 5] = [
    Severity::Critical,
    Severity::High,
    Severity::Medium,
    Severity::Low,
    Severity::Info,
];

#[derive(Clone, PartialEq)]
enum Notice {
    Info(String),
    Exported(PathBuf),
    Error(String),
}

/// Findings of one engagement: edit, retest against the latest capture,
/// export.
///
/// Mounted with a key per engagement. Every edit is written straight to the
/// engagement's findings file, so there is no save button to forget.
#[component]
pub fn FindingsPanel(engagement: Engagement, selected_target: Option<String>) -> Element {
    let eid = engagement.id.clone();
    let mut list = use_signal({
        let eid = eid.clone();
        move || findings::load(&eid)
    });
    let mut notice = use_signal(|| Option::<Notice>::None);
    let mut only_target = use_signal(|| true);
    let mut confirm_delete = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);

    let retest_all = {
        let engagement = engagement.clone();
        move |_| {
            let e = engagement.clone();
            busy.set(true);
            spawn(async move {
                let eid = e.id.clone();
                let res =
                    tokio::task::spawn_blocking(move || findings::retest_latest(&e, |_| true))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r);
                busy.set(false);
                list.set(findings::load(&eid));
                notice.set(Some(match res {
                    Ok(0) => Notice::Info(
                        "Nothing to retest: no finding has an automatic check with a capture to test against.".into(),
                    ),
                    Ok(n) => Notice::Info(format!(
                        "Retested {n} finding{} against the latest capture of each target.",
                        if n == 1 { "" } else { "s" }
                    )),
                    Err(e) => Notice::Error(format!("Retest failed: {e}")),
                }));
            });
        }
    };

    let export = {
        let engagement = engagement.clone();
        move |_| {
            let captures: Vec<_> = engagement
                .targets
                .iter()
                .flat_map(|t| watch::list_captures(&watch::target_dir(&engagement.id, &t.id)))
                .collect();
            notice.set(Some(
                match findings::export_markdown(&engagement, &captures) {
                    Ok(path) => Notice::Exported(path),
                    Err(e) => Notice::Error(format!("Export failed: {e}")),
                },
            ));
        }
    };

    let all = list.read().clone();
    let filtering = only_target() && selected_target.is_some();
    let mut shown: Vec<Finding> = all
        .iter()
        .filter(|f| !filtering || Some(&f.target_id) == selected_target.as_ref())
        .cloned()
        .collect();
    shown.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.id.cmp(&b.id)));
    let count = |s: RetestStatus| all.iter().filter(|f| f.status() == s).count();
    let target_label = |tid: &str| {
        engagement
            .targets
            .iter()
            .find(|t| t.id == tid)
            .map(|t| t.label.clone())
            .unwrap_or_else(|| tid.to_string())
    };

    rsx! {
        section { class: "findings",
            div { class: "hd",
                div { class: "titles",
                    h2 { "Findings" }
                    div { class: "row",
                        span { class: "muted", "{engagement.client} · {engagement.name} · {all.len()} total" }
                        span { class: "pill bad", "{count(RetestStatus::Open)} open" }
                        span { class: "pill warn", "{count(RetestStatus::StillPresent)} still present" }
                        span { class: "pill ok", "{count(RetestStatus::Fixed)} fixed" }
                        span { class: "pill info", "{count(RetestStatus::Unknown)} unknown" }
                    }
                }
                div { class: "row",
                    if selected_target.is_some() {
                        label { class: "check-label",
                            input {
                                r#type: "checkbox",
                                id: "only-target",
                                checked: only_target(),
                                onchange: move |e| only_target.set(e.checked()),
                            }
                            "Selected target only"
                        }
                    }
                    button {
                        class: "btn",
                        disabled: busy() || all.is_empty(),
                        onclick: retest_all,
                        if busy() { "Retesting…" } else { "Retest all" }
                    }
                    button {
                        class: "btn pri",
                        disabled: all.is_empty(),
                        onclick: export,
                        "Export Markdown"
                    }
                }
            }

            match notice() {
                Some(Notice::Info(m)) => rsx! { div { class: "notice", "{m}" } },
                Some(Notice::Error(m)) => rsx! { div { class: "notice bad-notice", "{m}" } },
                Some(Notice::Exported(path)) => rsx! {
                    div { class: "notice row",
                        span { "Exported to " }
                        span { class: "mono", "{path.display()}" }
                        button { class: "btn", onclick: move |_| reveal(&path), "Show in folder" }
                    }
                },
                None => rsx! {},
            }

            if all.is_empty() {
                div { class: "placeholder",
                    p { "No findings yet." }
                    p { class: "muted", "Promote a change from the Watch tab. The finding keeps the evidence of both captures and, where a capture can tell, how to retest it." }
                }
            } else if shown.is_empty() {
                p { class: "muted", "No findings for this target. Untick “Selected target only” to see all {all.len()}." }
            }

            for f in shown {
                {
                    let fid = f.id.clone();
                    // Each handler owns its copy of the IDs; signals are Copy.
                    let ids = || (eid.clone(), fid.clone());
                    let (title_ids, sev_ids, desc_ids, rem_ids, cvss_ids, manual_ids, del_ids) =
                        (ids(), ids(), ids(), ids(), ids(), ids(), ids());
                    let engagement_for_retest = engagement.clone();
                    let retest_fid = fid.clone();
                    let target_dir = watch::target_dir(&engagement.id, &f.target_id);
                    let deleting = confirm_delete().as_deref() == Some(fid.as_str());
                    let created = f.created_at.with_timezone(&Local).format("%d %b %Y %H:%M").to_string();
                    let sev_class = severity_label(f.severity).to_lowercase();
                    let cvss = f.cvss_vector.clone().unwrap_or_default();
                    rsx! {
                        article { class: "finding", key: "{f.id}",
                            div { class: "top",
                                span { class: "mono fid", "{f.id}" }
                                input {
                                    class: "title-input",
                                    id: "title-{f.id}",
                                    value: "{f.title}",
                                    onchange: move |e| apply(&title_ids.0, &title_ids.1, list, notice, |x| x.title = e.value()),
                                }
                                select {
                                    id: "sev-{f.id}",
                                    class: "sev sev-{sev_class}",
                                    onchange: move |e| {
                                        let v = e.value();
                                        if let Some(sev) = SEVERITIES.into_iter().find(|s| severity_label(*s) == v) {
                                            apply(&sev_ids.0, &sev_ids.1, list, notice, |x| x.severity = sev);
                                        }
                                    },
                                    for s in SEVERITIES {
                                        option { value: severity_label(s), selected: s == f.severity, {severity_label(s)} }
                                    }
                                }
                                span { class: "pill {status_tone(f.status())}", {status_label(f.status())} }
                            }
                            div { class: "meta muted",
                                span { class: "mono", {target_label(&f.target_id)} }
                                span { " · created {created}" }
                            }

                            div { class: "fields",
                                label { r#for: "desc-{f.id}", "Description" }
                                textarea {
                                    id: "desc-{f.id}",
                                    rows: 3,
                                    value: "{f.description}",
                                    onchange: move |e| apply(&desc_ids.0, &desc_ids.1, list, notice, |x| x.description = e.value()),
                                }
                                label { r#for: "rem-{f.id}", "Remediation" }
                                textarea {
                                    id: "rem-{f.id}",
                                    rows: 2,
                                    placeholder: "What the client should change",
                                    value: "{f.remediation}",
                                    onchange: move |e| apply(&rem_ids.0, &rem_ids.1, list, notice, |x| x.remediation = e.value()),
                                }
                                label { r#for: "cvss-{f.id}", "CVSS vector" }
                                input {
                                    id: "cvss-{f.id}",
                                    class: "mono",
                                    placeholder: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N",
                                    value: "{cvss}",
                                    onchange: move |e| {
                                        let v = e.value().trim().to_string();
                                        apply(&cvss_ids.0, &cvss_ids.1, list, notice, |x| {
                                            x.cvss_vector = Some(v).filter(|s| !s.is_empty())
                                        });
                                    },
                                }
                            }

                            div { class: "ev",
                                details { class: "evidence",
                                    summary { "Evidence · {f.evidence_ids.len()} REA record{plural(f.evidence_ids.len())}" }
                                    for id in f.evidence_ids.iter() {
                                        div { class: "mono evid", key: "{id}", "{id}" }
                                    }
                                }
                                div { class: "k", "Retest" }
                                p { class: "muted small", {describe_check(f.check.as_ref())} }
                                for (i, r) in f.retests.iter().enumerate().rev() {
                                    RetestRow { key: "{i}", retest: r.clone() }
                                }
                            }

                            div { class: "actions row",
                                if f.check.is_some() {
                                    button {
                                        class: "btn",
                                        onclick: move |_| {
                                            let e = engagement_for_retest.clone();
                                            let fid = retest_fid.clone();
                                            let res = findings::retest_latest(&e, |x| x.id == fid);
                                            list.set(findings::load(&e.id));
                                            notice.set(Some(match res {
                                                Ok(0) => Notice::Info(format!("{fid}: this target has no capture to retest against yet.")),
                                                Ok(_) => Notice::Info(format!("{fid} retested against the latest capture.")),
                                                Err(err) => Notice::Error(format!("Retest failed: {err}")),
                                            }));
                                        },
                                        "Retest against latest capture"
                                    }
                                }
                                select {
                                    id: "manual-{f.id}",
                                    onchange: move |e| {
                                        let status = match e.value().as_str() {
                                            "fixed" => RetestStatus::Fixed,
                                            "still-present" => RetestStatus::StillPresent,
                                            "unknown" => RetestStatus::Unknown,
                                            _ => return,
                                        };
                                        let latest = watch::primary_captures(&watch::list_captures(&target_dir))
                                            .last()
                                            .map(|c| c.id.clone())
                                            .unwrap_or_default();
                                        apply(&manual_ids.0, &manual_ids.1, list, notice, |x| {
                                            findings::record(x, Retest {
                                                capture_id: latest,
                                                at: Utc::now(),
                                                status,
                                                note: "Set by hand.".into(),
                                                manual: true,
                                            })
                                        });
                                    },
                                    option { value: "", selected: true, "Set status by hand…" }
                                    option { value: "fixed", "Fixed" }
                                    option { value: "still-present", "Still present" }
                                    option { value: "unknown", "Unknown" }
                                }
                                span { class: "spacer" }
                                if deleting {
                                    span { class: "muted small", "Delete {fid}? This cannot be undone." }
                                    button { class: "btn", onclick: move |_| confirm_delete.set(None), "Keep" }
                                    button {
                                        class: "btn danger",
                                        onclick: move |_| {
                                            let (eid, id) = &del_ids;
                                            match findings::update(eid, |all| all.retain(|x| &x.id != id)) {
                                                Ok(()) => list.set(findings::load(eid)),
                                                Err(e) => notice.set(Some(Notice::Error(format!("Could not delete: {e}")))),
                                            }
                                            confirm_delete.set(None);
                                        },
                                        "Delete"
                                    }
                                } else {
                                    button {
                                        class: "btn ghost",
                                        onclick: move |_| confirm_delete.set(Some(fid.clone())),
                                        "Delete"
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

#[component]
fn RetestRow(retest: Retest) -> Element {
    let r = retest;
    rsx! {
        div { class: "retest",
            span { class: "muted small", {r.at.with_timezone(&Local).format("%d %b %H:%M").to_string()} }
            span { class: "pill {status_tone(r.status)}", {status_label(r.status)} }
            span { class: "small", "{r.note}" }
            if r.manual {
                span { class: "pill info", "by hand" }
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

fn status_tone(s: RetestStatus) -> &'static str {
    match s {
        RetestStatus::Open => "bad",
        RetestStatus::StillPresent => "warn",
        RetestStatus::Fixed => "ok",
        RetestStatus::Unknown => "info",
    }
}

/// The one write path for every edit: update on disk, then show what is on
/// disk.
fn apply(
    eid: &str,
    fid: &str,
    mut list: Signal<Vec<Finding>>,
    mut notice: Signal<Option<Notice>>,
    f: impl FnOnce(&mut Finding),
) {
    let res = findings::update(eid, |all| {
        if let Some(x) = all.iter_mut().find(|x| x.id == fid) {
            f(x);
        }
    });
    match res {
        Ok(()) => list.set(findings::load(eid)),
        Err(e) => notice.set(Some(Notice::Error(format!("Could not save: {e}")))),
    }
}

/// What an automatic retest looks for, in words.
fn describe_check(check: Option<&Check>) -> String {
    match check {
        None => "No capture can confirm or rule this out: set its status by hand after checking it directly.".into(),
        Some(Check::Endpoint { method, path, status: Some(s) }) => {
            format!("Still present while {method} {path} answers {s}.")
        }
        Some(Check::Endpoint { method, path, status: None }) => {
            format!("Still present while the page requests {method} {path}.")
        }
        Some(Check::Param { method, path, param }) => {
            format!("Still present while {method} {path} is called with `{param}`.")
        }
        Some(Check::HeaderMissing { method, path, header }) => {
            format!("Still present while {method} {path} answers without {header}.")
        }
        Some(Check::PageHeaderMissing { header }) => {
            format!("Still present while pages are served without {header}.")
        }
        Some(Check::ThirdParty { origin }) => format!("Still present while the page loads from {origin}."),
    }
}

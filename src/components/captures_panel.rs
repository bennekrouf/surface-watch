use crate::components::os::reveal;
use chrono::Local;
use dioxus::prelude::*;
use surface_watch_core::{
    findings,
    flags::{endpoint_flags, Flag},
    model::{Capture, CaptureSource, Engagement, Finding, Target},
    watch,
};

/// What a capture held, computed from its surface.
#[derive(Clone, PartialEq)]
struct Summary {
    endpoints: usize,
    flagged: usize,
    third_parties: usize,
    events_dropped: u64,
}

/// Captures: every capture of the target, where it came from, what it holds,
/// which findings rely on it, and deleting one.
///
/// Mounted with a key per target.
#[component]
pub fn CapturesPanel(engagement: Engagement, target: Target) -> Element {
    let dir = watch::target_dir(&engagement.id, &target.id);
    let mut refresh = use_signal(|| 0u32);
    let mut confirm = use_signal(|| Option::<String>::None);
    let mut msg = use_signal(|| Option::<Result<String, String>>::None);
    let all_findings = use_signal({
        let eid = engagement.id.clone();
        move || findings::load(&eid)
    });

    let rows = use_resource({
        let dir = dir.clone();
        let origin = target.origin.clone();
        move || {
            let (dir, origin) = (dir.clone(), origin.clone());
            let _ = refresh();
            async move {
                tokio::task::spawn_blocking(move || {
                    watch::list_captures(&dir)
                        .into_iter()
                        .rev()
                        .map(|c| {
                            let summary =
                                watch::load_surface(&dir, &origin, &c.id).map(|s| Summary {
                                    endpoints: s.endpoints.len(),
                                    flagged: s
                                        .endpoints
                                        .iter()
                                        .filter(|(k, ep)| {
                                            endpoint_flags(k, ep, s.unstable.contains(*k))
                                                .iter()
                                                .any(Flag::is_lead)
                                        })
                                        .count(),
                                    third_parties: s.third_party_origins.len(),
                                    events_dropped: s.events_dropped,
                                });
                            (c, summary)
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default()
            }
        }
    });

    let list: Vec<(Capture, Result<Summary, String>)> = rows.read().clone().unwrap_or_default();
    let captures: Vec<Capture> = list.iter().map(|(c, _)| c.clone()).collect();
    // `captures` is newest first for display; the baseline rule reads oldest first.
    let oldest_first: Vec<Capture> = captures.iter().rev().cloned().collect();
    let baseline = watch::default_pair(&oldest_first).map(|p| p.0);
    let crawls = captures
        .iter()
        .filter(|c| c.source == CaptureSource::ReaScenario)
        .count();
    let hars = captures.len() - crawls;

    rsx! {
        section { class: "captures",
            div { class: "hd",
                div { class: "titles",
                    h2 { class: "mono", "{target.label}" }
                    span { class: "muted", "{captures.len()} capture{plural(captures.len())} · {crawls} from crawls, repeats included · {hars} HAR import{plural(hars)}" }
                }
                button {
                    class: "btn",
                    onclick: {
                        let dir = dir.clone();
                        move |_| reveal(&dir.join("captures"))
                    },
                    "Show folder"
                }
            }

            if let Some(m) = msg() {
                match m {
                    Ok(t) => rsx! { div { class: "notice", "{t}" } },
                    Err(e) => rsx! { div { class: "notice bad-notice", "{e}" } },
                }
            }

            if rows.read().is_none() {
                p { class: "muted", "Reading captures…" }
            } else if list.is_empty() {
                div { class: "placeholder",
                    p { "No captures yet." }
                    p { class: "muted", "Capture the target or import a HAR in the Watch tab." }
                }
            }

            for (c, summary) in list.iter().cloned() {
                {
                    let cited: Vec<Finding> = findings::citing(&all_findings.read(), &c).into_iter().cloned().collect();
                    let removes = watch::deletion_set(&captures, &c.id);
                    let repeats = removes.len().saturating_sub(1);
                    let confirming = confirm().as_deref() == Some(c.id.as_str());
                    let parent = c.repeat_of.as_ref().and_then(|p| captures.iter().find(|x| &x.id == p)).cloned();
                    let folder = watch::capture_dir(&dir, &c.id);
                    let (dir_del, id_del, id_ask) = (dir.clone(), c.id.clone(), c.id.clone());
                    let when = c.taken_at.with_timezone(&Local).format("%d %b %Y %H:%M:%S").to_string();
                    let tools: Vec<String> = [
                        c.toolchain.rea.as_ref().map(|v| format!("REA {v}")),
                        c.toolchain.browser.clone(),
                        c.toolchain.node.as_ref().map(|v| format!("Node {v}")),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    rsx! {
                        article { class: "capture", key: "{c.id}",
                            div { class: "top row",
                                span { class: "when", "{when}" }
                                match c.source {
                                    CaptureSource::ReaScenario => rsx! { span { class: "pill info", "crawl" } },
                                    CaptureSource::Har => rsx! { span { class: "pill warn", "HAR" } },
                                }
                                if let Some(l) = &c.label {
                                    span { class: "mono", "{l}" }
                                }
                                if baseline.as_deref() == Some(c.id.as_str()) {
                                    span { class: "pill ok", "baseline" }
                                }
                                if let Some(p) = &parent {
                                    span { class: "pill info",
                                        {format!("stability repeat of {}", p.taken_at.with_timezone(&Local).format("%H:%M:%S"))}
                                    }
                                }
                                span { class: "muted mono small cid", "{c.id}" }
                            }
                            match &summary {
                                Ok(s) => rsx! {
                                    div { class: "stats",
                                        span { strong { "{s.endpoints}" } " endpoints" }
                                        span { strong { "{s.flagged}" } " flagged" }
                                        span { strong { "{s.third_parties}" } " third-party origins" }
                                        span { strong { "{c.files.len()}" } if c.files.len() == 1 { " batch" } else { " batches" } }
                                        if s.events_dropped > 0 {
                                            span { class: "pill warn", "{s.events_dropped} events dropped by REA" }
                                        }
                                    }
                                },
                                Err(e) => rsx! { div { class: "notice bad-notice small", "This capture cannot be read: {e}" } },
                            }
                            div { class: "kv",
                                span { class: "k", "Cited by" }
                                if cited.is_empty() {
                                    span { class: "muted small", "No finding" }
                                } else {
                                    span { class: "row",
                                        for f in cited.iter() {
                                            span { class: "mono fid", key: "{f.id}", title: "{f.title}", "{f.id}" }
                                        }
                                    }
                                }
                                span { class: "k", "Tools" }
                                span { class: "small",
                                    if tools.is_empty() {
                                        span { class: "muted",
                                            {match c.source {
                                                CaptureSource::Har => "Recorded outside Surface Watch",
                                                CaptureSource::ReaScenario => "Not recorded (imported capture)",
                                            }}
                                        }
                                    } else {
                                        "{tools.join(\", \")}"
                                    }
                                }
                            }
                            details { class: "evidence",
                                summary { "Evidence · {c.evidence_ids.len()} record{plural(c.evidence_ids.len())}" }
                                for id in c.evidence_ids.iter() {
                                    div { class: "mono evid", key: "{id}", "{id}" }
                                }
                            }
                            div { class: "actions row",
                                button { class: "btn", onclick: move |_| reveal(&folder), "Show in folder" }
                                span { class: "spacer" }
                                if confirming {
                                    span { class: "small warn-text",
                                        {delete_warning(repeats, &cited)}
                                    }
                                    button { class: "btn", onclick: move |_| confirm.set(None), "Keep" }
                                    button {
                                        class: "btn danger",
                                        onclick: move |_| {
                                            msg.set(Some(match watch::delete_capture(&dir_del, &id_del) {
                                                Ok(ids) => Ok(format!("Deleted {} capture{}.", ids.len(), plural(ids.len()))),
                                                Err(e) => Err(format!("Could not delete: {e}")),
                                            }));
                                            confirm.set(None);
                                            refresh += 1;
                                        },
                                        "Delete permanently"
                                    }
                                } else {
                                    button { class: "btn ghost", onclick: move |_| confirm.set(Some(id_ask.clone())), "Delete" }
                                }
                            }
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

/// What a deletion takes with it, in words.
fn delete_warning(repeats: usize, cited: &[Finding]) -> String {
    let mut parts = vec!["This cannot be undone.".to_string()];
    if repeats > 0 {
        parts.push(format!(
            "Its stability repeat{} will be deleted too.",
            plural(repeats)
        ));
    }
    if !cited.is_empty() {
        let ids: Vec<&str> = cited.iter().map(|f| f.id.as_str()).collect();
        parts.push(format!(
            "{} cite{} it: they keep its evidence IDs, but the raw data behind them will be gone.",
            ids.join(", "),
            if ids.len() == 1 { "s" } else { "" }
        ));
    }
    parts.join(" ")
}

use crate::components::os::reveal;
use crate::components::watch_panel::capture_label;
use dioxus::prelude::*;
use std::path::PathBuf;
use surface_watch_core::{
    api0, findings,
    flags::{endpoint_flags, page_flags, Flag},
    model::{Capture, Engagement, Finding, Target},
    surface::{Endpoint, EndpointKey, Surface},
    watch,
};

/// Surface: what one capture shows of a target, with the entries worth a
/// tester's attention flagged and one click from becoming a finding.
///
/// Mounted with a key per target.
#[component]
pub fn SurfacePanel(engagement: Engagement, target: Target) -> Element {
    let dir = watch::target_dir(&engagement.id, &target.id);
    let mut chosen = use_signal(|| Option::<String>::None);
    let mut filter = use_signal(String::new);
    let mut leads_only = use_signal(|| false);
    let mut existing = use_signal({
        let eid = engagement.id.clone();
        move || findings::load(&eid)
    });
    let mut msg = use_signal(|| Option::<Result<String, String>>::None);

    let captures = use_resource({
        let dir = dir.clone();
        move || {
            let dir = dir.clone();
            async move {
                tokio::task::spawn_blocking(move || watch::list_captures(&dir))
                    .await
                    .unwrap_or_default()
            }
        }
    });

    // The capture shown: the user's choice, else the latest crawl.
    let current = use_memo(move || {
        let list = captures.read().clone().unwrap_or_default();
        let primary: Vec<Capture> = watch::primary_captures(&list)
            .into_iter()
            .cloned()
            .collect();
        chosen()
            .and_then(|id| primary.iter().find(|c| c.id == id).cloned())
            .or_else(|| watch::default_view(&list).cloned())
    });

    let surface = use_resource({
        let dir = dir.clone();
        let origin = target.origin.clone();
        move || {
            let dir = dir.clone();
            let origin = origin.clone();
            let capture = current();
            async move {
                let c = capture?;
                Some(
                    tokio::task::spawn_blocking(move || watch::load_surface(&dir, &origin, &c.id))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r),
                )
            }
        }
    });

    let list = captures.read().clone().unwrap_or_default();
    let primary: Vec<Capture> = watch::primary_captures(&list)
        .into_iter()
        .cloned()
        .collect();
    let cap = current();

    // Creates the finding unless one with the same retest check exists.
    let mut own_backend = use_signal(|| false);
    let mut exported = use_signal(|| Option::<Result<(PathBuf, String), String>>::None);
    let export_api0 = {
        let (e, t) = (engagement.clone(), target.clone());
        move |_| {
            let opts = api0::Options {
                forward_identity: own_backend(),
            };
            exported.set(Some(api0::export(&e, &t, opts).map(|(path, out)| {
                let summary = format!(
                    "{} endpoint{} exported, {} with request body fields; {} static or unstable call{} left out.",
                    out.endpoints,
                    if out.endpoints == 1 { "" } else { "s" },
                    out.with_body_fields,
                    out.skipped.len(),
                    if out.skipped.len() == 1 { "" } else { "s" },
                );
                (path, summary)
            })));
        }
    };

    let promote = {
        let eid = engagement.id.clone();
        let tid = target.id.clone();
        move |(c, key, ep, flag): (Capture, Option<EndpointKey>, Option<Endpoint>, Option<Flag>)| {
            let endpoint = key.as_ref().zip(ep.as_ref());
            let res = findings::update(&eid, |all| {
                let f = findings::promote_surface(&tid, &c, endpoint, flag.as_ref(), all);
                let id = f.id.clone();
                let title = f.title.clone();
                all.push(f);
                format!("{id} created: {title}. Edit it in the Findings tab.")
            });
            msg.set(Some(res));
            existing.set(findings::load(&eid));
        }
    };

    rsx! {
        section { class: "surface",
            div { class: "hd",
                div { class: "titles",
                    h2 { class: "mono", "{target.label}" }
                    span { class: "muted", "{engagement.client} · {engagement.name}" }
                }
                if !primary.is_empty() {
                    div { class: "row",
                        label {
                            class: "check-label",
                            title: "Send api0's identity headers to this backend. Only for a backend you run: they must not reach anyone else's.",
                            input {
                                r#type: "checkbox",
                                id: "own-backend",
                                checked: own_backend(),
                                onchange: move |e| own_backend.set(e.checked()),
                            }
                            "My own backend"
                        }
                        button {
                            class: "btn",
                            title: "Write the API calls seen from this frontend (latest crawl and every HAR import) as an api0 catalog",
                            onclick: export_api0,
                            "Export to api0"
                        }
                    }
                    label { class: "picker", r#for: "surface-capture",
                        span { class: "k", "Capture" }
                        select {
                            id: "surface-capture",
                            onchange: move |e| chosen.set(Some(e.value())),
                            for c in primary.iter().rev() {
                                option {
                                    value: "{c.id}",
                                    selected: cap.as_ref().map(|x| &x.id) == Some(&c.id),
                                    {capture_label(c, false)}
                                }
                            }
                        }
                    }
                }
            }

            match exported() {
                Some(Ok((path, summary))) => rsx! {
                    div { class: "notice api0-notice",
                        p { "{summary}" }
                        p { class: "mono small", "{path.display()}" }
                        p { class: "small warn-text",
                            "Uploading with `api0 api upload` replaces every API in the workspace it goes to, and the api0 command line always uploads to your default workspace. Upload into a workspace kept for imports, or add the group from the api0 console."
                        }
                        div { class: "row",
                            button { class: "btn", onclick: move |_| reveal(&path), "Show in folder" }
                            button { class: "btn ghost", onclick: move |_| exported.set(None), "Dismiss" }
                        }
                    }
                },
                Some(Err(e)) => rsx! { div { class: "notice bad-notice", "Could not export: {e}" } },
                None => rsx! {},
            }

            if let Some(m) = msg() {
                match m {
                    Ok(t) => rsx! { div { class: "notice", "{t}" } },
                    Err(e) => rsx! { div { class: "notice bad-notice", "Could not save the finding: {e}" } },
                }
            }

            if captures.read().is_none() {
                p { class: "muted", "Loading captures…" }
            } else if primary.is_empty() {
                div { class: "placeholder",
                    p { "No captures yet." }
                    p { class: "muted", "Capture the target in the Watch tab; its surface shows up here." }
                }
            } else {
                match (&*surface.read(), cap.clone()) {
                    (Some(Some(Ok(s))), Some(c)) => rsx! {
                        SurfaceView {
                            surface: s.clone(),
                            capture: c,
                            target_id: target.id.clone(),
                            existing: existing(),
                            filter: filter(),
                            leads_only: leads_only(),
                            on_filter: move |v| filter.set(v),
                            on_leads_only: move |v| leads_only.set(v),
                            on_promote: promote.clone(),
                        }
                    },
                    (Some(Some(Err(e))), _) => rsx! { div { class: "notice bad-notice", "Could not read this capture: {e}" } },
                    _ => rsx! { p { class: "muted", "Reading the capture…" } },
                }
            }
        }
    }
}

type Promotion = (Capture, Option<EndpointKey>, Option<Endpoint>, Option<Flag>);

#[component]
fn SurfaceView(
    surface: Surface,
    capture: Capture,
    target_id: String,
    existing: Vec<Finding>,
    filter: String,
    leads_only: bool,
    on_filter: EventHandler<String>,
    on_leads_only: EventHandler<bool>,
    on_promote: EventHandler<Promotion>,
) -> Element {
    let s = surface;
    // Promoted already: a finding with the same retest check exists.
    let promoted = |endpoint: Option<(&EndpointKey, &Endpoint)>, flag: Option<&Flag>| {
        let candidate = findings::promote_surface(&target_id, &capture, endpoint, flag, &existing);
        candidate.check.is_some() && existing.iter().any(|f| f.check == candidate.check)
    };

    let page = page_flags(&s);
    let needle = filter.to_lowercase();
    let mut rows: Vec<(EndpointKey, Endpoint, Vec<Flag>)> = s
        .endpoints
        .iter()
        .map(|(k, ep)| {
            (
                k.clone(),
                ep.clone(),
                endpoint_flags(k, ep, s.unstable.contains(k)),
            )
        })
        .filter(|(k, _, _)| needle.is_empty() || k.to_string().to_lowercase().contains(&needle))
        .filter(|(_, _, f)| !leads_only || f.iter().any(Flag::is_lead))
        .collect();
    // Flagged endpoints first, most serious first; then by path.
    let rank = |flags: &[Flag]| match flags.iter().find(|f| f.is_lead()).map(Flag::tone) {
        Some("bad") => 0,
        Some(_) => 1,
        None => 2,
    };
    rows.sort_by(|a, b| (rank(&a.2), &a.0).cmp(&(rank(&b.2), &b.0)));
    let lead_count = s
        .endpoints
        .iter()
        .filter(|(k, ep)| {
            endpoint_flags(k, ep, s.unstable.contains(*k))
                .iter()
                .any(Flag::is_lead)
        })
        .count();

    rsx! {
        div { class: "row",
            span { class: "pill info", "{s.endpoints.len()} endpoints" }
            span { class: if lead_count > 0 { "pill bad" } else { "pill ok" }, "{lead_count} flagged" }
            span { class: "pill info", "{s.scripts.len()} scripts" }
            span { class: "pill info", "{s.assets_ignored} assets hidden" }
            span { class: if s.third_party_origins.is_empty() { "pill ok" } else { "pill warn" }, "{s.third_party_origins.len()} third-party origins" }
            if s.events_dropped > 0 {
                span { class: "pill warn", "{s.events_dropped} events dropped by REA" }
            }
        }

        if !page.is_empty() {
            div { class: "diffgrp",
                div { class: "gh", span { "Pages" } span { class: "pill warn", "{page.len()}" } }
                for f in page {
                    {
                        let done = promoted(None, Some(&f));
                        let payload = (capture.clone(), None, None, Some(f.clone()));
                        rsx! {
                            div { class: "li", key: "{f.label()}",
                                span { class: "sg chg", "!" }
                                div { class: "w",
                                    span { "Served without {f.label().trim_start_matches(\"no \")}" }
                                    span { class: "note", {f.explanation()} }
                                }
                                button {
                                    class: "btn",
                                    disabled: done,
                                    onclick: move |_| on_promote.call(payload.clone()),
                                    if done { "Promoted" } else { "Promote to finding" }
                                }
                            }
                        }
                    }
                }
            }
        }

        div { class: "row filters",
            input {
                id: "surface-filter",
                class: "filter",
                placeholder: "Filter endpoints",
                value: "{filter}",
                oninput: move |e| on_filter.call(e.value()),
            }
            label { class: "check-label",
                input {
                    r#type: "checkbox",
                    id: "leads-only",
                    checked: leads_only,
                    onchange: move |e| on_leads_only.call(e.checked()),
                }
                "Flagged only"
            }
            span { class: "muted small", "{rows.len()} shown" }
        }

        div { class: "tablewrap",
            table { class: "t",
                thead {
                    tr {
                        th { "Endpoint" }
                        th { "Status" }
                        th { "Query params" }
                        th { "Flags" }
                        th {}
                    }
                }
                tbody {
                    for (k, ep, fl) in rows {
                        {
                            let lead = fl.iter().find(|f| f.is_lead()).cloned();
                            let done = promoted(Some((&k, &ep)), lead.as_ref());
                            let statuses: Vec<String> = ep.statuses.iter().map(u16::to_string).collect();
                            let params: Vec<String> = ep.params.iter().cloned().collect();
                            let headers: Vec<String> = ep.security_headers.iter().cloned().collect();
                            let payload = (capture.clone(), Some(k.clone()), Some(ep.clone()), lead.clone());
                            rsx! {
                                tr { key: "{k}",
                                    td { class: "m", title: "Security headers: {headers.join(\", \")}", "{k}" }
                                    td { class: "m", "{statuses.join(\", \")}" }
                                    td { class: "m", if params.is_empty() { span { class: "muted", "–" } } else { "{params.join(\", \")}" } }
                                    td {
                                        div { class: "flags",
                                            for f in fl.iter() {
                                                span { class: "pill {f.tone()}", title: "{f.explanation()}", {f.label()} }
                                            }
                                        }
                                    }
                                    td { class: "act",
                                        button {
                                            class: if lead.is_some() { "btn" } else { "btn ghost" },
                                            disabled: done,
                                            title: if lead.is_some() { "Create a finding from this flag" } else { "Create a finding for this endpoint" },
                                            onclick: move |_| on_promote.call(payload.clone()),
                                            if done { "Promoted" } else { "Promote" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        div { class: "side-lists",
            div {
                h3 { "Third-party origins" }
                if s.third_party_origins.is_empty() {
                    p { class: "muted small", "None: every request stayed on {s.origin}." }
                }
                for o in s.third_party_origins.iter() {
                    div { class: "mono", key: "{o}", "{o}" }
                }
            }
            details { class: "info",
                summary { "Scripts: {s.scripts.len()} (build hashes collapsed)" }
                div { class: "mono small",
                    for sc in s.scripts.iter() { div { key: "{sc}", "{sc}" } }
                }
            }
        }
    }
}

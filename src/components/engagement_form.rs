use chrono::Local;
use dioxus::prelude::*;
use surface_watch_core::{
    model::Engagement,
    setup::{self, EngagementDraft, TargetDraft},
};

/// Which form is open.
#[derive(Clone, PartialEq, Debug)]
pub enum FormMode {
    NewEngagement,
    /// Add a target to the engagement with this ID.
    AddTarget(String),
}

/// New engagement (with its first target), or a new target for an existing
/// engagement. Validation is the core's, so the form and the CLI agree.
#[component]
pub fn EngagementForm(
    mode: FormMode,
    /// The engagement a target is added to; `None` for a new engagement.
    engagement: Option<Engagement>,
    /// Called with `(engagement id, target id)` once saved.
    on_done: EventHandler<(String, String)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let mut client = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut start = use_signal(move || today.clone());
    let mut end = use_signal(String::new);
    let mut auth = use_signal(String::new);
    let mut origin = use_signal(String::new);
    let mut label = use_signal(String::new);
    let mut pages = use_signal(|| "/".to_string());
    let mut batch = use_signal(|| TargetDraft::default().batch_size.to_string());
    let mut settle = use_signal(|| TargetDraft::default().settle_ms.to_string());
    let mut error = use_signal(|| Option::<String>::None);

    let new_engagement = mode == FormMode::NewEngagement;
    let origin_hint = match origin().trim() {
        "" => None,
        typed => Some(setup::normalize_origin(typed)),
    };
    let page_count = setup::parse_pages(&pages()).map(|p| p.len()).unwrap_or(0);
    let batch_n: usize = batch().trim().parse().unwrap_or(0);

    let submit = {
        let mode = mode.clone();
        move |_| {
            let target = TargetDraft {
                origin: origin(),
                label: label(),
                pages: pages(),
                batch_size: match batch().trim().parse() {
                    Ok(n) => n,
                    Err(_) => return error.set(Some("Pages per batch must be a number.".into())),
                },
                settle_ms: match settle().trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        return error
                            .set(Some("Settle time must be a number of milliseconds.".into()))
                    }
                },
            };
            let res = match &mode {
                FormMode::NewEngagement => setup::create_engagement(
                    &EngagementDraft {
                        client: client(),
                        name: name(),
                        window_start: start(),
                        window_end: end(),
                        authorisation: auth(),
                    },
                    &target,
                )
                .map(|e| (e.id.clone(), e.targets[0].id.clone())),
                FormMode::AddTarget(eid) => {
                    setup::add_target(eid, &target).map(|t| (eid.clone(), t.id))
                }
            };
            match res {
                Ok(ids) => on_done.call(ids),
                Err(e) => error.set(Some(e)),
            }
        }
    };

    rsx! {
        section { class: "form",
            div { class: "hd",
                div { class: "titles",
                    if new_engagement {
                        h2 { "New engagement" }
                        span { class: "muted", "A client engagement with its test window, authorisation and first target." }
                    } else if let Some(e) = &engagement {
                        h2 { "Add a target" }
                        span { class: "muted", "To {e.client} · {e.name}" }
                    }
                }
            }

            if new_engagement {
                fieldset {
                    legend { "Engagement" }
                    div { class: "fields",
                        label { r#for: "f-client", "Client" }
                        input { id: "f-client", placeholder: "Acme Corp", value: "{client}", oninput: move |e| client.set(e.value()) }
                        label { r#for: "f-name", "Name" }
                        input { id: "f-name", placeholder: "Retail web app, Q4 test", value: "{name}", oninput: move |e| name.set(e.value()) }
                        label { r#for: "f-start", "Test window" }
                        div { class: "row",
                            input { id: "f-start", r#type: "date", value: "{start}", oninput: move |e| start.set(e.value()) }
                            span { class: "muted", "to" }
                            input { id: "f-end", r#type: "date", value: "{end}", oninput: move |e| end.set(e.value()) }
                            span { class: "muted small", "Captures are refused outside these dates. Leave a date empty for no bound." }
                        }
                        label { r#for: "f-auth", "Authorisation" }
                        div { class: "stack",
                            textarea {
                                id: "f-auth",
                                rows: 2,
                                placeholder: "SOW-2026-114, signed by J. Doe (CISO) on 28 Sep 2026",
                                value: "{auth}",
                                oninput: move |e| auth.set(e.value()),
                            }
                            span { class: "muted small", "Who authorised this test, and where that is recorded. Required: shown with every capture and in the report." }
                        }
                    }
                }
            }

            fieldset {
                legend { if new_engagement { "First target" } else { "Target" } }
                div { class: "fields",
                    label { r#for: "f-origin", "Origin" }
                    div { class: "stack",
                        input {
                            id: "f-origin",
                            class: "mono",
                            placeholder: "https://shop.acme.example",
                            value: "{origin}",
                            oninput: move |e| origin.set(e.value()),
                        }
                        match origin_hint {
                            Some(Ok(o)) => rsx! { span { class: "small ok-text", "In scope: {o} and nothing else." } },
                            Some(Err(e)) => rsx! { span { class: "small warn-text", "{e}" } },
                            None => rsx! { span { class: "muted small", "Scheme and host only. Requests to any other origin are counted as third parties." } },
                        }
                    }
                    label { r#for: "f-label", "Label" }
                    input { id: "f-label", placeholder: "Optional, shown in the sidebar instead of the origin", value: "{label}", oninput: move |e| label.set(e.value()) }
                    label { r#for: "f-pages", "Pages to visit" }
                    div { class: "stack",
                        textarea {
                            id: "f-pages",
                            class: "mono",
                            rows: 6,
                            value: "{pages}",
                            oninput: move |e| pages.set(e.value()),
                        }
                        span { class: "muted small",
                            "One per line: a path (/login) or a hash route (#/login). "
                            if page_count > 0 && batch_n > 0 {
                                {format!("{page_count} page{} in {} batch{}.", if page_count == 1 { "" } else { "s" }, page_count.div_ceil(batch_n), if page_count.div_ceil(batch_n) == 1 { "" } else { "es" })}
                            }
                            " Flows behind a login or a form need a HAR import."
                        }
                    }
                }
                details { class: "info advanced",
                    summary { "Crawl settings" }
                    div { class: "fields",
                        label { r#for: "f-batch", "Pages per batch" }
                        div { class: "row",
                            input { id: "f-batch", class: "num", r#type: "number", min: "1", max: "20", value: "{batch}", oninput: move |e| batch.set(e.value()) }
                            span { class: "muted small", "REA 6.4.0 fails on long browser runs; 6 is safe." }
                        }
                        label { r#for: "f-settle", "Settle time (ms)" }
                        div { class: "row",
                            input { id: "f-settle", class: "num", r#type: "number", min: "200", max: "30000", step: "100", value: "{settle}", oninput: move |e| settle.set(e.value()) }
                            span { class: "muted small", "How long each page gets to make its requests." }
                        }
                    }
                }
            }

            if let Some(e) = error() {
                div { class: "notice bad-notice", "{e}" }
            }

            div { class: "row form-actions",
                button { class: "btn", onclick: move |_| on_cancel.call(()), "Cancel" }
                button {
                    class: "btn pri",
                    onclick: submit,
                    if new_engagement { "Create engagement" } else { "Add target" }
                }
            }
        }
    }
}

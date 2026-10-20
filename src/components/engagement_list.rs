use dioxus::prelude::*;
use surface_watch_core::{licence, model::Engagement, store};

use crate::components::pro::{Pro, ProButton};

/// The selected target, as `(engagement id, target id)`.
pub type Selection = Option<(String, String)>;

/// Sidebar: engagements, each with its targets.
#[component]
pub fn EngagementList(
    engagements: Signal<Vec<Engagement>>,
    selected: Signal<Selection>,
    on_new: EventHandler<()>,
    on_add_target: EventHandler<String>,
) -> Element {
    let mut engagements = engagements;
    let mut selected = selected;
    let data_dir = store::data_dir().display().to_string();
    let mut pro = use_context::<Pro>();

    rsx! {
        aside { class: "side",
            div { class: "brand",
                span { class: "brand-name", "Surface Watch" }
                span { class: "brand-version", title: "Installed version",
                    {concat!("v", env!("CARGO_PKG_VERSION"))}
                }
                // A link, not a button: the webview hands it to the browser.
                a {
                    class: "feedback-link",
                    href: "{crate::feedback::contact_url()}",
                    target: "_blank",
                    title: "Ideas, bugs, questions: send them to the person who makes Surface Watch",
                    "Feedback"
                }
                ProButton {}
            }
            div { class: "grp row-between",
                span { "Engagements" }
                button {
                    class: "link",
                    title: "Reload engagements from disk",
                    onclick: move |_| engagements.set(store::load_engagements()),
                    "Reload"
                }
            }
            button {
                class: "btn new-eng",
                onclick: move |_| {
                    // Without Pro, a second engagement asks for Pro first.
                    let n = engagements.read().len();
                    if licence::engagement_limit(&pro.status.read(), n).is_some() {
                        pro.open.set(true);
                    } else {
                        on_new.call(());
                    }
                },
                "New engagement"
            }
            if engagements.read().is_empty() {
                div { class: "empty",
                    p { "No engagements yet." }
                    p { class: "muted",
                        "Each engagement is a folder in {data_dir}/engagements, holding its targets, captures and findings."
                    }
                }
            }
            for e in engagements.read().iter() {
                div { class: "eng", key: "{e.id}",
                    div { class: "eng-name", "{e.client} · {e.name}" }
                    if e.targets.is_empty() {
                        div { class: "muted sub-empty", "No targets" }
                    }
                    for t in e.targets.iter() {
                        {
                            let sel = (e.id.clone(), t.id.clone());
                            let on = selected.read().as_ref() == Some(&sel);
                            rsx! {
                                button {
                                    key: "{t.id}",
                                    class: if on { "it on" } else { "it" },
                                    onclick: move |_| selected.set(Some(sel.clone())),
                                    span { class: "mono", "{t.label}" }
                                }
                            }
                        }
                    }
                    {
                        let eid = e.id.clone();
                        rsx! {
                            button {
                                class: "link add-target",
                                onclick: move |_| on_add_target.call(eid.clone()),
                                "+ Add target"
                            }
                        }
                    }
                }
            }
        }
    }
}

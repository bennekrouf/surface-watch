use crate::components::{
    captures_panel::CapturesPanel,
    engagement_form::{EngagementForm, FormMode},
    engagement_list::{EngagementList, Selection},
    findings_panel::FindingsPanel,
    placeholder_panel::PlaceholderPanel,
    readiness_panel::ReadinessPanel,
    surface_panel::SurfacePanel,
    watch_panel::WatchPanel,
};
use dioxus::prelude::*;
use surface_watch_core::store;

/// The per-target tabs, in display order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Surface,
    Watch,
    Findings,
    Captures,
    Readiness,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Surface,
        Tab::Watch,
        Tab::Findings,
        Tab::Captures,
        Tab::Readiness,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Surface => "Surface",
            Tab::Watch => "Watch",
            Tab::Findings => "Findings",
            Tab::Captures => "Captures",
            Tab::Readiness => "Readiness",
        }
    }
}

#[component]
pub fn MainScreen() -> Element {
    let mut engagements = use_signal(store::load_engagements);
    // Open on the first target, if there is one: that is what a returning
    // user came back for. With nothing to watch yet, start on Readiness.
    let first_target: Selection = engagements
        .peek()
        .iter()
        .find_map(|e| e.targets.first().map(|t| (e.id.clone(), t.id.clone())));
    let mut tab = use_signal(|| {
        if first_target.is_some() {
            Tab::Watch
        } else {
            Tab::Readiness
        }
    });
    let mut selected = use_signal(|| first_target);
    let mut form = use_signal(|| Option::<FormMode>::None);

    // The selected engagement and target, if they still exist after a reload.
    let current = selected().and_then(|(eid, tid)| {
        let e = engagements.read().iter().find(|e| e.id == eid)?.clone();
        let t = e.targets.iter().find(|t| t.id == tid)?.clone();
        Some((e, t))
    });

    rsx! {
        div { class: "app",
            EngagementList {
                engagements,
                selected,
                on_new: move |_| form.set(Some(FormMode::NewEngagement)),
                on_add_target: move |eid| form.set(Some(FormMode::AddTarget(eid))),
            }
            if let Some(mode) = form() {
                main { class: "main",
                    div { class: "panel",
                        EngagementForm {
                            key: "{mode:?}",
                            engagement: match &mode {
                                FormMode::AddTarget(eid) => {
                                    engagements.read().iter().find(|e| &e.id == eid).cloned()
                                }
                                FormMode::NewEngagement => None,
                            },
                            mode: mode.clone(),
                            on_done: move |(eid, tid): (String, String)| {
                                engagements.set(store::load_engagements());
                                selected.set(Some((eid, tid)));
                                tab.set(Tab::Watch);
                                form.set(None);
                            },
                            on_cancel: move |_| form.set(None),
                        }
                    }
                }
            } else {
                main { class: "main",
                    nav { class: "tabs",
                        for t in Tab::ALL {
                            button {
                                class: if tab() == t { "tab on" } else { "tab" },
                                onclick: move |_| tab.set(t),
                                "{t.label()}"
                            }
                        }
                    }
                    div { class: "panel",
                        match tab() {
                            Tab::Readiness => rsx! {
                                ReadinessPanel {}
                            },
                            Tab::Surface => {
                                match current.clone() {
                                    Some((e, t)) => {
                                        let key = format!("{}/{}", e.id, t.id);
                                        rsx! {
                                            SurfacePanel { key: "{key}", engagement: e, target: t }
                                        }
                                    }
                                    None => rsx! {
                                        PlaceholderPanel {
                                            title: "Surface",
                                            body: "Select a target in the sidebar to see what its latest capture shows.",
                                        }
                                    },
                                }
                            }
                            Tab::Watch => {
                                match current.clone() {
                                    Some((e, t)) => {
                                        let key = format!("{}/{}", e.id, t.id);
                                        rsx! {
                                            WatchPanel {
                                                key: "{key}",
                                                engagement: e,
                                                target: t,
                                                on_changed: move |_| engagements.set(store::load_engagements()),
                                            }
                                        }
                                    }
                                    None => rsx! {
                                        PlaceholderPanel {
                                            title: "Watch",
                                            body: "Select a target in the sidebar to capture it and compare its captures.",
                                        }
                                    },
                                }
                            }
                            Tab::Findings => {
                                match current.clone() {
                                    Some((e, t)) => {
                                        let key = e.id.clone();
                                        rsx! {
                                            FindingsPanel { key: "{key}", engagement: e, selected_target: Some(t.id) }
                                        }
                                    }
                                    None => rsx! {
                                        PlaceholderPanel {
                                            title: "Findings",
                                            body: "Select a target in the sidebar to see its engagement's findings.",
                                        }
                                    },
                                }
                            }
                            Tab::Captures => {
                                match current.clone() {
                                    Some((e, t)) => {
                                        let key = format!("{}/{}", e.id, t.id);
                                        rsx! {
                                            CapturesPanel { key: "{key}", engagement: e, target: t }
                                        }
                                    }
                                    None => rsx! {
                                        PlaceholderPanel {
                                            title: "Captures",
                                            body: "Select a target in the sidebar to see its captures.",
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

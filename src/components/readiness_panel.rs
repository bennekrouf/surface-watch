use dioxus::prelude::*;
use surface_watch_core::readiness::{self, CheckState};

/// Toolchain checks plus the known REA issues and their workarounds.
#[component]
pub fn ReadinessPanel() -> Element {
    let mut report = use_resource(|| async {
        // Spawns `node` and REA (possibly via npx, which can take seconds):
        // never on the UI thread.
        tokio::task::spawn_blocking(readiness::check).await.ok()
    });

    rsx! {
        section { class: "readiness",
            div { class: "hd",
                h2 { "Readiness" }
                button { class: "btn", onclick: move |_| report.restart(), "Check again" }
            }
            match &*report.read() {
                None => rsx! { p { class: "muted", "Checking Node.js, REA and the browser…" } },
                Some(None) => rsx! { p { class: "bad", "The readiness check could not run." } },
                Some(Some(r)) => rsx! {
                    for c in r.checks.iter() {
                        div { class: "check",
                            div { class: "w",
                                span { "{c.name}" }
                                span { class: "muted", "{c.detail}" }
                            }
                            match c.state {
                                CheckState::Ready => rsx! { span { class: "pill ok", "ready" } },
                                CheckState::Warning => rsx! { span { class: "pill warn", "check version" } },
                                CheckState::Missing => rsx! { span { class: "pill bad", "missing" } },
                            }
                        }
                    }
                },
            }
            h3 { "Known REA issues" }
            for (issue, workaround) in readiness::KNOWN_ISSUES.iter() {
                div { class: "check",
                    div { class: "w",
                        span { "{issue}" }
                        span { class: "muted", "{workaround}" }
                    }
                    span { class: "pill warn", "workaround on" }
                }
            }
        }
    }
}

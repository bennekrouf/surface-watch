//! The Surface Watch Pro window: what the free version does, the licence on
//! this computer, and where to paste a key. The limit itself lives in the
//! core (`licence::engagement_limit`), so the CLI keeps it too.

use dioxus::prelude::*;
use surface_watch_core::licence::{self, Status, FREE_ENGAGEMENTS};

use crate::components::os;

/// Shared by every part of the UI that can open the window or needs to know
/// whether the app is licensed. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct Pro {
    pub open: Signal<bool>,
    pub status: Signal<Status>,
}

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
/// Nothing at all in a build that can't check licences.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let (label, class, title) = match &status {
        Status::Unavailable => return rsx! {},
        Status::Pro(l) => (
            "✦ PRO",
            "pro-btn active",
            format!("Surface Watch Pro, licensed to {}", l.email),
        ),
        Status::Renew(_) => (
            "✦ RENEW PRO",
            "pro-btn renew",
            "Your Pro licence doesn't cover this version".to_owned(),
        ),
        Status::Free => (
            "Get Pro…",
            "pro-btn",
            format!("The free version keeps {FREE_ENGAGEMENTS} engagement; Pro keeps any number"),
        ),
    };
    rsx! {
        button { class, title, onclick: move |_| pro.open.set(true), "{label}" }
    }
}

#[component]
pub fn ProWindow() -> Element {
    let mut pro = use_context::<Pro>();
    let mut key = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let mut close = move || {
        pro.open.set(false);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        match licence::activate(&pasted) {
            Ok(s) => {
                pro.status.set(s);
                key.set(String::new());
                error.set(None);
            }
            Err(e) => error.set(Some(e)),
        }
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close(),
            div {
                class: "modal",
                // Clicks inside the window don't close it.
                onclick: move |e| e.stop_propagation(),
                h2 { "Surface Watch Pro" }
                match &status {
                    Status::Free => rsx! {
                        p {
                            "The free version keeps one engagement, with every tab, the CLI and scheduled watches. "
                            "Pro keeps any number of engagements, and is the licence for commercial use. "
                            "Engagements already on this computer stay usable either way."
                        }
                    },
                    Status::Pro(l) => rsx! {
                        p { class: "pro-ok",
                            "Pro is active on this computer, licensed to "
                            strong { "{l.email}" }
                            ". Updates included until {l.updates_until}."
                        }
                    },
                    Status::Renew(l) => rsx! {
                        p {
                            "Your licence ("
                            strong { "{l.email}" }
                            ") covers versions released until {l.updates_until}. This version is newer, so it runs as the free version: renew Pro to use it here, or keep using a version from before that date."
                        }
                    },
                    Status::Unavailable => rsx! {
                        p { "This build of Surface Watch can't check licences, so nothing is limited. Builds from mayorana.ch can." }
                    },
                }
                if matches!(status, Status::Free | Status::Renew(_)) {
                    button {
                        class: "btn pri",
                        onclick: move |_| {
                            os::open_in_browser(licence::BUY_URL);
                        },
                        if matches!(status, Status::Renew(_)) { "Renew Surface Watch Pro" } else { "Buy Surface Watch Pro" }
                    }
                    p { class: "muted small", "Already bought it? Paste the licence key from your email:" }
                    textarea {
                        class: "licence-key",
                        rows: 4,
                        placeholder: "Licence key",
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                    }
                    if let Some(e) = error.read().as_ref() {
                        p { class: "bad small", "{e}" }
                    }
                    p { class: "muted small",
                        "The key is checked on this computer: no account, nothing sent anywhere."
                    }
                }
                div { class: "modal-actions",
                    if matches!(status, Status::Free | Status::Renew(_)) {
                        button {
                            class: "btn",
                            disabled: key.read().trim().is_empty(),
                            onclick: move |_| activate(),
                            "Activate"
                        }
                    }
                    if matches!(status, Status::Pro(_) | Status::Renew(_)) {
                        button {
                            class: "btn ghost",
                            title: "To move the licence to another computer",
                            onclick: move |_| {
                                licence::deactivate();
                                pro.status.set(licence::current());
                            },
                            "Remove the licence from this computer"
                        }
                    }
                    button { class: "btn ghost", onclick: move |_| close(), "Close" }
                }
            }
        }
    }
}

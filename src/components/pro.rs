//! The Surface Watch Pro window: what the free version does, the plan on
//! this computer, and where to paste a licence number. The limit itself lives in the
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

/// `f` off the UI thread: the licence calls block.
async fn off_ui<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

/// Checks the licence with api0 when `force`d or a check is due.
pub async fn refresh(force: bool) -> Status {
    off_ui(move || licence::refresh(force))
        .await
        .unwrap_or_else(licence::current)
}

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let (label, class, title) = match &status {
        Status::Pro(a) => (
            "✦ PRO",
            "pro-btn active",
            format!("Surface Watch Pro, {} plan for {}", a.plan.name, a.email),
        ),
        Status::NotPro(_) | Status::Offline(_) | Status::Unknown(_) => (
            "✦ RENEW PRO",
            "pro-btn renew",
            "Your licence doesn't make this Surface Watch Pro right now".to_owned(),
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
    let mut busy = use_signal(|| false);
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let asks_number = matches!(status, Status::Free | Status::Unknown(_));
    let mut close = move || {
        pro.open.set(false);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        busy.set(true);
        spawn(async move {
            match off_ui(move || licence::activate(&pasted)).await {
                Some(Ok(s)) => {
                    pro.status.set(s);
                    key.set(String::new());
                    error.set(None);
                }
                Some(Err(e)) => error.set(Some(e)),
                None => {}
            }
            busy.set(false);
        });
    };
    let mut check_again = move || {
        busy.set(true);
        spawn(async move {
            pro.status.set(refresh(true).await);
            busy.set(false);
        });
    };
    let mut manage = move |number: String| {
        busy.set(true);
        spawn(async move {
            match off_ui(move || licence::portal_url(&number)).await {
                Some(Ok(url)) => {
                    os::open_in_browser(&url);
                    error.set(None);
                }
                Some(Err(e)) => error.set(Some(e)),
                None => {}
            }
            busy.set(false);
        });
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
                            "Engagements already on this computer stay usable either way. "
                            "Pro comes with a monthly plan on mayorana.ch."
                        }
                    },
                    Status::Pro(a) => rsx! {
                        p { class: "pro-ok",
                            "Pro is active on this computer: "
                            strong { "{a.plan.name}" }
                            " plan for "
                            strong { "{a.email}" }
                            "."
                        }
                        p { class: "small",
                            "{a.credits.left} AI credits left this month. {a.renewal()}."
                        }
                    },
                    Status::NotPro(a) if !a.active => rsx! {
                        p {
                            "The subscription for "
                            strong { "{a.email}" }
                            " isn't active, so Surface Watch runs as the free version. Renew it to have Pro again."
                        }
                    },
                    Status::NotPro(a) => rsx! {
                        p {
                            "The "
                            strong { "{a.plan.name}" }
                            " plan doesn't include Surface Watch, so it runs as the free version. A plan with Surface Watch makes it Pro."
                        }
                    },
                    Status::Offline(a) => rsx! {
                        p {
                            "Surface Watch hasn't been able to check the licence for "
                            strong { "{a.email}" }
                            " for a week, so it runs as the free version until it can. Check the connection, then check again."
                        }
                    },
                    Status::Unknown(_) => rsx! {
                        p { "The licence number on this computer isn't known. Check it against the email, or paste it again." }
                    },
                }
                if !matches!(status, Status::Pro(_)) {
                    button {
                        class: "btn pri",
                        onclick: move |_| {
                            os::open_in_browser(licence::BUY_URL);
                        },
                        if asks_number { "Buy Surface Watch Pro" } else { "Renew Surface Watch Pro" }
                    }
                }
                if asks_number {
                    p { class: "muted small", "Already subscribed? Paste the licence number from your email:" }
                    input {
                        class: "licence-key",
                        placeholder: "ABCDE-FGHJK-LMNPQ-RSTUV",
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                    }
                    p { class: "muted small",
                        "The number is checked with api0 now and once a day. Pro keeps working for a week without a connection."
                    }
                }
                if let Some(e) = error.read().as_ref() {
                    p { class: "bad small", "{e}" }
                }
                div { class: "modal-actions",
                    if asks_number {
                        button {
                            class: "btn",
                            disabled: key.read().trim().is_empty() || busy(),
                            onclick: move |_| activate(),
                            "Activate"
                        }
                    }
                    if let (Some(number), false) = (status.licence().map(str::to_owned), asks_number) {
                        button {
                            class: "btn",
                            disabled: busy(),
                            title: "Change or cancel the plan on mayorana.ch",
                            onclick: move |_| manage(number.clone()),
                            "Manage subscription"
                        }
                        button {
                            class: "btn ghost",
                            disabled: busy(),
                            onclick: move |_| check_again(),
                            "Check again"
                        }
                    }
                    if status.licence().is_some() {
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

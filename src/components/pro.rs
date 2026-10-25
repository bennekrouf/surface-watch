//! The SurfaceWatch Pro window: what the free version does, the plan on
//! this computer, and where to paste a licence number. The limit itself lives in the
//! core (`licence::engagement_limit`), so the CLI keeps it too.

use dioxus::prelude::*;
use surfacewatch_core::licence::{self, FreeError, Status, FREE_ENGAGEMENTS};

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

/// Sends `true` for each Escape pressed in the page. The handler is kept on
/// `window` and replaced on each mount, so two never stack.
const ESCAPE_JS: &str =
    "if (window._proEscape) window.removeEventListener('keydown', window._proEscape);\
     window._proEscape = (e) => { if (e.key === 'Escape') dioxus.send(true); };\
     window.addEventListener('keydown', window._proEscape);";

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let (label, class, title) = match &status {
        Status::Pro(a) => (
            "✦ PRO",
            "pro-btn active",
            format!("SurfaceWatch Pro, {} plan for {}", a.plan.name, a.email),
        ),
        Status::NotPro(a) if a.is_free() => (
            "Get Pro…",
            "pro-btn",
            format!(
                "Free: {} AI credits left this month. The free version keeps {FREE_ENGAGEMENTS} engagement; Pro keeps any number",
                a.credits.left
            ),
        ),
        Status::NotPro(_) | Status::Offline(_) | Status::Unknown(_) => (
            "✦ RENEW PRO",
            "pro-btn renew",
            "Your licence doesn't make this SurfaceWatch Pro right now".to_owned(),
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
    let mut free_email = use_signal(String::new);
    let mut free_note = use_signal(|| Option::<String>::None);
    let mut free_hidden = use_signal(|| false);
    // Set by an Activate that made SurfaceWatch Pro, so the window
    // congratulates rather than just reporting it.
    let mut activated = use_signal(|| false);
    // Escape closes the window wherever the focus is: the Pro view has no
    // field to listen on.
    use_future(move || async move {
        let mut open = pro.open;
        let mut keys = document::eval(ESCAPE_JS);
        while keys.recv::<bool>().await.is_ok() {
            if *open.peek() {
                open.set(false);
                activated.set(false);
            }
        }
    });
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let asks_number =
        matches!(status, Status::Free | Status::Unknown(_)) || status.free_credits().is_some();
    let offers_free = matches!(status, Status::Free | Status::Unknown(_)) && !free_hidden();
    let mut close = move || {
        pro.open.set(false);
        activated.set(false);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        busy.set(true);
        spawn(async move {
            match off_ui(move || licence::activate(&pasted)).await {
                Some(Ok(s)) => {
                    activated.set(matches!(s, Status::Pro(_)));
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
    let mut ask_free = move || {
        let email = free_email.read().clone();
        busy.set(true);
        spawn(async move {
            match off_ui(move || licence::request_free_credits(&email)).await {
                Some(Ok(message)) => {
                    free_note.set(Some(format!(
                        "{message} Paste the licence number above when it arrives."
                    )));
                    error.set(None);
                }
                Some(Err(FreeError::Unavailable)) => free_hidden.set(true),
                Some(Err(FreeError::Refused(e))) => error.set(Some(e)),
                None => {}
            }
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

    if let Status::Pro(a) = &status {
        let number = a.licence.clone();
        return rsx! {
            div { class: "modal-backdrop", onclick: move |_| close(),
                div {
                    class: "modal pro-done",
                    onclick: move |e| e.stop_propagation(),
                    button { class: "modal-close", title: "Close", onclick: move |_| close(), "×" }
                    div { class: "pro-done-mark", "✓" }
                    h2 { class: "pro-done-title",
                        if activated() { "Congratulations, you're Pro!" } else { "You're Pro" }
                    }
                    p { class: "pro-done-sub", "Licence validated." }
                    p { class: "pro-done-meta",
                        "{a.credits.left} AI credits left this month"
                        br {}
                        "{a.renewal()}"
                    }
                    button {
                        class: "pro-done-manage",
                        disabled: busy(),
                        title: "Change or cancel the plan on mayorana.ch",
                        onclick: move |_| manage(number.clone()),
                        "Manage subscription"
                    }
                    if let Some(e) = error.read().as_ref() {
                        p { class: "pro-done-error", "{e}" }
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close(),
            div {
                class: "modal",
                // Clicks inside the window don't close it.
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "SurfaceWatch Pro" }
                    button { class: "modal-close", title: "Close", onclick: move |_| close(), "×" }
                }
                div { class: "modal-body",
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
                        Status::NotPro(a) if a.is_free() => rsx! {
                            p {
                                "Free — "
                                strong { "{a.credits.left} AI credits" }
                                " left this month, for "
                                strong { "{a.email}" }
                                ". {a.renewal()}."
                            }
                            p {
                                "The free version keeps one engagement. Pro keeps any number, and is the licence for commercial use. "
                                "It comes with a monthly plan on mayorana.ch."
                            }
                        },
                        Status::NotPro(a) if !a.active => rsx! {
                            p {
                                "The subscription for "
                                strong { "{a.email}" }
                                " isn't active, so SurfaceWatch runs as the free version. Renew it to have Pro again."
                            }
                        },
                        Status::NotPro(a) => rsx! {
                            p {
                                "The "
                                strong { "{a.plan.name}" }
                                " plan doesn't include SurfaceWatch, so it runs as the free version. A plan with SurfaceWatch makes it Pro."
                            }
                        },
                        Status::Offline(a) => rsx! {
                            p {
                                "SurfaceWatch hasn't been able to check the licence for "
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
                            if asks_number { "Buy SurfaceWatch Pro" } else { "Renew SurfaceWatch Pro" }
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
                    if offers_free {
                        div { class: "free-credits",
                            p { class: "muted small", "No plan? Get free AI credits every month, without Pro. The licence number comes by email." }
                            div { class: "free-row",
                                input {
                                    class: "licence-key",
                                    r#type: "email",
                                    placeholder: "you@example.com",
                                    value: "{free_email}",
                                    oninput: move |e| free_email.set(e.value()),
                                }
                                button {
                                    class: "btn",
                                    disabled: free_email.read().trim().is_empty() || busy(),
                                    onclick: move |_| ask_free(),
                                    "Get free AI credits"
                                }
                            }
                            if let Some(n) = free_note.read().as_ref() {
                                p { class: "pro-ok small", "{n}" }
                            }
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
                                if busy() { span { class: "pro-spinner", title: "Checking the licence" } } else { "Activate" }
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
                                title: "Removes it from every Mayorana app on this computer, to move it to another one",
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
}

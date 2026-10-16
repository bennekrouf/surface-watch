mod components;
mod screens;
mod theme;
mod update_check;

use dioxus::desktop::LogicalSize;
use dioxus::prelude::*;
use screens::main_screen::MainScreen;

const MAIN_CSS: &str = include_str!("../assets/main.css");

fn webview_data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Surface Watch")
        .join("webview")
}

fn main() {
    // Before anything can shell out: an app launched from Finder or a .dmg
    // does not inherit the terminal's PATH, so `node` and `rea` read as
    // "not found".
    surface_watch_core::env::adopt_login_path();

    let cfg = dioxus::desktop::Config::new()
        .with_data_directory(webview_data_dir())
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title(concat!("Surface Watch ", env!("CARGO_PKG_VERSION")))
                .with_inner_size(LogicalSize::new(1280.0, 820.0)),
        );
    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    let mut update = use_signal(|| Option::<update_check::UpdateInfo>::None);
    let mut dismissed = use_signal(|| false);
    use_future(move || async move {
        // After the window is up, never in its way; skipped when the user
        // has opted out of update checks.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        if std::env::var_os("DISABLE_UPDATE_CHECK").is_some() {
            return;
        }
        if let Some(info) = update_check::check().await {
            update.set(Some(info));
        }
    });

    rsx! {
        style { {MAIN_CSS} }
        theme::ThemeToggle {}
        div { class: "shell",
            if let (Some(info), false) = (update(), dismissed()) {
                div { class: "update-banner", role: "status",
                    span { class: "update-text",
                        "Surface Watch "
                        strong { "{info.latest_version}" }
                        " is available (you have {env!(\"CARGO_PKG_VERSION\")})."
                    }
                    {
                        // A button, not a link: the URL comes from a remote
                        // file and must open in the browser, never in the
                        // app's own webview.
                        let url = info.release_url.clone();
                        rsx! {
                            button {
                                class: "btn pri",
                                onclick: move |_| {
                                    components::os::open_in_browser(&url);
                                },
                                "Download"
                            }
                        }
                    }
                    if !info.sha256.is_empty() {
                        span {
                            class: "mono small update-sha",
                            title: "SHA-256 of the published build: compare it with the file you downloaded. Full value: {info.sha256}",
                            {format!("sha256 {}…", &info.sha256[..info.sha256.len().min(12)])}
                        }
                    }
                    button {
                        class: "btn ghost",
                        title: "Hide until the next launch",
                        onclick: move |_| dismissed.set(true),
                        "×"
                    }
                }
            }
            MainScreen {}
        }
    }
}

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
                .with_inner_size(LogicalSize::new(1280.0, 820.0))
                .with_window_icon(window_icon()),
        );
    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

/// The window icon, decoded from the embedded logo — same as GitAgent and
/// TrackCut.
///
/// build.rs embeds `assets/icon.ico` into the .exe resource, which covers the
/// Start menu and shortcuts — but the *window* (title bar, alt-tab, taskbar
/// button) shows only what the app sets at runtime: Windows falls back to a
/// blank default and Linux to a generic glyph when it sets nothing.
///
/// Downscaled to 64px on the way in: tao hands the OS this single bitmap for
/// every size it needs, and stretching a 1024px source down to a 16px title
/// bar is what makes the icon look muddy.
fn window_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
    const SIZE: u32 = 64;

    let img = image::load_from_memory(ICON_PNG).ok()?.resize_exact(
        SIZE,
        SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    dioxus::desktop::tao::window::Icon::from_rgba(img.into_rgba8().into_raw(), SIZE, SIZE).ok()
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

    // Surface Watch Pro: the free version keeps one engagement.
    use_context_provider(|| components::pro::Pro {
        open: Signal::new(false),
        status: Signal::new(surface_watch_core::licence::current()),
    });

    rsx! {
        style { {MAIN_CSS} }
        theme::ThemeToggle {}
        components::pro::ProWindow {}
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

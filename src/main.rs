mod components;
mod screens;

use desktop_kit::ui::{use_pro, use_theme, Banners, Header, ProButton, ProWindow};
use dioxus::desktop::LogicalSize;
use dioxus::prelude::*;
use screens::main_screen::MainScreen;

const MAIN_CSS: &str = include_str!("../assets/main.css");

fn webview_data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        // The data folder keeps the name from before SurfaceWatch; see store::data_dir.
        .join("Surface Watch")
        .join("webview")
}

/// The native title bar, as in every mayorana app: name, version and licence.
fn window_title(status: &surfacewatch_core::licence::Status) -> String {
    format!(
        "SurfaceWatch {}{}",
        env!("CARGO_PKG_VERSION"),
        surfacewatch_core::licence::title_badge(status)
    )
}

fn main() {
    surfacewatch_core::kit::init();
    // Before anything can shell out: an app launched from Finder or a .dmg
    // does not inherit the terminal's PATH, so `node` and `rea` read as
    // "not found".
    surfacewatch_core::env::adopt_login_path();

    let cfg = dioxus::desktop::Config::new()
        .with_data_directory(webview_data_dir())
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title(window_title(&surfacewatch_core::licence::current()))
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
    // SurfaceWatch Pro: the free version keeps one engagement.
    // Checked with api0 at start, then once a day.
    let pro = use_pro();
    // The title bar follows the licence, however it changes.
    use_effect(move || {
        dioxus::desktop::window().set_title(&window_title(&pro.status.read()));
    });

    let theme = use_theme();
    rsx! {
        style { {MAIN_CSS} }
        ProWindow { pro, pitch: components::pro::pitch() }
        Banners {}
        div { class: "shell",
            Header { theme, pro: rsx! { ProButton { pro, hint: components::pro::hint() } } }
            MainScreen {}
        }
    }
}

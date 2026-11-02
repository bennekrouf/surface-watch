//! How to use SurfaceWatch from Claude: what to paste, and where.
//!
//! The command is this app's own executable, read at runtime rather than
//! written down, so the snippet is right wherever it was installed — and in a
//! development build, which is what someone trying it from a checkout runs.

use dioxus::prelude::*;

/// Whether the window is open. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct McpOpen(pub Signal<bool>);

#[component]
pub fn McpWindow() -> Element {
    let mut open = use_context::<McpOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        desktop_kit::ui::ConnectWindow {
            lead: "Ask Claude about your engagements: what changed between two builds, which endpoints are flagged, \
                   which findings are still open after the retest. It reads what SurfaceWatch has stored and \
                   changes nothing: captures, retests and findings stay here, in the app.",
            try_it: "\u{201c}What changed in the shop\u{2019}s attack surface since the baseline?\u{201d}, or \u{201c}Which \
                     findings are still present, and what evidence do they rest on?\u{201d}",
            caution: "What Claude reads (paths, parameter names, findings) goes to the model it runs on. Check that the \
                      engagement allows that before connecting.",
            on_close: move |_| open.set(false),
        }
    }
}

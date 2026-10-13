use dioxus::prelude::*;

/// A tab whose screen is not built yet: says what will be there.
#[component]
pub fn PlaceholderPanel(title: &'static str, body: &'static str) -> Element {
    rsx! {
        section { class: "placeholder",
            h2 { "{title}" }
            p { class: "muted", "{body}" }
            span { class: "pill info", "Not built yet" }
        }
    }
}

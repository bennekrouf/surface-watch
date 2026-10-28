//! What SurfaceWatch Pro adds. The window, the badge and the licence checks
//! are desktop-kit's, the same in every mayorana app. The limit itself lives
//! in the core (`licence::engagement_limit`), so the CLI keeps it too.

use dioxus::prelude::*;
use surfacewatch_core::licence::FREE_ENGAGEMENTS;

/// The badge's tooltip while SurfaceWatch isn't Pro.
pub fn hint() -> String {
    format!("The free version keeps {FREE_ENGAGEMENTS} engagement; Pro keeps any number")
}

/// What the free version does and what Pro adds, in the Pro window.
pub fn pitch() -> Element {
    rsx! {
        p {
            "The free version keeps one engagement, with every tab, the CLI and scheduled watches. "
            "Pro keeps any number of engagements, and is the licence for commercial use. "
            "Engagements already on this computer stay usable either way. "
            "Pro comes with a monthly plan on mayorana.ch."
        }
    }
}

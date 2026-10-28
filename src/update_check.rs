//! Whether a newer SurfaceWatch is published: the same check in every mayorana
//! desktop app, so it lives in desktop-kit (`desktop_kit::update_check`).
//! `MAYORANA_UPDATE_URL` points it at a staging `latest.json`.

pub use desktop_kit::update_check::*;

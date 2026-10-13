//! Shared, UI-agnostic services for Surface Watch.
//!
//! Both frontends — the Dioxus desktop app at the workspace root and the
//! headless CLI in `crates/cli` — depend on this crate, so there is exactly
//! one definition of the surface model and one diff engine.
//!
//! The pipeline, in module order:
//!
//! * [`rea`] runs the REA CLI as a subprocess and returns its Evidence.
//! * [`capture`] reads REA browser-scenario captures (raw Evidence or its
//!   `normalized_result`) into typed network events.
//! * [`har`] converts HAR recordings (browser DevTools, Caido, ZAP,
//!   mitmproxy) into the same events, redacted.
//! * [`surface`] normalises those events into an attack surface.
//! * [`flags`] points out surface entries worth a tester's attention.
//! * [`diff`] compares two surfaces into security-relevant changes.
//! * [`model`] holds the engagement records the app persists via [`store`];
//!   [`setup`] validates and creates them.
//! * [`watch`] stores captures per target and compares any two of them.
//! * [`findings`] promotes diff items to findings, retests and exports them.

pub mod capture;
pub mod diff;
pub mod env;
pub mod findings;
pub mod flags;
pub mod har;
pub mod model;
pub mod rea;
pub mod readiness;
pub mod setup;
pub mod store;
pub mod surface;
pub mod watch;

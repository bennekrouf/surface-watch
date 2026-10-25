//! Regression test on real captures.
//!
//! The fixtures are REA 6.4.0 browser-scenario captures of OWASP Juice Shop
//! (a deliberately vulnerable app, run locally), crawling the same 21 pages
//! in 4 batches: once on v19.0.0 and twice on v20.3.0. They are trimmed to
//! the fields the surface reads. REA's own comparison reports these
//! captures as "changed" even between the two v20.3.0 runs; the
//! expectations below are the changes that are actually in the code.

use std::path::{Path, PathBuf};
use surfacewatch_core::{
    capture,
    diff::{diff, ChangeKind},
    surface::Surface,
};

const ORIGIN: &str = "http://127.0.0.1:3012";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/juice-shop")
        .join(name)
}

fn surface(name: &str) -> Surface {
    Surface::build(ORIGIN, &capture::read_dir(&fixture(name)).unwrap())
}

#[test]
fn two_captures_of_one_build_differ_only_by_known_unstable_items() {
    let d = diff(&surface("v20.3.0"), &surface("v20.3.0-repeat"));
    let significant: Vec<_> = d.significant().map(|c| c.subject.as_str()).collect();
    // Timer-driven progress save: fires in some runs and not others.
    assert_eq!(significant, vec!["GET /rest/continue-code"]);
}

#[test]
fn the_v19_to_v20_upgrade_reports_exactly_the_real_changes() {
    let before = surface("v19.0.0");
    let mut after = surface("v20.3.0");
    after.mark_unstable(&surface("v20.3.0-repeat"));
    let d = diff(&before, &after);

    let got: Vec<_> = d
        .significant()
        .map(|c| (c.kind, c.subject.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (ChangeKind::EndpointRemoved, "GET /rest/basket/NaN"),
            // The chatbot API was rewritten; its replacement is only called
            // when a message is sent, which this crawl does not do.
            (ChangeKind::EndpointRemoved, "GET /rest/chatbot/status"),
            (ChangeKind::ParamsChanged, "GET /rest/user/whoami"),
            (ChangeKind::ThirdPartyRemoved, "http://cdnjs.cloudflare.com"),
            (
                ChangeKind::ThirdPartyRemoved,
                "https://cdnjs.cloudflare.com"
            ),
        ]
    );
    assert_eq!(d.suppressed.len(), 1);
    assert_eq!((d.endpoints_before, d.endpoints_after), (23, 21));
}

#[test]
fn hashed_chunk_names_do_not_count_as_new_scripts() {
    let a = surface("v20.3.0");
    assert!(a.scripts.contains("/chunk-*.js"));
    assert!(!a
        .scripts
        .iter()
        .any(|s| s.starts_with("/chunk-") && s != "/chunk-*.js"));
}

#[test]
fn the_v20_surface_flags_what_a_tester_would_look_at_first() {
    use surfacewatch_core::flags::{endpoint_flags, page_flags, Flag};
    let s = surface("v20.3.0");
    let leads: Vec<(String, Flag)> = s
        .endpoints
        .iter()
        .flat_map(|(k, ep)| {
            endpoint_flags(k, ep, s.unstable.contains(k))
                .into_iter()
                .filter(Flag::is_lead)
                .map(move |f| (k.to_string(), f))
        })
        .collect();
    assert!(leads.contains(&("GET /rest/order-history".into(), Flag::ServerError(500))));
    assert!(leads.contains(&(
        "GET /rest/admin/application-configuration".into(),
        Flag::SensitivePath(200)
    )));
    assert!(leads.contains(&("GET /rest/track-order/:id".into(), Flag::ObjectId)));
    // Juice Shop serves its pages without a Content-Security-Policy.
    assert_eq!(
        page_flags(&s),
        vec![Flag::PageHeaderMissing("content-security-policy".into())]
    );
}

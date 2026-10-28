use super::*;
use crate::model::{CaptureSource, Toolchain};
use crate::surface::Endpoint;

fn cap(id: &str, ev: &str) -> Capture {
    Capture {
        id: id.into(),
        target_id: "t".into(),
        source: CaptureSource::ReaScenario,
        taken_at: Utc::now(),
        evidence_ids: vec![ev.into()],
        files: vec![],
        toolchain: Toolchain::default(),
        repeat_of: None,
        label: None,
    }
}

fn change(kind: ChangeKind, subject: &str, added: &[&str], removed: &[&str]) -> Change {
    Change {
        kind,
        subject: subject.into(),
        added: added.iter().map(|s| s.to_string()).collect(),
        removed: removed.iter().map(|s| s.to_string()).collect(),
    }
}

fn surface_with(method: &str, path: &str, ep: Endpoint) -> Surface {
    let mut s = Surface::default();
    s.endpoints.insert(
        EndpointKey {
            method: method.into(),
            path: path.into(),
        },
        ep,
    );
    s
}

#[test]
fn a_server_error_on_the_surface_becomes_a_retestable_finding() {
    let key = EndpointKey {
        method: "GET".into(),
        path: "/rest/order-history".into(),
    };
    let ep = Endpoint {
        statuses: [500].into(),
        ..Default::default()
    };
    let f = promote_surface(
        "t",
        &cap("c", "ev_c"),
        Some((&key, &ep)),
        Some(&Flag::ServerError(500)),
        &[],
    );
    assert_eq!(
        f.title,
        "GET /rest/order-history answers 500 to an ordinary request"
    );
    assert_eq!(f.severity, Severity::Low);
    assert_eq!(f.evidence_ids, vec!["ev_c"]);
    let s = surface_with("GET", "/rest/order-history", ep);
    assert_eq!(
        evaluate(f.check.as_ref().unwrap(), &s).0,
        RetestStatus::StillPresent
    );
}

#[test]
fn a_missing_page_header_retests_from_the_pages_seen() {
    let check = Check::PageHeaderMissing {
        header: "content-security-policy".into(),
    };
    let mut s = Surface::default();
    assert_eq!(evaluate(&check, &s).0, RetestStatus::Unknown);
    s.documents_seen = 2;
    assert_eq!(evaluate(&check, &s).0, RetestStatus::StillPresent);
    s.page_security_headers
        .insert("content-security-policy".into());
    assert_eq!(evaluate(&check, &s).0, RetestStatus::Fixed);
}

#[test]
fn ids_continue_past_the_highest_even_after_a_delete() {
    let (a, b) = (cap("a", "ev_a"), cap("b", "ev_b"));
    let c = change(ChangeKind::EndpointAdded, "GET /x", &["200"], &[]);
    let f1 = promote(&c, "t", &a, &b, &[]);
    let f2 = promote(&c, "t", &a, &b, std::slice::from_ref(&f1));
    assert_eq!((f1.id.as_str(), f2.id.as_str()), ("F-001", "F-002"));
    // F-001 deleted: the next one is still F-003, never a reused F-002.
    assert_eq!(next_id(&[f2]), "F-003");
}

#[test]
fn a_promoted_finding_cites_both_captures_and_gets_a_check() {
    let c = change(
        ChangeKind::ParamsChanged,
        "GET /rest/user/whoami",
        &["fields"],
        &[],
    );
    let f = promote(&c, "t", &cap("a", "ev_a"), &cap("b", "ev_b"), &[]);
    assert_eq!(f.evidence_ids, vec!["ev_a", "ev_b"]);
    assert_eq!(f.diff_refs, vec!["b/GET /rest/user/whoami"]);
    assert_eq!(
        f.check,
        Some(Check::Param {
            method: "GET".into(),
            path: "/rest/user/whoami".into(),
            param: "fields".into()
        })
    );
}

#[test]
fn a_removed_endpoint_has_no_check_because_no_capture_can_confirm_it() {
    let c = change(
        ChangeKind::EndpointRemoved,
        "GET /rest/chatbot/status",
        &[],
        &[],
    );
    assert!(promote(&c, "t", &cap("a", "x"), &cap("b", "y"), &[])
        .check
        .is_none());
}

#[test]
fn a_status_check_is_fixed_still_present_or_unknown() {
    let check = Check::Endpoint {
        method: "GET".into(),
        path: "/rest/order-history".into(),
        status: Some(500),
    };
    let ep = |s: u16| Endpoint {
        statuses: [s].into(),
        ..Default::default()
    };
    let still = surface_with("GET", "/rest/order-history", ep(500));
    let fixed = surface_with("GET", "/rest/order-history", ep(401));
    assert_eq!(evaluate(&check, &still).0, RetestStatus::StillPresent);
    assert_eq!(evaluate(&check, &fixed).0, RetestStatus::Fixed);
    // Not requested at all: not proof of a fix.
    assert_eq!(
        evaluate(&check, &Surface::default()).0,
        RetestStatus::Unknown
    );
}

#[test]
fn an_unstable_endpoint_retests_as_unknown() {
    let check = Check::Endpoint {
        method: "GET".into(),
        path: "/rest/continue-code".into(),
        status: None,
    };
    let mut s = surface_with("GET", "/rest/continue-code", Endpoint::default());
    s.unstable.insert(EndpointKey {
        method: "GET".into(),
        path: "/rest/continue-code".into(),
    });
    let (status, note) = evaluate(&check, &s);
    assert_eq!(status, RetestStatus::Unknown);
    assert!(note.contains("unstable"), "got {note}");
}

#[test]
fn a_newer_har_that_misses_the_endpoint_does_not_hide_the_crawls_answer() {
    let check = Check::Endpoint {
        method: "GET".into(),
        path: "/rest/order-history".into(),
        status: Some(500),
    };
    let c = change(
        ChangeKind::EndpointAdded,
        "GET /rest/order-history",
        &["500"],
        &[],
    );
    let mut f = promote(&c, "t", &cap("a", "x"), &cap("b", "y"), &[]);
    f.check = Some(check);
    let crawl = surface_with(
        "GET",
        "/rest/order-history",
        Endpoint {
            statuses: [500].into(),
            ..Default::default()
        },
    );
    let har = surface_with("POST", "/rest/chat", Endpoint::default());
    // Newest first: the HAR, then the crawl.
    let r = retest_best(&f, &[(cap("har", "h"), har), (cap("crawl", "c"), crawl)]).unwrap();
    assert_eq!(r.status, RetestStatus::StillPresent);
    assert_eq!(r.capture_id, "crawl");
}

#[test]
fn a_finding_cites_a_capture_through_evidence_promotion_or_retest() {
    let (a, b, c, d) = (
        cap("a", "ev_a"),
        cap("b", "ev_b"),
        cap("c", "ev_c"),
        cap("d", "ev_d"),
    );
    let mut f = promote(
        &change(ChangeKind::EndpointAdded, "GET /x", &[], &[]),
        "t",
        &a,
        &b,
        &[],
    );
    f.retests.push(Retest {
        capture_id: "c".into(),
        at: Utc::now(),
        status: RetestStatus::Fixed,
        note: String::new(),
        manual: false,
    });
    let all = [f];
    assert_eq!(citing(&all, &a).len(), 1); // evidence
    assert_eq!(citing(&all, &b).len(), 1); // evidence and diff ref
    assert_eq!(citing(&all, &c).len(), 1); // retest
    assert!(citing(&all, &d).is_empty());
}

#[test]
fn retesting_the_same_capture_twice_keeps_one_entry() {
    let c = change(ChangeKind::EndpointAdded, "GET /x", &["200"], &[]);
    let mut f = promote(&c, "t", &cap("a", "x"), &cap("b", "y"), &[]);
    let s = surface_with("GET", "/x", Endpoint::default());
    let later = cap("c", "z");
    for _ in 0..2 {
        let r = retest(&f, &later, &s).unwrap();
        record(&mut f, r);
    }
    assert_eq!(f.retests.len(), 1);
    assert_eq!(f.status(), RetestStatus::StillPresent);
}

#[test]
fn the_markdown_report_lists_findings_by_severity_with_evidence() {
    let e = Engagement {
        id: "e".into(),
        client: "OWASP".into(),
        name: "Juice Shop lab".into(),
        window_start: None,
        window_end: None,
        authorisation: String::new(),
        targets: vec![],
    };
    let (a, b) = (cap("a", "ev_a"), cap("b", "ev_b"));
    let low = promote(
        &change(ChangeKind::StatusChanged, "GET /o", &["500"], &["200"]),
        "t",
        &a,
        &b,
        &[],
    );
    let info = promote(
        &change(ChangeKind::EndpointAdded, "GET /n", &["200"], &[]),
        "t",
        &a,
        &b,
        std::slice::from_ref(&low),
    );
    let md = to_markdown(&e, &[info, low], &[a, b]);
    let (low_at, info_at) = (md.find("## F-001").unwrap(), md.find("## F-002").unwrap());
    assert!(low_at < info_at, "Low severity must come before Info");
    assert!(md.contains("`ev_a`") && md.contains("`ev_b`"));
}

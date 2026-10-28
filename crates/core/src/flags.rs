//! Pointing out what in a surface deserves a tester's attention.
//!
//! These are heuristics, not verdicts: a flag says "look here", and the
//! tester decides whether it becomes a finding. Each flag is explained in
//! plain words so a reader can judge it without knowing the rule.

use crate::surface::{Endpoint, EndpointKey, Surface};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Flag {
    /// Answered with a 5xx status during the crawl.
    ServerError(u16),
    /// Path names an admin, debug or configuration area and answered 2xx.
    SensitivePath(u16),
    /// A path segment is an object ID: a candidate for access-control tests.
    ObjectId,
    /// Answered 401 or 403: protected, which is worth knowing, not a problem.
    AuthRequired,
    /// Seen in only one of two captures of the same build.
    Unstable,
    /// HTML pages were served without this security header.
    PageHeaderMissing(String),
}

impl Flag {
    pub fn label(&self) -> String {
        match self {
            Flag::ServerError(s) => format!("{s} server error"),
            Flag::SensitivePath(_) => "sensitive path, reachable".into(),
            Flag::ObjectId => "object ID in path".into(),
            Flag::AuthRequired => "auth required".into(),
            Flag::Unstable => "unstable".into(),
            Flag::PageHeaderMissing(h) => format!("no {h}"),
        }
    }

    pub fn explanation(&self) -> String {
        match self {
            Flag::ServerError(s) => format!(
                "Answered {s} to a request from the crawl. Server errors on ordinary requests often leak stack traces or point at unhandled input."
            ),
            Flag::SensitivePath(s) => format!(
                "The path names an admin, debug or configuration area and answered {s} without any login in the crawl."
            ),
            Flag::ObjectId => "Takes an object ID in the path: test whether another user's ID is accepted.".into(),
            Flag::AuthRequired => "Refused the crawl's anonymous request, as a protected endpoint should.".into(),
            Flag::Unstable => "Appears in only one of two captures of the same build, usually a timer-driven call.".into(),
            Flag::PageHeaderMissing(h) => format!("No HTML page in the capture was served with {h}."),
        }
    }

    /// Semantic tone: `bad` deserves attention first, `info` is context.
    pub fn tone(&self) -> &'static str {
        match self {
            Flag::ServerError(_) | Flag::SensitivePath(_) => "bad",
            Flag::PageHeaderMissing(_) | Flag::ObjectId => "warn",
            Flag::AuthRequired | Flag::Unstable => "info",
        }
    }

    /// Worth offering as a finding. Protected and unstable endpoints are not.
    pub fn is_lead(&self) -> bool {
        !matches!(self, Flag::AuthRequired | Flag::Unstable)
    }
}

/// Path words that name an area testers check first.
// A constant pattern: a mistake in it fails the tests, not a user's run.
#[allow(clippy::unwrap_used)]
fn sensitive_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)/(admin|administration|internal|debug|config|configuration|private|actuator|swagger|api-docs|metrics|backup|\.git|\.env|console)(/|-|$)",
        )
        .unwrap()
    })
}

/// Flags for one endpoint, most important first.
pub fn endpoint_flags(key: &EndpointKey, ep: &Endpoint, unstable: bool) -> Vec<Flag> {
    let mut flags = vec![];
    if let Some(s) = ep.statuses.iter().copied().find(|s| *s >= 500) {
        flags.push(Flag::ServerError(s));
    }
    if sensitive_path().is_match(&key.path) {
        if let Some(s) = ep.statuses.iter().copied().find(|s| (200..300).contains(s)) {
            flags.push(Flag::SensitivePath(s));
        }
    }
    if key.path.contains("/:id") || key.path.contains("/:hash") {
        flags.push(Flag::ObjectId);
    }
    if ep.statuses.iter().any(|s| *s == 401 || *s == 403) {
        flags.push(Flag::AuthRequired);
    }
    if unstable {
        flags.push(Flag::Unstable);
    }
    flags
}

/// Page-level flags. Only raised when the capture saw at least one page.
pub fn page_flags(surface: &Surface) -> Vec<Flag> {
    if surface.documents_seen == 0 {
        return vec![];
    }
    let mut wanted = vec!["content-security-policy"];
    // HSTS only means something over HTTPS.
    if surface.origin.starts_with("https://") {
        wanted.push("strict-transport-security");
    }
    wanted
        .into_iter()
        .filter(|h| !surface.page_security_headers.contains(*h))
        .map(|h| Flag::PageHeaderMissing(h.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(p: &str) -> EndpointKey {
        EndpointKey {
            method: "GET".into(),
            path: p.into(),
        }
    }

    fn ep(statuses: &[u16]) -> Endpoint {
        Endpoint {
            statuses: statuses.iter().copied().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn juice_shop_endpoints_get_the_expected_flags() {
        assert_eq!(
            endpoint_flags(&key("/rest/order-history"), &ep(&[500]), false),
            vec![Flag::ServerError(500)]
        );
        assert_eq!(
            endpoint_flags(
                &key("/rest/admin/application-configuration"),
                &ep(&[200]),
                false
            ),
            vec![Flag::SensitivePath(200)]
        );
        assert_eq!(
            endpoint_flags(&key("/rest/track-order/:id"), &ep(&[200]), false),
            vec![Flag::ObjectId]
        );
        assert_eq!(
            endpoint_flags(&key("/api/Addresss"), &ep(&[401]), false),
            vec![Flag::AuthRequired]
        );
    }

    #[test]
    fn a_protected_admin_path_is_not_flagged_as_reachable() {
        assert_eq!(
            endpoint_flags(&key("/rest/admin/users"), &ep(&[403]), false),
            vec![Flag::AuthRequired]
        );
    }

    #[test]
    fn words_that_merely_contain_admin_are_not_sensitive() {
        assert!(endpoint_flags(&key("/rest/badminton"), &ep(&[200]), false).is_empty());
    }

    #[test]
    fn page_flags_need_a_page_and_hsts_needs_https() {
        let mut s = Surface {
            origin: "http://h".into(),
            ..Default::default()
        };
        assert!(page_flags(&s).is_empty());
        s.documents_seen = 1;
        assert_eq!(
            page_flags(&s),
            vec![Flag::PageHeaderMissing("content-security-policy".into())]
        );
        s.origin = "https://h".into();
        assert_eq!(page_flags(&s).len(), 2);
        s.page_security_headers
            .insert("content-security-policy".into());
        assert_eq!(
            page_flags(&s),
            vec![Flag::PageHeaderMissing("strict-transport-security".into())]
        );
    }
}

//! Comparing two surfaces.
//!
//! The output is what a pentester acts on: endpoints that appeared or went
//! away, and endpoints whose parameters, statuses or security headers
//! changed. Unstable endpoints (see [`Surface::mark_unstable`]) are reported
//! as suppressed rather than silently dropped, so the reader can see what
//! was left out and why.

use crate::surface::{EndpointKey, Surface};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChangeKind {
    EndpointAdded,
    EndpointRemoved,
    StatusChanged,
    ParamsChanged,
    SecurityHeadersChanged,
    ThirdPartyAdded,
    ThirdPartyRemoved,
    ScriptsChanged,
}

impl ChangeKind {
    /// Group heading, in the order groups are shown.
    pub fn label(self) -> &'static str {
        match self {
            ChangeKind::EndpointAdded => "New API endpoint",
            ChangeKind::EndpointRemoved => "Removed API endpoint",
            ChangeKind::StatusChanged => "Status changed",
            ChangeKind::ParamsChanged => "Query parameters changed",
            ChangeKind::SecurityHeadersChanged => "Security headers changed",
            ChangeKind::ThirdPartyAdded => "New third-party origin",
            ChangeKind::ThirdPartyRemoved => "Removed third-party origin",
            ChangeKind::ScriptsChanged => "Scripts changed",
        }
    }

    /// Script churn is context, not attack surface.
    pub fn is_informational(self) -> bool {
        matches!(self, ChangeKind::ScriptsChanged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    /// The endpoint, origin or script the change is about.
    pub subject: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceDiff {
    pub changes: Vec<Change>,
    pub suppressed: Vec<EndpointKey>,
    pub endpoints_before: usize,
    pub endpoints_after: usize,
}

impl SurfaceDiff {
    /// Changes that alter the attack surface, excluding informational ones.
    pub fn significant(&self) -> impl Iterator<Item = &Change> {
        self.changes.iter().filter(|c| !c.kind.is_informational())
    }
}

pub fn diff(before: &Surface, after: &Surface) -> SurfaceDiff {
    let unstable: BTreeSet<_> = before.unstable.union(&after.unstable).cloned().collect();
    let keys = |s: &Surface| -> BTreeSet<EndpointKey> {
        s.endpoints
            .keys()
            .filter(|k| !unstable.contains(*k))
            .cloned()
            .collect()
    };
    let (b, a) = (keys(before), keys(after));
    let mut changes = vec![];

    for k in a.difference(&b) {
        let statuses = after.endpoints[k]
            .statuses
            .iter()
            .map(u16::to_string)
            .collect();
        changes.push(change(
            ChangeKind::EndpointAdded,
            k.to_string(),
            statuses,
            vec![],
        ));
    }
    for k in b.difference(&a) {
        changes.push(change(
            ChangeKind::EndpointRemoved,
            k.to_string(),
            vec![],
            vec![],
        ));
    }
    for k in b.intersection(&a) {
        let (x, y) = (&before.endpoints[k], &after.endpoints[k]);
        let statuses = |s: &BTreeSet<u16>| s.iter().map(u16::to_string).collect::<BTreeSet<_>>();
        push_set_change(
            &mut changes,
            ChangeKind::StatusChanged,
            k,
            &statuses(&x.statuses),
            &statuses(&y.statuses),
        );
        push_set_change(
            &mut changes,
            ChangeKind::ParamsChanged,
            k,
            &x.params,
            &y.params,
        );
        push_set_change(
            &mut changes,
            ChangeKind::SecurityHeadersChanged,
            k,
            &x.security_headers,
            &y.security_headers,
        );
    }
    for o in after
        .third_party_origins
        .difference(&before.third_party_origins)
    {
        changes.push(change(
            ChangeKind::ThirdPartyAdded,
            o.clone(),
            vec![],
            vec![],
        ));
    }
    for o in before
        .third_party_origins
        .difference(&after.third_party_origins)
    {
        changes.push(change(
            ChangeKind::ThirdPartyRemoved,
            o.clone(),
            vec![],
            vec![],
        ));
    }
    if before.scripts != after.scripts {
        changes.push(change(
            ChangeKind::ScriptsChanged,
            "scripts".into(),
            after.scripts.difference(&before.scripts).cloned().collect(),
            before.scripts.difference(&after.scripts).cloned().collect(),
        ));
    }
    changes.sort_by(|x, y| (x.kind, &x.subject).cmp(&(y.kind, &y.subject)));

    SurfaceDiff {
        changes,
        suppressed: unstable.into_iter().collect(),
        endpoints_before: b.len(),
        endpoints_after: a.len(),
    }
}

fn change(kind: ChangeKind, subject: String, added: Vec<String>, removed: Vec<String>) -> Change {
    Change {
        kind,
        subject,
        added,
        removed,
    }
}

fn push_set_change(
    out: &mut Vec<Change>,
    kind: ChangeKind,
    key: &EndpointKey,
    before: &BTreeSet<String>,
    after: &BTreeSet<String>,
) {
    if before != after {
        out.push(change(
            kind,
            key.to_string(),
            after.difference(before).cloned().collect(),
            before.difference(after).cloned().collect(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::Endpoint;

    fn key(m: &str, p: &str) -> EndpointKey {
        EndpointKey {
            method: m.into(),
            path: p.into(),
        }
    }

    fn surface(endpoints: &[(&str, &str, &[&str])]) -> Surface {
        let mut s = Surface::default();
        for (m, p, params) in endpoints {
            s.endpoints.insert(
                key(m, p),
                Endpoint {
                    params: params.iter().map(|x| x.to_string()).collect(),
                    statuses: [200].into(),
                    ..Default::default()
                },
            );
        }
        s
    }

    #[test]
    fn identical_surfaces_have_no_changes() {
        let s = surface(&[("GET", "/rest/languages", &[])]);
        assert!(diff(&s, &s.clone()).changes.is_empty());
    }

    #[test]
    fn the_juice_shop_v19_to_v20_changes_are_reported() {
        let before = surface(&[
            ("GET", "/rest/chatbot/status", &[]),
            ("GET", "/rest/user/whoami", &[]),
        ]);
        let after = surface(&[("GET", "/rest/user/whoami", &["fields"])]);
        let d = diff(&before, &after);
        let kinds: Vec<_> = d
            .changes
            .iter()
            .map(|c| (c.kind, c.subject.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (ChangeKind::EndpointRemoved, "GET /rest/chatbot/status"),
                (ChangeKind::ParamsChanged, "GET /rest/user/whoami"),
            ]
        );
        assert_eq!(d.changes[1].added, vec!["fields".to_string()]);
    }

    #[test]
    fn unstable_endpoints_are_suppressed_not_reported() {
        let before = surface(&[]);
        let mut after = surface(&[("GET", "/rest/continue-code", &[])]);
        after.unstable.insert(key("GET", "/rest/continue-code"));
        let d = diff(&before, &after);
        assert!(d.changes.is_empty());
        assert_eq!(d.suppressed, vec![key("GET", "/rest/continue-code")]);
    }

    #[test]
    fn script_churn_is_informational() {
        let before = surface(&[]);
        let mut after = surface(&[]);
        after.scripts.insert("/confetti-*.js".into());
        let d = diff(&before, &after);
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.significant().count(), 0);
    }
}

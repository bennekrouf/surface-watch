//! Findings: promoted from diff items, retested against later captures,
//! exported for the report.
//!
//! Stored as one `findings.json` per engagement. A finding cites the REA
//! Evidence IDs of the captures it came from, and carries a [`Check`] where
//! a capture can confirm or rule it out, so a retest is a lookup in the
//! latest surface rather than a judgement call.
//!
//! A retest never reports a finding fixed just because the crawl did not see
//! it: an endpoint missing from a capture may still answer on the server.
//! That case is `Unknown`.

use crate::diff::{Change, ChangeKind};
use crate::flags::Flag;
use crate::model::{Capture, Check, Engagement, Finding, Retest, RetestStatus, Severity};
use crate::store;
use crate::surface::{Endpoint, EndpointKey, Surface};
use crate::watch;
use chrono::Utc;
use std::path::PathBuf;

pub fn findings_path(engagement_id: &str) -> PathBuf {
    store::engagement_dir(engagement_id).join("findings.json")
}

pub fn load(engagement_id: &str) -> Vec<Finding> {
    std::fs::read_to_string(findings_path(engagement_id))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Read-modify-write of an engagement's findings under the store lock, so
/// two windows editing findings cannot lose each other's change.
pub fn update<T>(engagement_id: &str, f: impl FnOnce(&mut Vec<Finding>) -> T) -> Result<T, String> {
    store::with_lock(|| {
        let mut list = load(engagement_id);
        let out = f(&mut list);
        let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
        store::write_locked(&findings_path(engagement_id), &json).map_err(|e| e.to_string())?;
        Ok(out)
    })
}

/// `F-001`, `F-002`, … — one past the highest number in use, so deleting a
/// finding never makes its ID point at a different one later.
pub fn next_id(existing: &[Finding]) -> String {
    let max = existing
        .iter()
        .filter_map(|f| f.id.strip_prefix("F-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("F-{:03}", max + 1)
}

/// The reference a diff item gets in `Finding::diff_refs`.
pub fn diff_ref(after: &Capture, change: &Change) -> String {
    format!("{}/{}", after.id, change.subject)
}

/// A draft finding from one diff item. Title, severity and wording are
/// starting points the tester edits.
pub fn promote(
    change: &Change,
    target_id: &str,
    before: &Capture,
    after: &Capture,
    existing: &[Finding],
) -> Finding {
    let key = parse_key(&change.subject);
    let endpoint = |status| {
        key.clone().map(|k| Check::Endpoint {
            method: k.method,
            path: k.path,
            status,
        })
    };
    let first = |v: &[String]| v.first().cloned().unwrap_or_default();
    let s = &change.subject;

    let (title, severity, check) = match change.kind {
        ChangeKind::EndpointAdded => (format!("New endpoint {s}"), Severity::Info, endpoint(None)),
        ChangeKind::EndpointRemoved => (
            format!("{s} no longer requested by the page"),
            Severity::Info,
            None,
        ),
        ChangeKind::StatusChanged => {
            let status: Option<u16> = change.added.first().and_then(|x| x.parse().ok());
            let sev = if status.is_some_and(|x| x >= 500) {
                Severity::Low
            } else {
                Severity::Info
            };
            (
                format!("{s} now answers {}", first(&change.added)),
                sev,
                endpoint(status),
            )
        }
        ChangeKind::ParamsChanged => {
            let param = first(&change.added);
            (
                format!("New query parameter `{param}` on {s}"),
                Severity::Info,
                key.clone().map(|k| Check::Param {
                    method: k.method,
                    path: k.path,
                    param,
                }),
            )
        }
        ChangeKind::SecurityHeadersChanged if !change.removed.is_empty() => {
            let header = first(&change.removed);
            (
                format!("{s} no longer sends {header}"),
                Severity::Low,
                key.clone().map(|k| Check::HeaderMissing {
                    method: k.method,
                    path: k.path,
                    header,
                }),
            )
        }
        ChangeKind::SecurityHeadersChanged => (
            format!("Security headers changed on {s}"),
            Severity::Info,
            None,
        ),
        ChangeKind::ThirdPartyAdded => (
            format!("Page now loads from {s}"),
            Severity::Info,
            Some(Check::ThirdParty { origin: s.clone() }),
        ),
        ChangeKind::ThirdPartyRemoved | ChangeKind::ScriptsChanged => (
            format!("{}: {s}", change.kind.label()),
            Severity::Info,
            None,
        ),
    };

    let mut evidence_ids = before.evidence_ids.clone();
    evidence_ids.extend(after.evidence_ids.iter().cloned());

    Finding {
        id: next_id(existing),
        target_id: target_id.to_string(),
        title,
        severity,
        cvss_vector: None,
        description: format!(
            "Found by comparing capture {} with capture {}.",
            before.id, after.id
        ),
        remediation: String::new(),
        evidence_ids,
        diff_refs: vec![diff_ref(after, change)],
        check,
        created_at: Utc::now(),
        retests: vec![],
    }
}

/// A draft finding from something present in one capture's surface: an
/// endpoint (with its most important flag, if any) or a page-level flag.
pub fn promote_surface(
    target_id: &str,
    capture: &Capture,
    endpoint: Option<(&EndpointKey, &Endpoint)>,
    flag: Option<&Flag>,
    existing: &[Finding],
) -> Finding {
    let ep_check = |status: Option<u16>| {
        endpoint.map(|(k, _)| Check::Endpoint {
            method: k.method.clone(),
            path: k.path.clone(),
            status,
        })
    };
    let subject = endpoint.map(|(k, _)| k.to_string()).unwrap_or_default();
    let (title, severity, check) = match flag {
        Some(Flag::ServerError(s)) => (
            format!("{subject} answers {s} to an ordinary request"),
            Severity::Low,
            ep_check(Some(*s)),
        ),
        Some(Flag::SensitivePath(s)) => (
            format!("{subject} is reachable without logging in"),
            Severity::Low,
            ep_check(Some(*s)),
        ),
        Some(Flag::ObjectId) => (
            format!("{subject} takes an object ID: test access control"),
            Severity::Info,
            ep_check(None),
        ),
        Some(Flag::PageHeaderMissing(h)) => (
            format!("Pages are served without {h}"),
            Severity::Low,
            Some(Check::PageHeaderMissing { header: h.clone() }),
        ),
        Some(Flag::AuthRequired | Flag::Unstable) | None => (
            format!("Endpoint {subject}"),
            Severity::Info,
            ep_check(None),
        ),
    };
    let mut description = format!("Observed in capture {}.", capture.id);
    if let Some(f) = flag {
        description = format!("{} {description}", f.explanation());
    }
    if let Some((_, ep)) = endpoint {
        let statuses: Vec<_> = ep.statuses.iter().map(u16::to_string).collect();
        if !statuses.is_empty() {
            description += &format!(" Status seen: {}.", statuses.join(", "));
        }
    }
    let what = flag.map(|f| f.label()).unwrap_or_else(|| "endpoint".into());
    Finding {
        id: next_id(existing),
        target_id: target_id.to_string(),
        title,
        severity,
        cvss_vector: None,
        description,
        remediation: String::new(),
        evidence_ids: capture.evidence_ids.clone(),
        diff_refs: vec![format!("{}/surface/{what}/{subject}", capture.id)],
        check,
        created_at: Utc::now(),
        retests: vec![],
    }
}

fn parse_key(subject: &str) -> Option<EndpointKey> {
    let (method, path) = subject.split_once(' ')?;
    Some(EndpointKey {
        method: method.to_string(),
        path: path.to_string(),
    })
}

/// What `surface` says about `check`, with the reason in words.
pub fn evaluate(check: &Check, surface: &Surface) -> (RetestStatus, String) {
    use RetestStatus::*;
    let lookup = |method: &str, path: &str| {
        let key = EndpointKey {
            method: method.to_string(),
            path: path.to_string(),
        };
        let unstable = surface.unstable.contains(&key);
        (
            surface.endpoints.get(&key).filter(|_| !unstable),
            key,
            unstable,
        )
    };
    let not_seen = |key: &EndpointKey, unstable: bool| {
        if unstable {
            (
                Unknown,
                format!("{key} is unstable between runs of this build."),
            )
        } else {
            (
                Unknown,
                format!("{key} was not requested by the crawled pages; check it directly."),
            )
        }
    };

    match check {
        Check::Endpoint {
            method,
            path,
            status,
        } => match (lookup(method, path), status) {
            ((None, key, unstable), _) => not_seen(&key, unstable),
            ((Some(_), key, _), None) => (StillPresent, format!("{key} is still requested.")),
            ((Some(ep), key, _), Some(want)) => {
                let seen: Vec<_> = ep.statuses.iter().map(u16::to_string).collect();
                if ep.statuses.contains(want) {
                    (StillPresent, format!("{key} still answers {want}."))
                } else {
                    (Fixed, format!("{key} now answers {}.", seen.join(", ")))
                }
            }
        },
        Check::Param {
            method,
            path,
            param,
        } => match lookup(method, path) {
            (None, key, unstable) => not_seen(&key, unstable),
            (Some(ep), key, _) if ep.params.contains(param) => (
                StillPresent,
                format!("{key} is still called with `{param}`."),
            ),
            (Some(_), key, _) => (Fixed, format!("{key} is no longer called with `{param}`.")),
        },
        Check::HeaderMissing {
            method,
            path,
            header,
        } => match lookup(method, path) {
            (None, key, unstable) => not_seen(&key, unstable),
            (Some(ep), key, _) if ep.security_headers.contains(header) => {
                (Fixed, format!("{key} sends {header} again."))
            }
            (Some(_), key, _) => (
                StillPresent,
                format!("{key} still answers without {header}."),
            ),
        },
        Check::PageHeaderMissing { header } => {
            if surface.documents_seen == 0 {
                (Unknown, "The capture loaded no HTML page.".into())
            } else if surface.page_security_headers.contains(header) {
                (Fixed, format!("Pages are now served with {header}."))
            } else {
                (
                    StillPresent,
                    format!("Pages are still served without {header}."),
                )
            }
        }
        Check::ThirdParty { origin } => {
            if surface.third_party_origins.contains(origin) {
                (StillPresent, format!("The page still loads from {origin}."))
            } else {
                // Third parties load from the page itself, so absence across
                // the same crawl is real evidence, unlike a missing endpoint.
                (
                    Fixed,
                    format!("The crawled pages no longer load from {origin}."),
                )
            }
        }
    }
}

/// Retests `finding` against one capture's surface.
pub fn retest(finding: &Finding, capture: &Capture, surface: &Surface) -> Option<Retest> {
    let (status, note) = evaluate(finding.check.as_ref()?, surface);
    Some(Retest {
        capture_id: capture.id.clone(),
        at: Utc::now(),
        status,
        note,
        manual: false,
    })
}

/// Appends `retest`, replacing an earlier automatic retest against the same
/// capture so retesting twice does not duplicate the history.
pub fn record(finding: &mut Finding, retest: Retest) {
    if let Some(last) = finding.retests.last() {
        if !last.manual && !retest.manual && last.capture_id == retest.capture_id {
            finding.retests.pop();
        }
    }
    finding.retests.push(retest);
}

/// Retests `finding` against several captures, newest first, and keeps the
/// newest answer that is not `Unknown`. A HAR covering one flow must not
/// turn a crawl's clear answer about another endpoint into "unknown".
pub fn retest_best(finding: &Finding, candidates: &[(Capture, Surface)]) -> Option<Retest> {
    let results: Vec<Retest> = candidates
        .iter()
        .filter_map(|(c, s)| retest(finding, c, s))
        .collect();
    results
        .iter()
        .find(|r| r.status != RetestStatus::Unknown)
        .or(results.first())
        .cloned()
}

/// Retests the findings `select` picks against the latest capture of each
/// source (crawl, HAR) of their target. Returns how many were retested;
/// findings without a check, or whose target has no capture, are left alone.
pub fn retest_latest(
    engagement: &Engagement,
    select: impl Fn(&Finding) -> bool,
) -> Result<usize, String> {
    use std::collections::HashMap;
    // Surfaces are built before taking the store lock: reading captures is
    // slow, and the lock should only cover the findings file itself.
    let mut latest: HashMap<String, Vec<(Capture, Surface)>> = HashMap::new();
    for t in &engagement.targets {
        let dir = watch::target_dir(&engagement.id, &t.id);
        let captures = watch::list_captures(&dir);
        let mut newest: Vec<&Capture> = vec![];
        for c in watch::primary_captures(&captures).into_iter().rev() {
            if !newest.iter().any(|n| n.source == c.source) {
                newest.push(c);
            }
        }
        let mut surfaces = vec![];
        for c in newest {
            surfaces.push((c.clone(), watch::load_surface(&dir, &t.origin, &c.id)?));
        }
        latest.insert(t.id.clone(), surfaces);
    }
    update(&engagement.id, |list| {
        let mut n = 0;
        for f in list.iter_mut().filter(|f| select(f)) {
            let candidates = latest
                .get(&f.target_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if let Some(r) = retest_best(f, candidates) {
                record(f, r);
                n += 1;
            }
        }
        n
    })
}

/// Findings that rely on `capture`: they cite its evidence, were promoted
/// from it, or were retested against it.
pub fn citing<'a>(findings: &'a [Finding], capture: &Capture) -> Vec<&'a Finding> {
    let prefix = format!("{}/", capture.id);
    findings
        .iter()
        .filter(|f| {
            f.evidence_ids
                .iter()
                .any(|e| capture.evidence_ids.contains(e))
                || f.diff_refs.iter().any(|d| d.starts_with(&prefix))
                || f.retests.iter().any(|r| r.capture_id == capture.id)
        })
        .collect()
}

pub fn status_label(status: RetestStatus) -> &'static str {
    match status {
        RetestStatus::Open => "open",
        RetestStatus::Fixed => "fixed",
        RetestStatus::StillPresent => "still present",
        RetestStatus::Unknown => "unknown",
    }
}

pub fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "Info",
        Severity::Low => "Low",
        Severity::Medium => "Medium",
        Severity::High => "High",
        Severity::Critical => "Critical",
    }
}

/// The engagement's findings as a Markdown report, highest severity first,
/// each with its evidence and retest history.
pub fn to_markdown(engagement: &Engagement, findings: &[Finding], captures: &[Capture]) -> String {
    let mut out = format!("# {} · {}\n\n", engagement.client, engagement.name);
    if let (Some(s), Some(e)) = (engagement.window_start, engagement.window_end) {
        out += &format!("Test window: {s} to {e}\n\n");
    }
    if !engagement.authorisation.is_empty() {
        out += &format!("Authorisation: {}\n\n", engagement.authorisation);
    }
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.id.cmp(&b.id)));

    out += "| ID | Severity | Status | Title |\n|---|---|---|---|\n";
    for f in &sorted {
        out += &format!(
            "| {} | {} | {} | {} |\n",
            f.id,
            severity_label(f.severity),
            status_label(f.status()),
            f.title.replace('|', "\\|")
        );
    }

    for f in sorted {
        let target = engagement
            .targets
            .iter()
            .find(|t| t.id == f.target_id)
            .map(|t| t.origin.as_str())
            .unwrap_or(&f.target_id);
        out += &format!("\n## {} {}\n\n", f.id, f.title);
        out += &format!(
            "- Severity: {}\n- Status: {}\n- Target: {target}\n",
            severity_label(f.severity),
            status_label(f.status())
        );
        if let Some(v) = &f.cvss_vector {
            out += &format!("- CVSS: `{v}`\n");
        }
        if !f.description.trim().is_empty() {
            out += &format!("\n### Description\n\n{}\n", f.description.trim());
        }
        if !f.remediation.trim().is_empty() {
            out += &format!("\n### Remediation\n\n{}\n", f.remediation.trim());
        }
        out += "\n### Evidence\n\n";
        let cited: Vec<&Capture> = captures
            .iter()
            .filter(|c| c.evidence_ids.iter().any(|e| f.evidence_ids.contains(e)))
            .collect();
        for c in &cited {
            let tools = [
                c.toolchain.rea.as_ref().map(|v| format!("REA {v}")),
                c.toolchain.browser.clone(),
                c.toolchain.node.as_ref().map(|v| format!("Node {v}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            out += &format!(
                "- Capture {} ({}){}\n",
                c.id,
                c.taken_at.format("%Y-%m-%d %H:%M UTC"),
                if tools.is_empty() {
                    String::new()
                } else {
                    format!(", {tools}")
                }
            );
            for e in &c.evidence_ids {
                out += &format!("  - `{e}`\n");
            }
        }
        if cited.is_empty() {
            for e in &f.evidence_ids {
                out += &format!("- `{e}`\n");
            }
        }
        if !f.retests.is_empty() {
            out += "\n### Retests\n\n";
            for r in &f.retests {
                out += &format!(
                    "- {}: {}{} (capture {}). {}\n",
                    r.at.format("%Y-%m-%d"),
                    status_label(r.status),
                    if r.manual { ", set by hand" } else { "" },
                    r.capture_id,
                    r.note
                );
            }
        }
    }
    out += "\nREA evidence is static and network observation: endpoints are those the crawled pages requested, and server-side behaviour beyond the observed responses is not covered.\n";
    out
}

/// Writes the Markdown report into the engagement's `exports` folder and
/// returns its path.
pub fn export_markdown(engagement: &Engagement, captures: &[Capture]) -> Result<PathBuf, String> {
    let md = to_markdown(engagement, &load(&engagement.id), captures);
    let path = store::engagement_dir(&engagement.id)
        .join("exports")
        .join(format!(
            "findings-{}.md",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
    store::write_private(&path, &md).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests;

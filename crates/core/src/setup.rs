//! Creating engagements and targets, from the app's form or the CLI.
//!
//! Validation lives here, not in either frontend, so a target typed into the
//! form and one passed to `surfacewatch target add` follow the same rules.
//! Every error says what to type instead.

use crate::model::{CaptureProfile, Engagement, Target};
use crate::store;
use chrono::NaiveDate;

/// What the user typed for a new engagement.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngagementDraft {
    pub client: String,
    pub name: String,
    /// `YYYY-MM-DD`, or empty for no bound.
    pub window_start: String,
    pub window_end: String,
    pub authorisation: String,
}

/// What the user typed for a new target.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetDraft {
    pub origin: String,
    pub label: String,
    /// One page per line; blank lines and `#` comments are ignored.
    pub pages: String,
    pub batch_size: usize,
    pub settle_ms: u64,
}

impl Default for TargetDraft {
    fn default() -> Self {
        let profile = CaptureProfile::default();
        TargetDraft {
            origin: String::new(),
            label: String::new(),
            pages: String::new(),
            batch_size: profile.batch_size,
            settle_ms: profile.settle_ms,
        }
    }
}

/// `https://Shop.Example.com/` → `https://shop.example.com`.
///
/// Only an origin is accepted: captures are scoped to it, and a path here
/// would silently exclude the rest of the site from "in scope".
pub fn normalize_origin(input: &str) -> Result<String, String> {
    let s = input.trim();
    let (scheme, rest) = s
        .split_once("://")
        .ok_or("Enter the origin with its scheme, e.g. https://shop.example.com")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "{scheme}:// is not supported: use http:// or https://"
        ));
    }
    let (host, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err("The origin needs a host name, e.g. https://shop.example.com".into());
    }
    if host.contains('@') {
        return Err("Leave credentials out of the origin.".into());
    }
    if !(path.is_empty() || path == "/") {
        return Err(format!(
            "Enter only the origin ({scheme}://{host}); put paths like {path} in the pages list."
        ));
    }
    Ok(format!("{scheme}://{}", host.to_ascii_lowercase()))
}

/// One page per line, as typed. `login` becomes `/login`; `#/login` stays a
/// hash route. Duplicates are dropped, order kept.
pub fn parse_pages(text: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("# ") || line == "#" {
            continue;
        }
        if line.contains("://") {
            return Err(format!(
                "{line}: list pages relative to the origin, e.g. /login or #/login"
            ));
        }
        let page = if line.starts_with('/') || line.starts_with("#/") {
            line.to_string()
        } else {
            format!("/{line}")
        };
        if !out.contains(&page) {
            out.push(page);
        }
    }
    if out.is_empty() {
        return Err("List at least one page to visit, e.g. / or #/login".into());
    }
    Ok(out)
}

fn parse_date(label: &str, s: &str) -> Result<Option<NaiveDate>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| format!("{label} {s:?} is not a date (YYYY-MM-DD)"))
}

/// A folder-safe ID from a name: `Acme Retail Q4` → `acme-retail-q4`,
/// suffixed `-2`, `-3`, … when `taken` already has it.
pub fn slug(name: &str, taken: &[String]) -> String {
    let mut base = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c);
        } else if !base.ends_with('-') && !base.is_empty() {
            base.push('-');
        }
    }
    let base = base
        .trim_end_matches('-')
        .chars()
        .take(48)
        .collect::<String>();
    let base = if base.is_empty() {
        "item".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Checks a draft and builds the engagement, without saving it.
pub fn build_engagement(
    draft: &EngagementDraft,
    taken_ids: &[String],
) -> Result<Engagement, String> {
    let client = draft.client.trim();
    let name = draft.name.trim();
    if client.is_empty() {
        return Err("Enter the client's name.".into());
    }
    if name.is_empty() {
        return Err("Enter a name for the engagement, e.g. Retail Q4.".into());
    }
    if draft.authorisation.trim().is_empty() {
        return Err(
            "Record who authorised this test and where (e.g. statement of work reference). Captures are only run against authorised targets."
                .into(),
        );
    }
    let start = parse_date("Start date", &draft.window_start)?;
    let end = parse_date("End date", &draft.window_end)?;
    if let (Some(s), Some(e)) = (start, end) {
        if e < s {
            return Err(format!(
                "The test window ends ({e}) before it starts ({s})."
            ));
        }
    }
    Ok(Engagement {
        id: slug(&format!("{client} {name}"), taken_ids),
        client: client.to_string(),
        name: name.to_string(),
        window_start: start,
        window_end: end,
        authorisation: draft.authorisation.trim().to_string(),
        targets: vec![],
    })
}

/// Checks a draft and builds the target, without saving it.
pub fn build_target(draft: &TargetDraft, engagement: &Engagement) -> Result<Target, String> {
    let origin = normalize_origin(&draft.origin)?;
    if engagement.targets.iter().any(|t| t.origin == origin) {
        return Err(format!("{origin} is already a target of this engagement."));
    }
    let pages = parse_pages(&draft.pages)?;
    if !(1..=20).contains(&draft.batch_size) {
        return Err("Pages per batch must be between 1 and 20 (6 works with REA 6.4.0).".into());
    }
    if !(200..=30_000).contains(&draft.settle_ms) {
        return Err("Settle time must be between 200 and 30000 ms.".into());
    }
    let label = match draft.label.trim() {
        "" => origin.clone(),
        l => l.to_string(),
    };
    let taken: Vec<String> = engagement.targets.iter().map(|t| t.id.clone()).collect();
    let host = origin.split_once("://").map(|(_, h)| h).unwrap_or(&origin);
    Ok(Target {
        id: slug(host, &taken),
        origin,
        label,
        profile: CaptureProfile {
            pages,
            batch_size: draft.batch_size,
            settle_ms: draft.settle_ms,
        },
    })
}

/// Validates and saves a new engagement with its first target.
pub fn create_engagement(
    draft: &EngagementDraft,
    target: &TargetDraft,
) -> Result<Engagement, String> {
    let taken: Vec<String> = store::load_engagements()
        .into_iter()
        .map(|e| e.id)
        .collect();
    let mut e = build_engagement(draft, &taken)?;
    let t = build_target(target, &e)?;
    e.targets.push(t);
    store::save_engagement(&e).map_err(|err| format!("Could not save: {err}"))?;
    Ok(e)
}

/// Validates and adds a target to an existing engagement.
pub fn add_target(engagement_id: &str, draft: &TargetDraft) -> Result<Target, String> {
    store::with_lock(|| {
        let mut e = store::load_engagements()
            .into_iter()
            .find(|e| e.id == engagement_id)
            .ok_or_else(|| format!("No engagement {engagement_id:?}."))?;
        let t = build_target(draft, &e)?;
        e.targets.push(t.clone());
        let json = serde_json::to_string_pretty(&e).map_err(|err| err.to_string())?;
        store::write_locked(&store::engagement_dir(&e.id).join("engagement.json"), &json)
            .map_err(|err| format!("Could not save: {err}"))?;
        Ok(t)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> EngagementDraft {
        EngagementDraft {
            client: "Acme".into(),
            name: "Retail Q4".into(),
            window_start: "2026-10-01".into(),
            window_end: "2026-10-31".into(),
            authorisation: "SOW-2026-114, signed by J. Doe".into(),
        }
    }

    #[test]
    fn origins_are_normalised_and_paths_refused() {
        assert_eq!(
            normalize_origin(" https://Shop.Example.com/ ").unwrap(),
            "https://shop.example.com"
        );
        assert_eq!(
            normalize_origin("http://127.0.0.1:3012").unwrap(),
            "http://127.0.0.1:3012"
        );
        assert!(normalize_origin("shop.example.com")
            .unwrap_err()
            .contains("scheme"));
        assert!(normalize_origin("ftp://x")
            .unwrap_err()
            .contains("not supported"));
        assert!(normalize_origin("https://shop.example.com/login")
            .unwrap_err()
            .contains("pages list"));
        assert!(normalize_origin("https://u:p@shop.example.com")
            .unwrap_err()
            .contains("credentials"));
    }

    #[test]
    fn pages_accept_paths_hash_routes_and_bare_names() {
        let pages = parse_pages("/\n#/login\n\nsearch\n# a comment\n#/login\n").unwrap();
        assert_eq!(pages, vec!["/", "#/login", "/search"]);
        assert!(parse_pages("https://x/login")
            .unwrap_err()
            .contains("relative"));
        assert!(parse_pages("\n# only a comment\n").is_err());
    }

    #[test]
    fn ids_are_folder_safe_and_unique() {
        assert_eq!(slug("Acme Retail Q4", &[]), "acme-retail-q4");
        assert_eq!(slug("  Ünïcode / ../etc  ", &[]), "n-code-etc");
        assert_eq!(slug("Acme", &["acme".into()]), "acme-2");
        assert_eq!(slug("../..", &[]), "item");
    }

    #[test]
    fn an_engagement_needs_an_authorisation_and_a_sane_window() {
        assert_eq!(
            build_engagement(&draft(), &[]).unwrap().id,
            "acme-retail-q4"
        );
        let mut d = draft();
        d.authorisation = "  ".into();
        assert!(build_engagement(&d, &[])
            .unwrap_err()
            .contains("authorised"));
        let mut d = draft();
        d.window_end = "2026-09-01".into();
        assert!(build_engagement(&d, &[])
            .unwrap_err()
            .contains("before it starts"));
        let mut d = draft();
        d.window_start = "1 Oct".into();
        assert!(build_engagement(&d, &[])
            .unwrap_err()
            .contains("not a date"));
    }

    #[test]
    fn a_target_gets_an_id_from_its_host_and_cannot_be_added_twice() {
        let mut e = build_engagement(&draft(), &[]).unwrap();
        let td = TargetDraft {
            origin: "https://shop.acme.example/".into(),
            pages: "/\n#/login".into(),
            ..Default::default()
        };
        let t = build_target(&td, &e).unwrap();
        assert_eq!(t.id, "shop-acme-example");
        assert_eq!(t.label, "https://shop.acme.example");
        assert_eq!(t.profile.batch_size, 6);
        e.targets.push(t);
        assert!(build_target(&td, &e)
            .unwrap_err()
            .contains("already a target"));
    }
}

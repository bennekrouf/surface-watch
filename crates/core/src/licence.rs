//! SurfaceWatch Pro: the licence on this computer.
//!
//! Pro comes with a monthly plan bought on mayorana.ch. The plan's licence
//! number (`ABCDE-FGHJK-LMNPQ-RSTUV`, emailed when it is bought) is saved here
//! and checked with api0 when the app starts and then once a day (the CLI
//! checks when a day has passed since the last check):
//!
//! ```text
//! GET https://gateway.api0.ai/v1/licence
//! Authorization: Bearer <licence>
//! X-Client-App: surface-watch
//! ```
//!
//! The answer names the plan, the apps it makes Pro, whether it is paid up and
//! the AI credits left. It is kept on disk, so the CLI and a SurfaceWatch
//! started offline read the last one: Pro lasts a week
//! (`desktop_kit::licence::GRACE`) after the last check that got through, then
//! waits for the next.
//!
//! Without Pro, SurfaceWatch keeps [`FREE_ENGAGEMENTS`] engagement: creating
//! another one, from the app or the CLI, asks for Pro. Engagements already on
//! disk are never locked or removed.
//!
//! The calls block, for the CLI; the app makes them off the UI thread.
//!
//! What every app agrees on — the number's format, api0's answer, the grace
//! period, the shared file — is `desktop_kit::licence`. Here: the requests,
//! where SurfaceWatch keeps its own answer, and its engagement limit.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use desktop_kit::licence::{
    check_due, free_reply, load_shared, normalise, plausible_email, reconcile, Shared, GATEWAY,
};
pub use desktop_kit::licence::{Answer, FreeError, Saved, Status};
use serde::Deserialize;

/// The product id api0 knows SurfaceWatch by: a plan lists it to make
/// SurfaceWatch Pro. It keeps the app's earlier name, which licences carry.
pub const PRODUCT: &str = "surface-watch";

/// Where "Buy SurfaceWatch Pro" leads.
pub const BUY_URL: &str = "https://mayorana.ch/en/apps/surfacewatch";

const LICENCE_FILE: &str = "licence.json";

/// How many engagements the free version keeps.
pub const FREE_ENGAGEMENTS: usize = 1;

/// What `saved` amounts to for SurfaceWatch at `now` (seconds since 1970).
pub fn status_at(saved: Option<&Saved>, now: u64) -> Status {
    desktop_kit::licence::status_at(saved, now, PRODUCT)
}

/// What SurfaceWatch makes of a licence status.
pub trait StatusExt {
    /// What the window title adds after "SurfaceWatch <version>".
    fn title_badge(&self) -> &'static str;
    /// What `surfacewatch-cli licence` prints.
    fn describe(&self) -> String;
}

impl StatusExt for Status {
    fn title_badge(&self) -> &'static str {
        self.title_badge_for(PRODUCT)
    }

    fn describe(&self) -> String {
        match self {
            Status::Free => format!(
                "Free version: keeps {FREE_ENGAGEMENTS} engagement. SurfaceWatch Pro keeps any number: {BUY_URL}"
            ),
            Status::Pro(a) => format!(
                "SurfaceWatch Pro: {} plan for {}. {} AI credits left this month. {}.",
                a.plan.name,
                a.email,
                a.credits.left,
                a.renewal()
            ),
            Status::NotPro(a) if a.is_free() => format!(
                "Free: {} AI credits left this month, for {}. {}. The free version keeps \
                 {FREE_ENGAGEMENTS} engagement; SurfaceWatch Pro keeps any number: {BUY_URL}",
                a.credits.left,
                a.email,
                a.renewal()
            ),
            Status::NotPro(a) if !a.active => format!(
                "The subscription for {} isn't active, so this runs as the free version. Renew it: {BUY_URL}",
                a.email
            ),
            Status::NotPro(a) => format!(
                "The {} plan doesn't include SurfaceWatch, so this runs as the free version: {BUY_URL}",
                a.plan.name
            ),
            Status::Offline(_) => "The licence couldn't be checked for a week (offline?), so this \
                 runs as the free version until it can."
                .into(),
            Status::Unknown(_) => "The licence number on this computer isn't known. Check it \
                 against the email and run `surfacewatch-cli licence <number>` again."
                .into(),
        }
    }
}

/// Writes the shared licence file.
fn save_shared(text: &str) {
    desktop_kit::licence::save_shared(text, PRODUCT);
}

/// Why another engagement can't be created, when `existing` are already on
/// disk; `None` when it can.
pub fn engagement_limit(status: &Status, existing: usize) -> Option<String> {
    if status.is_pro() || existing < FREE_ENGAGEMENTS {
        return None;
    }
    Some(format!(
        "The free version of SurfaceWatch keeps one engagement, and this computer already has {existing}. \
         SurfaceWatch Pro keeps any number, and is the licence for commercial use: subscribe at {BUY_URL}, \
         then paste the licence number under Get Pro… in the app, or run `surfacewatch-cli licence <number>`."
    ))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The folder the licence (and the free version's choices) are kept in.
pub fn dir() -> PathBuf {
    crate::store::data_dir()
}

fn licence_path() -> PathBuf {
    dir().join(LICENCE_FILE)
}

fn load_own() -> Option<Saved> {
    let text = std::fs::read_to_string(licence_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn load() -> Option<Saved> {
    let shared = load_shared();
    if shared == Shared::Removed {
        let _ = std::fs::remove_file(licence_path());
    }
    let (saved, share) = reconcile(shared, load_own());
    if share {
        if let Some(saved) = &saved {
            save_shared(&saved.licence);
        }
    }
    saved
}

fn save(saved: &Saved) -> Result<(), String> {
    let json = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
    // Atomic and owner-only, like everything else the app keeps.
    std::fs::create_dir_all(dir())
        .and_then(|()| crate::store::write_private(&licence_path(), &json))
        .map_err(|e| format!("The licence couldn't be saved: {e}"))
}

/// The licence as of the last check, without asking api0.
pub fn current() -> Status {
    status_at(load().as_ref(), now())
}

/// Why a check didn't give an answer.
#[derive(Debug)]
enum CheckError {
    /// api0 doesn't know this licence number.
    Unknown,
    /// Offline, api0 down, or an answer that made no sense.
    Unreachable(String),
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default()
}

fn ask(licence: &str) -> Result<Answer, CheckError> {
    let resp = client()
        .get(format!("{GATEWAY}/v1/licence"))
        .bearer_auth(licence)
        .header("X-Client-App", PRODUCT)
        .send()
        .map_err(|e| CheckError::Unreachable(e.to_string()))?;
    match resp.status().as_u16() {
        200 => resp
            .json::<Answer>()
            .map_err(|e| CheckError::Unreachable(e.to_string())),
        401 => Err(CheckError::Unknown),
        code => Err(CheckError::Unreachable(format!("api0 answered {code}"))),
    }
}

/// Checks `typed` with api0 and, if api0 knows it, saves it.
pub fn activate(typed: &str) -> Result<Status, String> {
    let licence = normalise(typed).ok_or(
        "This isn't a licence number. It has 20 letters and digits, like ABCDE-FGHJK-LMNPQ-RSTUV.",
    )?;
    let answer = match ask(&licence) {
        Ok(a) => a,
        Err(CheckError::Unknown) => {
            return Err("This licence number isn't known. Check it against the email.".into())
        }
        Err(CheckError::Unreachable(e)) => {
            return Err(format!(
                "The licence couldn't be checked ({e}). Check the connection and try again."
            ))
        }
    };
    let saved = Saved {
        licence,
        answer: Some(answer),
        checked: now(),
    };
    save(&saved)?;
    save_shared(&saved.licence);
    Ok(status_at(Some(&saved), now()))
}

/// Checks the saved licence with api0 again, when `force`d or a day after the
/// last check, and keeps the answer. Offline, the last answer stands.
pub fn refresh(force: bool) -> Status {
    let Some(mut saved) = load() else {
        return Status::Free;
    };
    if !force && !check_due(&saved, now()) {
        return status_at(Some(&saved), now());
    }
    match ask(&saved.licence) {
        Ok(answer) => {
            saved.answer = Some(answer);
            saved.checked = now();
            let _ = save(&saved);
        }
        Err(CheckError::Unknown) => {
            saved.answer = None;
            let _ = save(&saved);
        }
        Err(CheckError::Unreachable(_)) => {}
    }
    status_at(Some(&saved), now())
}

/// The address of the page where the subscription is changed or cancelled.
pub fn portal_url(licence: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Portal {
        url: String,
    }
    let resp = client()
        .post(format!("{GATEWAY}/v1/licence/portal"))
        .bearer_auth(licence)
        .header("X-Client-App", PRODUCT)
        .send()
        .map_err(|e| format!("The subscription page couldn't be opened ({e})."))?;
    if !resp.status().is_success() {
        return Err(format!(
            "The subscription page couldn't be opened (api0 answered {}).",
            resp.status().as_u16()
        ));
    }
    let portal: Portal = resp
        .json()
        .map_err(|e| format!("The subscription page couldn't be opened ({e})."))?;
    Ok(portal.url)
}

/// Asks api0 to email `email` a licence for the free allowance of AI credits.
/// The licence number itself only goes by email.
pub fn request_free_credits(email: &str) -> Result<String, FreeError> {
    if !plausible_email(email) {
        return Err(FreeError::Refused(
            "Enter the email the licence should go to.".into(),
        ));
    }
    let resp = client()
        .post(format!("{GATEWAY}/api/plans/free"))
        .header("X-Client-App", PRODUCT)
        .json(&serde_json::json!({ "email": email.trim() }))
        .send()
        .map_err(|e| {
            FreeError::Refused(format!("The free AI credits couldn't be requested ({e})."))
        })?;
    let status = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();
    free_reply(status, &body)
}

/// Removes the licence from this computer (to move it to another one), for
/// every Mayorana app on it.
pub fn deactivate() {
    let _ = std::fs::remove_file(licence_path());
    save_shared("");
}

#[cfg(test)]
mod tests {
    use super::*;
    use desktop_kit::licence::Plan;

    const DAY: u64 = 24 * 3600;

    fn answer(products: &[&str], active: bool) -> Answer {
        Answer {
            licence: "ABCDE-FGHJK-LMNPQ-RSTUV".into(),
            email: "dev@example.ch".into(),
            plan: Plan {
                id: "pro".into(),
                name: "Pro".into(),
            },
            products: products.iter().map(|p| p.to_string()).collect(),
            active,
            status: "active".into(),
            period_end: "2026-11-21T10:00:00Z".into(),
            ..Answer::default()
        }
    }

    fn saved(answer: Answer, checked: u64) -> Saved {
        Saved {
            licence: answer.licence.clone(),
            answer: Some(answer),
            checked,
        }
    }

    #[test]
    fn the_free_version_keeps_one_engagement() {
        assert!(engagement_limit(&Status::Free, 0).is_none());
        assert!(engagement_limit(&Status::Free, 1).is_some());
        let now = 1_000 * DAY;
        let pro = status_at(Some(&saved(answer(&["surface-watch"], true), now)), now);
        assert!(engagement_limit(&pro, 3).is_none());
        let stale = status_at(
            Some(&saved(answer(&["surface-watch"], true), now)),
            now + 8 * DAY,
        );
        assert!(engagement_limit(&stale, 1).is_some());
    }
}

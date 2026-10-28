//! SurfaceWatch Pro: what the free version keeps, and the licence calls the
//! CLI makes.
//!
//! The licence itself (the number, api0's answer, the daily check, the shared
//! file) is `desktop_kit::licence`, the same in every mayorana app; the core
//! names SurfaceWatch's product id and where it keeps api0's last answer in
//! [`crate::kit`]. The app and the CLI read the same answer.
//!
//! Without Pro, SurfaceWatch keeps [`FREE_ENGAGEMENTS`] engagement: creating
//! another one, from the app or the CLI, asks for Pro. Engagements already on
//! disk are never locked or removed.

use std::future::Future;

pub use desktop_kit::licence::{buy_url, current, deactivate, title_badge, FreeError, Status};

/// The product id api0 knows SurfaceWatch by: a plan lists it to make
/// SurfaceWatch Pro. It keeps the app's earlier name, which licences carry.
pub const PRODUCT: &str = "surface-watch";

/// How many engagements the free version keeps.
pub const FREE_ENGAGEMENTS: usize = 1;

/// What SurfaceWatch makes of a licence status.
pub trait StatusExt {
    /// What `surfacewatch-cli licence` prints.
    fn describe(&self) -> String;
}

impl StatusExt for Status {
    fn describe(&self) -> String {
        match self {
            Status::Free => format!(
                "Free version: keeps {FREE_ENGAGEMENTS} engagement. SurfaceWatch Pro keeps any number: {}",
                buy_url()
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
                 {FREE_ENGAGEMENTS} engagement; SurfaceWatch Pro keeps any number: {}",
                a.credits.left,
                a.email,
                a.renewal(),
                buy_url()
            ),
            Status::NotPro(a) if !a.active => format!(
                "The subscription for {} isn't active, so this runs as the free version. Renew it: {}",
                a.email,
                buy_url()
            ),
            Status::NotPro(a) => format!(
                "The {} plan doesn't include SurfaceWatch, so this runs as the free version: {}",
                a.plan.name,
                buy_url()
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

/// Why another engagement can't be created, when `existing` are already on
/// disk; `None` when it can.
pub fn engagement_limit(status: &Status, existing: usize) -> Option<String> {
    if status.is_pro() || existing < FREE_ENGAGEMENTS {
        return None;
    }
    Some(format!(
        "The free version of SurfaceWatch keeps one engagement, and this computer already has {existing}. \
         SurfaceWatch Pro keeps any number, and is the licence for commercial use: subscribe at {}, \
         then paste the licence number under Get Pro… in the app, or run `surfacewatch-cli licence <number>`.",
        buy_url()
    ))
}

/// The licence calls, for the CLI, which has no runtime of its own: each
/// runs desktop-kit's request to the end on a small one.
pub mod blocking {
    use super::*;

    fn wait<T>(call: impl Future<Output = T>) -> Result<T, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("The licence couldn't be checked ({e})."))?;
        Ok(runtime.block_on(call))
    }

    /// Checks `typed` with api0 and, if api0 knows it, saves it.
    pub fn activate(typed: &str) -> Result<Status, String> {
        wait(desktop_kit::licence::activate(typed))?
    }

    /// Checks the saved licence with api0 again, when `force`d or due.
    pub fn refresh(force: bool) -> Status {
        wait(desktop_kit::licence::refresh(force)).unwrap_or_else(|_| current())
    }

    /// Asks api0 to email `email` a licence for free AI credits.
    pub fn request_free_credits(email: &str) -> Result<String, FreeError> {
        wait(desktop_kit::licence::request_free_credits(email)).map_err(FreeError::Refused)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desktop_kit::licence::{Answer, Plan, Saved};

    fn status_at(saved: Option<&Saved>, now: u64) -> Status {
        desktop_kit::licence::status_at(saved, now, PRODUCT)
    }

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

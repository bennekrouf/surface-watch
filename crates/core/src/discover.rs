//! Suggesting pages to crawl from the target's own code.
//!
//! A crawl only visits the pages it is given, and hidden pages — an admin
//! area, an accounting screen — are exactly the ones nobody lists. Single-page
//! apps declare every route in their JavaScript, so REA's bundle analysis can
//! read them out: open the target once in a throwaway browser, analyse the
//! scripts it loaded, and offer each route as a page to add to the crawl.
//!
//! Only routes are used. REA's endpoint candidates from the same analysis are
//! low-confidence guesses (form fields, translation keys) and are ignored; the
//! crawl and HAR imports observe real API calls.

use crate::model::{Engagement, Target};
use crate::rea::Rea;
use crate::readiness;
use crate::store;
use crate::watch;
use chrono::Utc;
use serde_json::Value;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// One route found in the code, as a page the crawl could visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// As the crawl would visit it: `#/administration` or `/administration`.
    pub page: String,
    /// As declared in the code.
    pub route: String,
    /// Already in the target's page list.
    pub already_listed: bool,
    /// Has a parameter (`:id`): it cannot be visited without a real value.
    pub needs_value: bool,
}

#[derive(Debug, Clone)]
pub struct Scan {
    pub suggestions: Vec<Suggestion>,
    pub routes_found: usize,
    /// Where the raw REA output was saved, for provenance.
    pub evidence_file: PathBuf,
    pub evidence_id: Option<String>,
}

/// Route values from REA's `analyze-web-bundle` output.
pub fn routes_from_bundle(output: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(output).map_err(|e| format!("not REA output: {e}"))?;
    let inner = v.get("normalized_result").unwrap_or(&v);
    let routes = inner["observations"]["routes"]
        .as_array()
        .ok_or("REA's output has no routes section")?;
    Ok(routes
        .iter()
        .filter_map(|r| r["value"].as_str().map(str::to_string))
        .collect())
}

/// Turns routes into pages. Hash routing (`#/x`) is what Angular and many
/// single-page apps use; otherwise routes are plain paths.
pub fn suggestions(
    routes: &[String],
    existing_pages: &[String],
    hash_routing: bool,
) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = vec![];
    for route in routes {
        let r = route.trim().trim_start_matches('/');
        // The root, catch-alls, and file-like paths (`/engine.io`) are not pages.
        if r.is_empty() || r.contains('*') || r.contains('.') {
            continue;
        }
        let page = if hash_routing {
            format!("#/{r}")
        } else {
            format!("/{r}")
        };
        if out.iter().any(|s| s.page == page) {
            continue;
        }
        out.push(Suggestion {
            // `#/search?q=apple` in the list already covers `#/search`.
            already_listed: existing_pages
                .iter()
                .any(|p| p.split('?').next() == Some(page.as_str())),
            needs_value: r.split('/').any(|seg| seg.starts_with(':')),
            route: route.clone(),
            page,
        });
    }
    // New, visitable pages first; then those needing a value; listed ones last.
    out.sort_by_key(|s| (s.already_listed, s.needs_value, s.page.clone()));
    out
}

/// Whether a target's pages use hash routing, judged from its current list.
pub fn uses_hash_routing(target: &Target) -> bool {
    target.profile.pages.iter().any(|p| p.starts_with("#/"))
}

/// A throwaway headless browser with CDP on a private port, killed and its
/// profile removed on drop, even if the scan fails half-way.
struct ScanBrowser {
    child: Child,
    profile: PathBuf,
    port: u16,
}

impl ScanBrowser {
    fn launch(url: &str) -> Result<Self, String> {
        let exe = readiness::find_browser().ok_or("no Chrome-family browser found")?;
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .map_err(|e| format!("no free local port: {e}"))?;
        let profile = std::env::temp_dir().join(format!(
            "surfacewatch-scan-{}-{}",
            std::process::id(),
            Utc::now().timestamp_millis()
        ));
        let child = Command::new(exe)
            .args([
                "--headless=new",
                "--no-first-run",
                "--no-default-browser-check",
                "--remote-debugging-address=127.0.0.1",
                &format!("--remote-debugging-port={port}"),
                &format!("--user-data-dir={}", profile.display()),
                url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start the browser: {e}"))?;
        let b = ScanBrowser {
            child,
            profile,
            port,
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if Instant::now() > deadline {
                return Err("the browser did not open its debugging port within 20 s".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(b)
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for ScanBrowser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

/// Opens `target` in a throwaway browser and suggests pages from its code.
/// Blocking, about 10–20 s. Refused outside the engagement's test window:
/// it sends requests to the target like a capture does.
pub fn scan(engagement: &Engagement, target: &Target) -> Result<Scan, String> {
    let now = Utc::now();
    if !engagement.window_allows(now.date_naive()) {
        return Err(format!(
            "{} is outside the test window of {}",
            now.date_naive(),
            engagement.name
        ));
    }
    let rea = Rea::detect();
    let browser = ScanBrowser::launch(&format!("{}/", target.origin))?;
    // Let the app load and register its routes before reading its scripts.
    std::thread::sleep(Duration::from_secs(5));

    let listed = rea.list_browser_targets(&browser.endpoint())?;
    let target_id = page_target_id(&listed, &target.origin)
        .ok_or_else(|| format!("the browser has no page open on {}", target.origin))?;
    let output = rea.analyze_web_bundle(&browser.endpoint(), &target_id)?;
    drop(browser);

    let evidence_file = watch::target_dir(&engagement.id, &target.id)
        .join("scans")
        .join(format!("routes-{}.json", now.format("%Y%m%d-%H%M%S")));
    store::write_private(&evidence_file, &output).map_err(|e| e.to_string())?;

    let routes = routes_from_bundle(&output)?;
    Ok(Scan {
        suggestions: suggestions(&routes, &target.profile.pages, uses_hash_routing(target)),
        routes_found: routes.len(),
        evidence_file,
        evidence_id: crate::rea::evidence_id(&output),
    })
}

/// The CDP target ID of the page open on `origin`, from REA's target list.
fn page_target_id(listed: &str, origin: &str) -> Option<String> {
    let v: Value = serde_json::from_str(listed).ok()?;
    let inner = v.get("normalized_result").unwrap_or(&v);
    inner["targets"].as_array()?.iter().find_map(|t| {
        let same = t["origin"].as_str() == Some(origin) || t["url"].as_str()?.starts_with(origin);
        same.then(|| t["target_id"].as_str().map(str::to_string))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn routes_become_hash_pages_without_root_files_or_duplicates() {
        let s = suggestions(
            &routes(&[
                "",
                "/",
                "/engine.io",
                "administration",
                "accounting",
                "/accounting",
                "**",
            ]),
            &[],
            true,
        );
        let pages: Vec<_> = s.iter().map(|x| x.page.as_str()).collect();
        assert_eq!(pages, vec!["#/accounting", "#/administration"]);
    }

    #[test]
    fn listed_pages_and_parameters_are_marked_and_sorted_last() {
        let s = suggestions(
            &routes(&["score-board", "address/edit/:addressId", "login"]),
            &["#/login".to_string()],
            true,
        );
        let got: Vec<_> = s
            .iter()
            .map(|x| (x.page.as_str(), x.needs_value, x.already_listed))
            .collect();
        assert_eq!(
            got,
            vec![
                ("#/score-board", false, false),
                ("#/address/edit/:addressId", true, false),
                ("#/login", false, true),
            ]
        );
    }

    #[test]
    fn a_listed_page_with_a_query_covers_its_route() {
        let s = suggestions(
            &routes(&["search"]),
            &["#/search?q=apple".to_string()],
            true,
        );
        assert!(s[0].already_listed);
    }

    #[test]
    fn path_routing_gives_plain_paths() {
        let s = suggestions(&routes(&["/admin/users"]), &[], false);
        assert_eq!(s[0].page, "/admin/users");
    }

    #[test]
    fn rea_output_with_or_without_the_envelope_is_read() {
        let inner = r#"{"observations":{"routes":[{"value":"administration","mechanism":"property:path"}]}}"#;
        assert_eq!(routes_from_bundle(inner).unwrap(), vec!["administration"]);
        let wrapped = format!(r#"{{"evidence_id":"ev_1","normalized_result":{inner}}}"#);
        assert_eq!(
            routes_from_bundle(&wrapped).unwrap(),
            vec!["administration"]
        );
        assert!(routes_from_bundle("{}").is_err());
    }

    #[test]
    fn the_page_on_the_target_origin_is_picked_from_the_target_list() {
        let listed = r#"{"normalized_result":{"targets":[
            {"target_id":"A","url":"about:blank","origin":null},
            {"target_id":"B","url":"http://127.0.0.1:3011/#/","origin":"http://127.0.0.1:3011"}]}}"#;
        assert_eq!(
            page_target_id(listed, "http://127.0.0.1:3011").as_deref(),
            Some("B")
        );
        assert_eq!(page_target_id(listed, "https://other.example"), None);
    }

    #[test]
    fn the_juice_shop_bundle_yields_its_hidden_pages() {
        let output = include_str!("../tests/fixtures/juice-shop/bundle-routes-v20.3.0.json");
        let found = routes_from_bundle(output).unwrap();
        let s = suggestions(&found, &[], true);
        for hidden in [
            "#/administration",
            "#/accounting",
            "#/score-board",
            "#/web3-sandbox",
        ] {
            assert!(s.iter().any(|x| x.page == hidden), "{hidden} missing");
        }
        assert!(s.iter().all(|x| !x.page.contains("engine.io")));
    }
}

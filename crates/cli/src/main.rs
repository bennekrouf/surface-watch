//! `surfacewatch-cli` — the headless edition.
//!
//! The desktop app is where engagements are reviewed; this binary is for the
//! places a window is not: a scheduled watch from cron, a CI job, or a quick
//! check over SSH. It uses the same core, so a diff printed here is the diff
//! the app shows.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use surfacewatch_core::licence::StatusExt;
use surfacewatch_core::{
    api0, capture,
    diff::{self, SurfaceDiff},
    discover, findings, licence,
    model::{CaptureProfile, Engagement},
    rea, readiness, setup, store,
    surface::Surface,
    watch,
};

const USAGE: &str = "\
surfacewatch-cli — client-side attack surface monitor

USAGE:
  surfacewatch-cli readiness
  surfacewatch-cli capture <origin> <out-dir> <page>...
  surfacewatch-cli surface <origin> <capture-dir>
  surfacewatch-cli diff <origin> <before-dir> <after-dir> [--repeat <dir>] [--json]

  surfacewatch-cli engagement add <id> <client> <name> --auth TEXT [--from YYYY-MM-DD] [--to YYYY-MM-DD]
  surfacewatch-cli target add <engagement> <target-id> <origin> <page>...
  surfacewatch-cli import <engagement> <target> <capture-dir> [--repeat-of <capture-id>]
  surfacewatch-cli import-har <engagement> <target> <file.har>
  surfacewatch-cli watch <engagement> <target> [--twice]
  surfacewatch-cli findings <engagement> [--retest] [--export]
  surfacewatch-cli suggest-pages <engagement> <target> [--add]
  surfacewatch-cli export-api0 <engagement> <target> [--forward-identity]
  surfacewatch-cli licence [<number> | --remove | --free <email>]

A capture directory holds one REA browser-scenario output per batch (*.json).
--repeat names a second capture of the after build; endpoints that differ
between the two are suppressed as unstable.

`watch` captures a stored target and compares it with its first capture.
--twice captures it a second time to find unstable endpoints.";

fn main() -> ExitCode {
    surfacewatch_core::env::adopt_login_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("readiness") => cmd_readiness(),
        Some("capture") if args.len() >= 4 => {
            cmd_capture(&args[1], Path::new(&args[2]), &args[3..])
        }
        Some("surface") if args.len() == 3 => cmd_surface(&args[1], Path::new(&args[2])),
        Some("diff") if args.len() >= 4 => cmd_diff(&args[1..]),
        Some("engagement") if args.get(1).map(String::as_str) == Some("add") && args.len() >= 5 => {
            cmd_engagement_add(&args[2..])
        }
        Some("target") if args.get(1).map(String::as_str) == Some("add") && args.len() >= 6 => {
            cmd_target_add(&args[2], &args[3], &args[4], &args[5..])
        }
        Some("import") if args.len() >= 4 => cmd_import(&args[1..]),
        Some("export-api0") if args.len() >= 3 => cmd_export_api0(
            &args[1],
            &args[2],
            args.iter().any(|a| a == "--forward-identity"),
        ),
        Some("suggest-pages") if args.len() >= 3 => {
            cmd_suggest_pages(&args[1], &args[2], args.iter().any(|a| a == "--add"))
        }
        Some("import-har") if args.len() == 4 => {
            cmd_import_har(&args[1], &args[2], Path::new(&args[3]))
        }
        Some("findings") if args.len() >= 2 => cmd_findings(
            &args[1],
            args.iter().any(|a| a == "--retest"),
            args.iter().any(|a| a == "--export"),
        ),
        Some("watch") if args.len() >= 3 => {
            cmd_watch(&args[1], &args[2], args.iter().any(|a| a == "--twice"))
        }
        Some("licence" | "license") => cmd_licence(&args[1..]),
        Some("--version" | "-V") => {
            println!("surfacewatch-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `licence` checks the licence with api0 and shows it, `licence <number>`
/// activates a licence number, `licence --remove` takes it off this computer,
/// and `licence --free <email>` has a licence for free AI credits emailed.
fn cmd_licence(args: &[String]) -> Result<(), String> {
    let status = match args.first().map(String::as_str) {
        Some("--free") => {
            let email = args
                .get(1)
                .ok_or("usage: surfacewatch-cli licence --free <email>")?;
            let message = licence::request_free_credits(email).map_err(|e| match e {
                licence::FreeError::Unavailable => {
                    "There are no free AI credits right now.".to_string()
                }
                licence::FreeError::Refused(why) => why,
            })?;
            println!(
                "{message} Then run `surfacewatch-cli licence <number>` with the number from the email."
            );
            return Ok(());
        }
        Some("--remove") => {
            licence::deactivate();
            println!("Removed the licence from this computer, for every Mayorana app on it.");
            licence::current()
        }
        Some(_) => licence::activate(&args.join(""))?,
        None => licence::refresh(true),
    };
    println!("{}", status.describe());
    Ok(())
}

fn cmd_readiness() -> Result<(), String> {
    let report = readiness::check();
    for c in &report.checks {
        let mark = match c.state {
            readiness::CheckState::Ready => "ok  ",
            readiness::CheckState::Warning => "warn",
            readiness::CheckState::Missing => "MISS",
        };
        println!("[{mark}] {:<24} {}", c.name, c.detail);
    }
    println!("\nKnown REA issues:");
    for (issue, workaround) in readiness::KNOWN_ISSUES {
        println!("  - {issue}\n    {workaround}");
    }
    if report.ready() {
        Ok(())
    } else {
        Err("not ready to capture".into())
    }
}

fn cmd_capture(origin: &str, out: &Path, pages: &[String]) -> Result<(), String> {
    let report = readiness::check();
    let browser = report.browser.ok_or("no Chrome-family browser found")?;
    let profile = CaptureProfile {
        pages: pages.to_vec(),
        ..Default::default()
    };
    let rea = rea::Rea::detect();
    for (i, scenario) in rea::scenario_batches(origin, &profile, &browser)
        .iter()
        .enumerate()
    {
        let scenario_path = out.join(format!("scenario-{i}.json.in"));
        store::write_private(&scenario_path, &scenario.to_string()).map_err(|e| e.to_string())?;
        let output = rea
            .capture_scenario(&scenario_path)
            .map_err(|e| format!("batch {i}: {e}"))?;
        let batch_path = out.join(format!("batch-{i}.json"));
        store::write_private(&batch_path, &output).map_err(|e| e.to_string())?;
        let id = rea::evidence_id(&output).unwrap_or_else(|| "?".into());
        println!("batch {i}: {} ({id})", batch_path.display());
    }
    Ok(())
}

fn load(origin: &str, dir: &Path) -> Result<Surface, String> {
    Ok(Surface::build(origin, &capture::read_dir(dir)?))
}

fn cmd_surface(origin: &str, dir: &Path) -> Result<(), String> {
    let s = load(origin, dir)?;
    println!(
        "{} endpoints · {} scripts · {} assets ignored · {} third-party origins",
        s.endpoints.len(),
        s.scripts.len(),
        s.assets_ignored,
        s.third_party_origins.len()
    );
    for (k, ep) in &s.endpoints {
        let statuses: Vec<_> = ep.statuses.iter().map(u16::to_string).collect();
        let params: Vec<_> = ep.params.iter().cloned().collect();
        println!(
            "  {:<48} [{}] {}",
            k.to_string(),
            statuses.join(","),
            params.join(",")
        );
    }
    Ok(())
}

fn cmd_diff(args: &[String]) -> Result<(), String> {
    let origin = &args[0];
    let mut dirs: Vec<PathBuf> = vec![];
    let mut repeat = None;
    let mut as_json = false;
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--repeat" => {
                repeat = Some(PathBuf::from(
                    it.next().ok_or("--repeat needs a directory")?,
                ))
            }
            "--json" => as_json = true,
            _ => dirs.push(PathBuf::from(a)),
        }
    }
    let [before_dir, after_dir] = dirs.as_slice() else {
        return Err("diff needs exactly two capture directories".into());
    };
    let before = load(origin, before_dir)?;
    let mut after = load(origin, after_dir)?;
    if let Some(r) = repeat {
        after.mark_unstable(&load(origin, &r)?);
    }
    let d = diff::diff(&before, &after);

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    print_diff(&d);
    Ok(())
}

fn print_diff(d: &SurfaceDiff) {
    println!(
        "API endpoints: {} -> {} · {} changes",
        d.endpoints_before,
        d.endpoints_after,
        d.significant().count()
    );
    if d.changes.is_empty() {
        println!("No attack-surface change.");
    }
    let mut current = None;
    for c in &d.changes {
        if current != Some(c.kind) {
            println!("\n{}", c.kind.label());
            current = Some(c.kind);
        }
        let mut line = format!("  {}", c.subject);
        if !c.added.is_empty() {
            line += &format!("  +{:?}", c.added);
        }
        if !c.removed.is_empty() {
            line += &format!("  -{:?}", c.removed);
        }
        println!("{line}");
    }
    for k in &d.suppressed {
        println!("\nSuppressed as unstable: {k}");
    }
}

fn find_engagement(id: &str) -> Result<Engagement, String> {
    store::load_engagements()
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("no engagement {id:?}; create it with `engagement add`"))
}

fn cmd_engagement_add(args: &[String]) -> Result<(), String> {
    let (id, client, name) = (&args[0], &args[1], &args[2]);
    let mut draft = setup::EngagementDraft {
        client: client.clone(),
        name: name.clone(),
        ..Default::default()
    };
    let mut it = args[3..].iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--from" => draft.window_start = value.clone(),
            "--to" => draft.window_end = value.clone(),
            "--auth" => draft.authorisation = value.clone(),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let taken: Vec<String> = store::load_engagements()
        .into_iter()
        .map(|e| e.id)
        .collect();
    if let Some(why) = licence::engagement_limit(&licence::refresh(false), taken.len()) {
        return Err(why);
    }
    let mut e = setup::build_engagement(&draft, &taken)?;
    // The ID given on the command line wins, made folder-safe.
    let wanted = setup::slug(id, &[]);
    if taken.contains(&wanted) {
        return Err(format!("engagement {wanted:?} already exists"));
    }
    e.id = wanted;
    store::save_engagement(&e).map_err(|e| e.to_string())?;
    println!("created {}", store::engagement_dir(&e.id).display());
    Ok(())
}

fn cmd_target_add(
    engagement: &str,
    id: &str,
    origin: &str,
    pages: &[String],
) -> Result<(), String> {
    let draft = setup::TargetDraft {
        origin: origin.to_string(),
        pages: pages.join("\n"),
        ..Default::default()
    };
    let e = find_engagement(engagement)?;
    let mut t = setup::build_target(&draft, &e)?;
    // Keep the ID given on the command line, made folder-safe and unique.
    let taken: Vec<String> = e.targets.iter().map(|t| t.id.clone()).collect();
    t.id = setup::slug(id, &taken);
    let mut e = e;
    let pages = t.profile.pages.len();
    let tid = t.id.clone();
    e.targets.push(t);
    store::save_engagement(&e).map_err(|e| e.to_string())?;
    println!("added {tid} ({pages} pages) to {engagement}");
    Ok(())
}

fn cmd_import(args: &[String]) -> Result<(), String> {
    let (engagement, target, dir) = (&args[0], &args[1], Path::new(&args[2]));
    let e = find_engagement(engagement)?;
    if !e.targets.iter().any(|t| &t.id == target) {
        return Err(format!("no target {target:?} in {engagement}"));
    }
    let repeat_of = match args[3..] {
        [ref flag, ref id] if flag == "--repeat-of" => Some(id.clone()),
        [] => None,
        _ => return Err("expected only --repeat-of <capture-id>".into()),
    };
    let c = watch::import_capture(
        &watch::target_dir(engagement, target),
        target,
        dir,
        repeat_of,
        chrono::Utc::now(),
    )?;
    println!("imported capture {} ({} batches)", c.id, c.files.len());
    Ok(())
}

fn cmd_import_har(engagement: &str, target: &str, har: &Path) -> Result<(), String> {
    let e = find_engagement(engagement)?;
    if !e.targets.iter().any(|t| t.id == target) {
        return Err(format!("no target {target:?} in {engagement}"));
    }
    let c = watch::import_har(&watch::target_dir(engagement, target), target, har)?;
    println!(
        "imported {} as capture {} (recorded {}); request body field names were kept, but no cookie, header value, body value or query value",
        har.display(),
        c.id,
        c.taken_at.format("%Y-%m-%d %H:%M UTC")
    );
    Ok(())
}

fn cmd_watch(engagement: &str, target: &str, twice: bool) -> Result<(), String> {
    let e = find_engagement(engagement)?;
    let t = e
        .targets
        .iter()
        .find(|t| t.id == target)
        .ok_or_else(|| format!("no target {target:?} in {engagement}"))?;
    let c = watch::capture_target(&e, t, None)?;
    println!("captured {} ({} batches)", c.id, c.files.len());
    if twice {
        let r = watch::capture_target(&e, t, Some(c.id.clone()))?;
        println!("repeat {}", r.id);
    }
    let dir = watch::target_dir(engagement, target);
    let captures = watch::list_captures(&dir);
    let baseline = watch::baseline_for(&captures, c.source)
        .map(|b| b.id.clone())
        .unwrap_or_default();
    if baseline == c.id {
        println!("This is the first capture: it is the baseline.");
        return Ok(());
    }
    println!("\nCompared with baseline {baseline}:");
    print_diff(&watch::compare(&dir, &t.origin, &baseline, &c.id)?);
    Ok(())
}

fn cmd_findings(engagement: &str, retest: bool, export: bool) -> Result<(), String> {
    let e = find_engagement(engagement)?;
    if retest {
        let n = findings::retest_latest(&e, |_| true)?;
        println!("retested {n} finding(s) against the latest capture of each target");
    }
    for f in findings::load(&e.id) {
        println!(
            "{}  {:<8} {:<13} {}",
            f.id,
            findings::severity_label(f.severity),
            findings::status_label(f.status()),
            f.title
        );
    }
    if export {
        let captures: Vec<_> = e
            .targets
            .iter()
            .flat_map(|t| watch::list_captures(&watch::target_dir(&e.id, &t.id)))
            .collect();
        println!(
            "exported {}",
            findings::export_markdown(&e, &captures)?.display()
        );
    }
    Ok(())
}

fn cmd_suggest_pages(engagement: &str, target: &str, add: bool) -> Result<(), String> {
    let e = find_engagement(engagement)?;
    let t = e
        .targets
        .iter()
        .find(|t| t.id == target)
        .ok_or_else(|| format!("no target {target:?} in {engagement}"))?;
    println!("Opening {} and reading its code…", t.origin);
    let scan = discover::scan(&e, t)?;
    println!(
        "{} routes found in the code (REA evidence {}, saved to {})",
        scan.routes_found,
        scan.evidence_id.as_deref().unwrap_or("?"),
        scan.evidence_file.display()
    );
    let new: Vec<String> = scan
        .suggestions
        .iter()
        .filter(|s| !s.already_listed && !s.needs_value)
        .map(|s| s.page.clone())
        .collect();
    for s in &scan.suggestions {
        let tag = if s.already_listed {
            "listed"
        } else if s.needs_value {
            "needs a value"
        } else {
            "new"
        };
        println!("  {:<14} {}", tag, s.page);
    }
    if add && !new.is_empty() {
        let n = setup::add_pages(&e.id, &t.id, &new)?;
        println!("Added {n} page(s) to the crawl of {}.", t.label);
    } else if !new.is_empty() {
        println!(
            "Run again with --add to add the {} new page(s) to the crawl.",
            new.len()
        );
    }
    Ok(())
}

fn cmd_export_api0(engagement: &str, target: &str, forward_identity: bool) -> Result<(), String> {
    let e = find_engagement(engagement)?;
    let t = e
        .targets
        .iter()
        .find(|t| t.id == target)
        .ok_or_else(|| format!("no target {target:?} in {engagement}"))?;
    let (path, out) = api0::export(&e, t, api0::Options { forward_identity })?;
    println!(
        "{} endpoint(s) exported ({} with request body fields), {} skipped:",
        out.endpoints,
        out.with_body_fields,
        out.skipped.len()
    );
    for s in &out.skipped {
        println!("  skipped {:<40} {}", s.endpoint, s.reason);
    }
    println!("\n{}", path.display());
    println!(
        "\nWarning: `api0 api upload` replaces every API in the workspace it uploads to, and \
         the api0 CLI always uploads to your default workspace. Upload into a workspace kept \
         for imports, or add the group from the api0 console."
    );
    Ok(())
}

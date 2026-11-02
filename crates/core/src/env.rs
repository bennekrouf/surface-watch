//! Making the app's `PATH` look like your terminal's.
//!
//! An app launched from Finder, the Dock or a `.dmg` does not inherit the
//! shell environment. It gets roughly `/usr/bin:/bin:/usr/sbin:/sbin`, which
//! is why `node`, `npx` and `rea` report as "not found" in the packaged build
//! and work perfectly under `cargo run` — the terminal passed its own `PATH`
//! down.
//!
//! Homebrew installs Node into `/opt/homebrew/bin`, and nvm into a directory
//! only the shell profile knows about; a bundle sees neither.
//! So at startup we ask the login shell what it thinks `PATH` should be and
//! adopt it, falling back to the usual locations when that fails.
//!
//! A *non-interactive* login shell is used deliberately: it reads the profile
//! where `brew shellenv` lives, without an interactive shell's prompts. That
//! is not the same as "cannot block", though — `-l` still sources the whole
//! profile, and `nvm`, `conda`, and corporate MDM hooks all routinely do slow
//! or networked work there. Since this runs as the first statement of `main`,
//! a profile that hangs means no window ever appears and the app looks dead.
//! So the probe runs under a deadline and falls back to [`FALLBACKS`].

/// Directories worth having even if the shell tells us nothing.
const FALLBACKS: &[&str] = &[
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Adopts the login shell's `PATH`, merged with whatever we already have.
/// Call once, before anything runs a subprocess.
pub fn adopt_login_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    let login = desktop_kit::shell::login_path();
    let cargo_bin = dirs::home_dir().map(|h| h.join(".cargo/bin").to_string_lossy().to_string());

    let mut extras: Vec<String> = FALLBACKS.iter().map(|s| s.to_string()).collect();
    if let Some(bin) = cargo_bin {
        extras.push(bin);
    }

    let merged = merge_paths(&current, login.as_deref(), &extras);
    std::env::set_var("PATH", merged);
}

/// Login shell first (it is the informed answer), then what we already had,
/// then the fallbacks. Order is preserved and duplicates are dropped, so the
/// first place a tool is found stays the one that wins.
pub fn merge_paths(current: &str, login: Option<&str>, extras: &[String]) -> String {
    desktop_kit::shell::merge_paths(&[login.unwrap_or_default(), current, &extras.join(":")])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extras() -> Vec<String> {
        vec!["/opt/homebrew/bin".to_string(), "/usr/bin".to_string()]
    }

    #[test]
    fn a_bundles_bare_path_gains_the_places_tools_actually_live() {
        // The exact failure: node is in /opt/homebrew/bin, the bundle never sees it.
        let merged = merge_paths("/usr/bin:/bin", None, &extras());
        assert!(merged.split(':').any(|p| p == "/opt/homebrew/bin"));
    }

    #[test]
    fn the_login_shell_is_believed_before_anything_else() {
        let merged = merge_paths("/usr/bin", Some("/opt/homebrew/bin:/usr/bin"), &extras());
        assert!(merged.starts_with("/opt/homebrew/bin"));
    }

    #[test]
    fn a_directory_never_appears_twice() {
        let merged = merge_paths(
            "/usr/bin:/bin",
            Some("/usr/bin:/opt/homebrew/bin"),
            &extras(),
        );
        let count = merged.split(':').filter(|p| *p == "/usr/bin").count();
        assert_eq!(count, 1, "got {merged}");
    }

    #[test]
    fn order_decides_which_copy_of_a_tool_wins() {
        let merged = merge_paths("/usr/bin", Some("/opt/homebrew/bin:/usr/bin"), &extras());
        let entries: Vec<&str> = merged.split(':').collect();
        let brew = entries
            .iter()
            .position(|p| *p == "/opt/homebrew/bin")
            .unwrap();
        let usr = entries.iter().position(|p| *p == "/usr/bin").unwrap();
        assert!(brew < usr);
    }

    #[test]
    fn empty_and_ragged_input_does_not_produce_empty_entries() {
        let merged = merge_paths("::/usr/bin: :", Some(""), &extras());
        assert!(!merged.split(':').any(|p| p.trim().is_empty()));
    }

    #[test]
    fn nothing_at_all_still_yields_a_usable_path() {
        let merged = merge_paths("", None, &extras());
        assert!(merged.split(':').any(|p| p == "/usr/bin"));
    }
}

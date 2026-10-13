use std::path::Path;

/// Opens the file manager at `path`.
pub fn reveal(path: &Path) {
    let _ = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
    } else if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
    };
}

/// Opens an http(s) URL in the user's browser, never in the app's webview.
///
/// Anything else is refused: the URL can come from a remote `latest.json`,
/// and a `file:` or custom-scheme link must not be launched from here.
pub fn open_in_browser(url: &str) -> bool {
    if !is_web_url(url) {
        return false;
    }
    let url = url.to_string();
    std::thread::spawn(move || {
        let _ = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&url).status()
        } else if cfg!(windows) {
            // `start` treats its first quoted argument as a window title.
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &url])
                .status()
        } else {
            std::process::Command::new("xdg-open").arg(&url).status()
        };
    });
    true
}

pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url
            .chars()
            .any(|c| c.is_whitespace() || "\"'`&|^<>".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_web_urls_are_opened() {
        assert!(is_web_url(
            "https://mayorana.ch/downloads/surface-watch/v0.2.0/x.dmg?src=updater"
        ));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("https://x.test/a b"));
        // cmd.exe metacharacters would let a crafted URL run commands on Windows.
        assert!(!is_web_url("https://x.test/&calc"));
        assert!(!is_web_url("https://x.test/|calc"));
    }
}

//! Durable on-disk writes for config and caches.
//!
//! A bare `std::fs::write` is neither atomic nor private, so every persisted
//! file goes through here instead:
//!
//! * **Atomicity.** `fs::write` truncates first and then writes. A crash, a
//!   full disk, or a second window writing the same file leaves a truncated
//!   or interleaved file behind. Every loader here ends in
//!   `unwrap_or_default()`, so a corrupt `profiles.json` does not surface as
//!   an error — the user simply finds all their profiles gone. Writing to a
//!   sibling temp file and `rename`-ing over the target makes the swap a
//!   single atomic step: readers see the old file or the new one, never half
//!   of either.
//!
//! * **Concurrency.** The multi-window design deliberately runs one process
//!   with many VirtualDoms, but windows still share the filesystem, and a
//!   read-modify-write of a whole profile list from two windows loses one
//!   window's edit. A process-wide lock around the read-modify-write closes
//!   the in-process race; `rename` keeps a second *process* from ever seeing
//!   a partial file.
//!
//! * **Permissions.** Engagement folders hold client names, in-scope hosts
//!   and captured traffic metadata. None of that has any business being
//!   world-readable, so files are created 0600 on Unix.

use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Where engagements live: `<local data dir>/Surface Watch`. The folder keeps
/// the app's name from before SurfaceWatch, so engagements made then are found.
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Surface Watch")
}

/// One folder per engagement, holding its records and raw REA Evidence.
pub fn engagement_dir(engagement_id: &str) -> PathBuf {
    data_dir().join("engagements").join(engagement_id)
}

/// Serialises read-modify-write cycles on shared config across windows.
///
/// One lock for all files rather than one per path: these writes are rare and
/// tiny, and a single lock cannot deadlock against itself the way a map of
/// per-path locks acquired in varying order can.
fn store_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    // A poisoned lock here means another thread panicked mid-write. The data
    // it guards is on disk and each write is atomic, so recovering is strictly
    // better than propagating the panic into every later save.
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Run `f` while holding the config lock. Use for any read-modify-write of a
/// shared file, so two windows cannot interleave load → edit → save.
pub fn with_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = store_lock();
    f()
}

/// Atomically replace `path` with `contents`, owner-readable only.
///
/// Errors are returned rather than swallowed so a caller that cares can say
/// so; most callers legitimately do not, and `let _ =` reads honestly at
/// those sites.
pub fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    let _guard = store_lock();
    write_private_locked(path, contents)
}

fn write_private_locked(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // The temp file must sit next to the target: `rename` is only atomic
    // within a filesystem, and a temp dir is frequently a different one.
    // Unique per call, not just per process: two threads writing the same
    // path must not share a temp file. The lock already serialises them, but
    // the invariant should not depend on that.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp{}-{seq}", std::process::id()));

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    file.write_all(contents.as_bytes())?;
    // Without the flush+sync, `rename` can commit a directory entry pointing
    // at data the page cache has not written yet — the file exists and is
    // empty after a power loss.
    file.flush()?;
    let _ = file.sync_all();
    drop(file);

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// `write_private` for a caller already inside [`with_lock`]. Taking the
/// lock again would deadlock — it is not reentrant.
pub fn write_locked(path: &Path, contents: &str) -> std::io::Result<()> {
    write_private_locked(path, contents)
}

/// `write_private` for callers that have nothing useful to do with a failure
/// — keeps the `let _ =` noise out of the UI code.
pub fn write_best_effort(path: &Path, contents: &str) {
    let _ = write_private(path, contents);
}

/// Every engagement under [`data_dir`], sorted by client then name.
///
/// A folder whose `engagement.json` is missing or unreadable is skipped
/// rather than failing the whole list: one damaged engagement must not hide
/// the others.
pub fn load_engagements() -> Vec<crate::model::Engagement> {
    let Ok(entries) = std::fs::read_dir(data_dir().join("engagements")) else {
        return vec![];
    };
    let mut out: Vec<crate::model::Engagement> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_to_string(e.path().join("engagement.json")).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    out.sort_by(|a, b| (&a.client, &a.name).cmp(&(&b.client, &b.name)));
    out
}

/// Saves `engagement` to its folder, creating it if needed.
pub fn save_engagement(engagement: &crate::model::Engagement) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(engagement).map_err(std::io::Error::other)?;
    write_private(
        &engagement_dir(&engagement.id).join("engagement.json"),
        &json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sw-store-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_write_creates_parent_directories() {
        let path = tmp_dir("mkdir").join("nested/deeper/profiles.json");
        write_private(&path, "[]").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");
    }

    #[test]
    fn a_rewrite_replaces_the_previous_contents_entirely() {
        let path = tmp_dir("replace").join("profiles.json");
        write_private(&path, "[{\"a\":1}]").unwrap();
        write_private(&path, "[]").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");
    }

    /// No temp file may survive a successful write — a stray `.tmp1234`
    /// beside `profiles.json` would show up in any directory listing the app
    /// does.
    #[test]
    fn no_temp_file_is_left_behind() {
        let dir = tmp_dir("notmp");
        write_private(&dir.join("profiles.json"), "[]").unwrap();
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().to_string()))
            .collect();
        assert_eq!(names, vec!["profiles.json".to_string()], "got {names:?}");
    }

    /// Saved requests carry auth headers; other users on the machine have no
    /// business reading them.
    #[cfg(unix)]
    #[test]
    fn a_secret_bearing_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let path = tmp_dir("perms").join("saved.json");
        write_private(&path, "{}").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "got {:o}", mode & 0o777);
    }

    /// The property that matters under concurrency: a reader never sees a
    /// partial file. Whatever it reads must parse.
    #[test]
    fn concurrent_writers_never_expose_a_partial_file() {
        let path = tmp_dir("concurrent").join("profiles.json");
        write_private(&path, "[]").unwrap();
        let readers_saw_garbage = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        std::thread::scope(|s| {
            for n in 0..4 {
                let path = path.clone();
                s.spawn(move || {
                    let payload = format!("[{}]", vec!["0"; 2000 * (n + 1)].join(","));
                    for _ in 0..20 {
                        write_private(&path, &payload).unwrap();
                    }
                });
            }
            for _ in 0..4 {
                let path = path.clone();
                let flag = readers_saw_garbage.clone();
                s.spawn(move || {
                    for _ in 0..200 {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            if serde_json::from_str::<serde_json::Value>(&text).is_err() {
                                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                    }
                });
            }
        });

        assert!(
            !readers_saw_garbage.load(std::sync::atomic::Ordering::SeqCst),
            "a reader observed a torn file"
        );
    }
}

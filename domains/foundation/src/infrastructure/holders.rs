//! Who is running a started item: one lock file per process, released by the OS.
//!
//! A process that starts an item takes an exclusive `flock` on its own file in `<db>.holders/`
//! (created on first use, mode 600 in a mode 700 directory) and writes the file's NAME on the
//! item. The lock lives exactly as long as the process: the kernel drops it when the process
//! exits, is killed (`kill -9`) or crashes, and it is not inherited by the agents and probes the
//! process starts (Rust opens files close-on-exec). So "the holder's file can be locked" means
//! "the holder is dead", whatever its pid has since become: the name is `<pid>-<time>`, unique
//! for ever (the file is made with `create_new`), and a new process that reuses the pid makes a
//! different file. Recovery and the stuck-watch check take over only items whose holder is
//! dead; a live holder's items are left alone, and nothing waits on it.
//!
//! What this does not cover: processes on different hosts (a lock on a network file system is
//! not relied on), and a holders directory deleted while a holder lives (its items then read as
//! held by a dead process). The domain knows nothing of files or locks; only the store calls this.

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use endora_kernel::RepositoryError;
use endora_persistence::backend;

type Res<T> = Result<T, RepositoryError>;

/// What a holder name stands for right now.
pub(super) enum Held {
    /// Its lock is held by a live process (or the name cannot be trusted): leave the item alone.
    Alive,
    /// Nobody holds it. The guard (if the file exists) keeps the lock until it is dropped, so
    /// two processes cannot both take the file over; `remove` deletes the dead holder's file.
    Dead(Option<Guard>),
}

/// The lock on a dead holder's file, held while its items are taken over.
pub(super) struct Guard {
    _file: File,
    path: PathBuf,
}

impl Guard {
    /// Deletes the dead holder's file (the lock goes with the descriptor).
    pub(super) fn remove(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// This process's own holder file, once made.
struct Own {
    name: String,
    path: PathBuf,
    _file: File,
}

impl Drop for Own {
    fn drop(&mut self) {
        // Removed while still locked; a process killed outright leaves the (free) file, which
        // the next recovery that meets its name removes.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The holders directory of one database file, and this process's holder in it.
pub(super) struct Holders {
    dir: PathBuf,
    own: Mutex<Option<Own>>,
    /// Whether the one sweep of stale files (see [`Holders::stale`]) was handed out.
    swept: AtomicBool,
}

/// A holder name is `<pid>-<hex time>`: letters, digits and dashes only, so a name read from
/// the file (which anyone who can write the file could change) can never leave the directory.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-')
        && !name.starts_with('-')
}

impl Holders {
    /// Refuses a SQLite URI (`file:...`) spelling of the database path. Core never documented URI
    /// support, and a URI is a second spelling the holders directory cannot be derived from: the
    /// lease would be off for that process in both directions. Call it BEFORE the file is opened.
    pub(super) fn refuse_uri(path: &str) -> Res<()> {
        if path.len() >= 5 && path[..5].eq_ignore_ascii_case("file:") {
            return Err(backend(format!(
                "database path `{path}` is a SQLite URI, which this program does not support: give a plain file path (nothing was opened)"
            )));
        }
        Ok(())
    }

    /// The holders of the database at `path` (already opened, so it exists); `None` for a private
    /// in-memory database (exactly `:memory:` or an empty name: nothing else can open it) and on
    /// platforms without `flock`. The directory sits beside the file under the file's canonical
    /// path, so two plain spellings of one path share it. A path that cannot be made canonical is
    /// an error, never a silent "no holders".
    pub(super) fn for_db(path: &str) -> Res<Option<Self>> {
        if cfg!(not(unix)) || path.is_empty() || path == ":memory:" {
            return Ok(None);
        }
        let mut dir = std::fs::canonicalize(path)
            .map_err(|e| backend(format!("database path `{path}`: {e}")))?
            .into_os_string();
        dir.push(".holders");
        Ok(Some(Self {
            dir: PathBuf::from(dir),
            own: Mutex::new(None),
            swept: AtomicBool::new(false),
        }))
    }

    /// Hands out, once per process, the dead holders' files nobody holds (not this process's):
    /// each comes with its lock, so no other process can take it over meanwhile. The store removes
    /// the ones no item names (a named one is left for the recovery that settles its item).
    pub(super) fn stale(&self) -> Vec<(String, Guard)> {
        if self.swept.swap(true, Ordering::SeqCst) {
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !plain(&name) {
                continue;
            }
            if let Held::Dead(Some(g)) = self.probe(&name) {
                out.push((name, g));
            }
        }
        out
    }

    /// This process's holder name, making and locking its file the first time.
    pub(super) fn own_name(&self) -> Res<String> {
        let mut own = self
            .own
            .lock()
            .map_err(|_| backend("the holder lock is poisoned"))?;
        if let Some(o) = own.as_ref() {
            return Ok(o.name.clone());
        }
        let made = make_own(&self.dir)?;
        let name = made.name.clone();
        *own = Some(made);
        Ok(name)
    }

    /// Whether the process behind `name` is alive. This process's own name counts as dead here:
    /// an item it started and left is its own to recover (one `serve` tick after another).
    pub(super) fn probe(&self, name: &str) -> Held {
        if self
            .own
            .lock()
            .is_ok_and(|o| o.as_ref().is_some_and(|o| o.name == name))
        {
            return Held::Dead(None);
        }
        if !plain(name) {
            return Held::Alive;
        }
        let path = self.dir.join(name);
        match OpenOptions::new().read(true).open(&path) {
            Ok(file) => match lock(&file) {
                Ok(true) => Held::Dead(Some(Guard { _file: file, path })),
                // Locked by another process, or the lock cannot be asked: do not touch the item.
                _ => Held::Alive,
            },
            Err(e) if e.kind() == ErrorKind::NotFound => Held::Dead(None),
            Err(_) => Held::Alive,
        }
    }
}

/// `Ok(true)` when the exclusive lock was taken, `Ok(false)` when another descriptor holds it.
#[cfg(unix)]
fn lock(file: &File) -> std::io::Result<bool> {
    use rustix::fs::{FlockOperation, flock};
    use rustix::io::Errno;
    match flock(file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(true),
        Err(Errno::WOULDBLOCK) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(not(unix))]
fn lock(_file: &File) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(unix)]
fn make_own(dir: &std::path::Path) -> Res<Own> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    let at = |what: &str, e: &dyn std::fmt::Display| {
        backend(format!("holders directory {}: {what}: {e}", dir.display()))
    };
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| at("cannot create it", &e))?;
    // A sweeping process may lock or delete a brand-new file before this one has locked it
    // (the file is free for that instant): then this file is not ours and we try a new name.
    for _ in 0..5 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!("{}-{nanos:x}", std::process::id());
        let path = dir.join(&name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| at("cannot make its holder file", &e))?;
        if !lock(&file).map_err(|e| at("cannot lock its holder file", &e))? {
            continue;
        }
        let same = std::fs::metadata(&path)
            .ok()
            .zip(file.metadata().ok())
            .is_some_and(|(a, b)| a.dev() == b.dev() && a.ino() == b.ino());
        if same {
            return Ok(Own {
                name,
                path,
                _file: file,
            });
        }
    }
    Err(at(
        "cannot keep a holder file",
        &"it was removed five times",
    ))
}

#[cfg(not(unix))]
fn make_own(_dir: &std::path::Path) -> Res<Own> {
    Err(backend("holder locks need a Unix system"))
}

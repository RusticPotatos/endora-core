//! The local notice sink: an append-only text file, one line per notice, safe to ask twice.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::application::NoticeSink;
use crate::domain::Notice;

/// Appends each notice's [`Notice::line`] to a file. A notice whose key is already in the file is
/// not written again, so a retry after a crash between the write and Core's mark leaves one line.
/// The file is only ever appended to, created if it is missing (mode 600).
#[derive(Debug, Clone)]
pub struct FileSink {
    path: PathBuf,
}

impl FileSink {
    /// A sink writing to `path`.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
        }
    }

    fn has(&self, key: &str) -> Result<bool, String> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("cannot read {}: {}", self.path.display(), e.kind())),
        };
        let prefix = format!("key={key} ");
        Ok(text.lines().any(|l| l.starts_with(&prefix)))
    }
}

impl NoticeSink for FileSink {
    fn deliver(&self, notice: &Notice) -> Result<(), String> {
        if self.has(&notice.key)? {
            return Ok(());
        }
        let mut o = std::fs::OpenOptions::new();
        o.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o
            .open(&self.path)
            .map_err(|e| format!("cannot open {}: {}", self.path.display(), e.kind()))?;
        // One write call with its newline, so a line is never split by another writer.
        f.write_all(format!("{}\n", notice.line()).as_bytes())
            .and_then(|()| f.sync_data())
            .map_err(|e| format!("cannot write {}: {}", self.path.display(), e.kind()))
    }
}

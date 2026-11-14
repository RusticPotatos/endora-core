//! The environment every child process of this capability gets: a minimal `PATH` and `HOME` and
//! nothing inherited except names the config allows. The Core process holds the owner token's
//! path and the database's path in its environment; an executor (a script a probe runs, a command
//! the owner wrote) must not see them. This is the same allow-list scrub the git capability
//! applies. It narrows what leaks by accident; the isolation boundary itself is the sandbox the
//! deployment puts around untrusted executors.

use std::path::Path;
use std::process::Command;

/// The only `PATH` a child gets.
pub const PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// Clears `cmd`'s environment, then sets `PATH`, `HOME` (`home`) and the allowed names that are
/// set in this process.
pub fn apply(cmd: &mut Command, home: &Path, allow: &[String]) {
    cmd.env_clear().env("PATH", PATH).env("HOME", home);
    for name in allow {
        if let Some(v) = std::env::var_os(name) {
            cmd.env(name, v);
        }
    }
}

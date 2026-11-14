//! Shared test helpers: an owner proof (through the real credential path) and a plain actor.
#![allow(dead_code)]

use endora_foundation::application::{Core, OwnerProof};
use endora_foundation::domain::Actor;

/// A fixed owner token for tests (the CLI generates a random one).
pub const TOKEN: &str = concat!(
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef"
);

/// Enrols the test token if the file has no owner yet, then authenticates with it.
pub fn owner(core: &Core) -> OwnerProof {
    let _ = core.enroll_owner(TOKEN);
    core.authenticate_owner(TOKEN).expect("test owner token")
}

/// A capability actor with this name.
pub fn actor_named(name: &str) -> Actor {
    Actor::capability(name).expect("a plain test name")
}

/// True when some confinement backend can run on this host. Otherwise prints why and returns
/// false, and the test returns early: a host with no backend skips (with a reason on stderr)
/// instead of failing, and the same test runs for real where a backend works.
pub fn confined_or_skip() -> bool {
    let image = std::env::var("ENDORA_FOUNDATION_EXECUTOR_IMAGE")
        .unwrap_or_else(|_| endora_foundation_git::DEFAULT_IMAGE.to_owned());
    let (c, note) = endora_foundation_git::detect(&image);
    if c == endora_foundation_git::Confinement::Unavailable {
        eprintln!("SKIPPED: no confinement backend can run on this host: {note}");
        return false;
    }
    true
}

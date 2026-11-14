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

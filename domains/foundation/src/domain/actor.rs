//! WHO: the typed actor behind every evidence row, and the reserved names a capability can
//! never take. Constructors are the whole defence here: `core`, `policy` and `owner` can only
//! be built inside this crate, a capability's actor is built by [`Actor::capability`] which
//! rejects the reserved names, and the owner actor exists only behind an
//! [`crate::application::OwnerProof`].

/// What kind of actor this is. Written to every evidence row next to the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// The engine itself (its own verifier checks, recovery).
    Core,
    /// The owner, proven by the owner token.
    Owner,
    /// A standing policy (`policy:auto_accept`, `policy:auto-observe`).
    Policy,
    /// A capability or a proposer: anything outside the engine that asks for calls.
    Capability,
}

impl ActorKind {
    /// Stable lowercase name, as stored.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Owner => "owner",
            Self::Policy => "policy",
            Self::Capability => "capability",
        }
    }

    /// The kind with this name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "core" => Self::Core,
            "owner" => Self::Owner,
            "policy" => Self::Policy,
            "capability" => Self::Capability,
            _ => return None,
        })
    }
}

/// Why a capability actor could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorError {
    /// The name is reserved (`core`, `owner`, `policy`, or a `policy:` / `owner:` prefix).
    Reserved(String),
    /// The name is empty or has characters other than letters, digits, `.`, `_`, `-`.
    Invalid(String),
}

impl std::fmt::Display for ActorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reserved(n) => write!(f, "the actor name {n:?} is reserved"),
            Self::Invalid(n) => write!(f, "the actor name {n:?} is not a plain name"),
        }
    }
}

impl std::error::Error for ActorError {}

/// Whether `name` is one a capability may never take (compared without regard to case).
#[must_use]
pub fn is_reserved(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    matches!(n.as_str(), "core" | "owner" | "policy")
        || n.starts_with("policy:")
        || n.starts_with("owner:")
        || n.starts_with("core:")
}

/// Who is acting: a kind and a name. Compare with `==`; the fields are private so only the
/// constructors below can make one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    kind: ActorKind,
    name: String,
}

impl Actor {
    /// The engine (crate-private: a capability or a CLI caller cannot construct it).
    pub(crate) fn core() -> Self {
        Self {
            kind: ActorKind::Core,
            name: "core".to_owned(),
        }
    }

    /// A standing policy, named `policy:<name>` (crate-private).
    pub(crate) fn policy(name: &str) -> Self {
        Self {
            kind: ActorKind::Policy,
            name: format!("policy:{name}"),
        }
    }

    /// The owner (crate-private: reachable only through an `OwnerProof`).
    pub(crate) fn owner() -> Self {
        Self {
            kind: ActorKind::Owner,
            name: "owner".to_owned(),
        }
    }

    /// An actor outside the engine. Public, and the only public constructor.
    ///
    /// # Errors
    /// [`ActorError::Reserved`] for `core`, `owner`, `policy` and any name starting `policy:`,
    /// `owner:` or `core:`; [`ActorError::Invalid`] for an empty name or odd characters.
    pub fn capability(name: &str) -> Result<Self, ActorError> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
        {
            return Err(ActorError::Invalid(name.to_owned()));
        }
        if is_reserved(name) {
            return Err(ActorError::Reserved(name.to_owned()));
        }
        Ok(Self {
            kind: ActorKind::Capability,
            name: name.to_owned(),
        })
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ActorKind {
        self.kind
    }

    /// The name written in evidence and in `accepted_by`.
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capability_cannot_take_a_reserved_name() {
        for n in [
            "core",
            "owner",
            "policy",
            "Core",
            "OWNER",
            "policy:auto_accept",
            "policy:x",
            "owner:me",
            "core:verifier",
        ] {
            assert_eq!(
                Actor::capability(n),
                Err(ActorError::Reserved(n.to_owned())),
                "{n}"
            );
        }
    }

    #[test]
    fn a_plain_name_builds_a_capability_actor() {
        let a = Actor::capability("agent-1").unwrap();
        assert_eq!(a.kind(), ActorKind::Capability);
        assert_eq!(a.name(), "agent-1");
        assert!(Actor::capability("").is_err());
        assert!(Actor::capability("a b").is_err());
    }
}

//! The owner credential and the proof of it.
//!
//! The owner holds a token (32 random bytes as hex) in a file only they read. The database
//! keeps ONLY a salted SHA-256 of it (`owner_credential`). A KDF such as argon2 would add a
//! heavy dependency and buy nothing here: the token is 256 bits of OS randomness, so there is
//! nothing to guess, and slow hashing only protects low-entropy secrets.
//!
//! [`OwnerProof`] has a private field and is returned only by [`Core::authenticate_owner`], so
//! every owner decision method that takes `&OwnerProof` is unreachable without the token.
//! LIMIT (stated, not defended): a process that can read the token file, or edit the database
//! file while running unconfined as the same user, is the owner as far as this machine goes.

use endora_kernel::AppError;
use sha2::{Digest, Sha256};

use super::core::Core;
use crate::domain::{Actor, Kind};
use crate::infrastructure::{hex, os_random};

/// Shortest token the engine will enrol (the CLI generates 64 hex characters).
pub const MIN_TOKEN_LEN: usize = 32;
/// How long a failed authentication waits before it answers.
pub const FAILURE_DELAY_MS: u64 = 250;

/// Proof that the caller presented the owner token. Cannot be built outside this crate.
#[derive(Debug)]
pub struct OwnerProof {
    _private: (),
}

impl OwnerProof {
    /// The owner actor this proof stands for (what evidence rows record).
    #[must_use]
    pub fn actor(&self) -> Actor {
        Actor::owner()
    }
}

/// Why the owner could not be authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No owner credential exists yet (run `init --token-out`).
    NotEnrolled,
    /// The token does not match.
    Wrong,
    /// The database could not be read or written.
    Backend(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnrolled => write!(
                f,
                "no owner credential exists: run `init --token-out <path>`"
            ),
            Self::Wrong => write!(f, "the owner token is wrong"),
            Self::Backend(m) => write!(f, "owner credential unreadable: {m}"),
        }
    }
}

impl std::error::Error for AuthError {}

fn salted(salt: &[u8], token: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(token.as_bytes());
    h.finalize().into()
}

/// Equal-length comparison that does not stop at the first difference.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

impl Core {
    /// A fresh owner token: 32 bytes from the OS, as 64 hex characters.
    ///
    /// # Errors
    /// [`AppError::BadRequest`] when the OS source cannot be read.
    pub fn generate_owner_token() -> Result<String, AppError> {
        let b = os_random(32).map_err(|e| AppError::BadRequest {
            message: format!("no OS randomness: {e}"),
        })?;
        Ok(hex(&b))
    }

    /// Whether an owner credential exists.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn owner_enrolled(&self) -> Result<bool, AppError> {
        Ok(self.store().owner_credential()?.is_some())
    }

    /// Stores the owner credential (a salted hash of `token`, never the token) if none exists.
    /// Refuses to replace one: there is no engine path to change the owner.
    ///
    /// # Errors
    /// [`AppError::BadRequest`] for a short token or an existing credential.
    pub fn enroll_owner(&self, token: &str) -> Result<(), AppError> {
        if token.len() < MIN_TOKEN_LEN {
            return Err(AppError::BadRequest {
                message: format!("an owner token must be at least {MIN_TOKEN_LEN} characters"),
            });
        }
        if self.owner_enrolled()? {
            return Err(AppError::BadRequest {
                message: "an owner credential already exists".to_owned(),
            });
        }
        let salt = os_random(16).map_err(|e| AppError::BadRequest {
            message: format!("no OS randomness: {e}"),
        })?;
        self.store().insert_owner_credential(
            &hex(&salt),
            &hex(&salted(&salt, token)),
            self.now_ms(),
        )?;
        self.record_control(
            "core.owner-enrolled",
            &Actor::core(),
            true,
            "an owner credential was created",
        )?;
        Ok(())
    }

    /// Checks `token` against the owner credential. The comparison is constant-time; a failure
    /// waits [`FAILURE_DELAY_MS`] and writes an `auth-failed` evidence row that carries no token
    /// content.
    ///
    /// # Errors
    /// [`AuthError`].
    pub fn authenticate_owner(&self, token: &str) -> Result<OwnerProof, AuthError> {
        let cred = self
            .store()
            .owner_credential()
            .map_err(|e| AuthError::Backend(e.to_string()))?;
        let ok = cred
            .as_ref()
            .is_some_and(|(salt, hash)| match (unhex(salt), unhex(hash)) {
                (Some(s), Some(h)) => constant_time_eq(&salted(&s, token), &h),
                _ => false,
            });
        if ok {
            return Ok(OwnerProof { _private: () });
        }
        std::thread::sleep(std::time::Duration::from_millis(FAILURE_DELAY_MS));
        let why = if cred.is_some() {
            "auth-failed: the presented owner token did not match"
        } else {
            "auth-failed: no owner credential exists"
        };
        let _ = self.record_control("core.auth", &Actor::core(), false, why);
        Err(if cred.is_some() {
            AuthError::Wrong
        } else {
            AuthError::NotEnrolled
        })
    }

    fn record_control(
        &self,
        call: &str,
        actor: &Actor,
        allowed: bool,
        what: &str,
    ) -> Result<(), AppError> {
        let kind = if allowed {
            Kind::Control
        } else {
            Kind::Refusal
        };
        self.note_public(call, actor, kind, allowed, what)
    }
}

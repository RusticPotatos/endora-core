//! # Endora foundation (reset, slice 1)
//!
//! Six verbs and nothing else: **hold** a goal and its work, **decide** whether a call may run,
//! **run** it through one port, **record** what happened, **require** an authorised acceptance
//! before work is done, and **recover** from a crash out of durable state.
//!
//! ROCA layout: [`domain`] is pure (imports only `endora-kernel`: no IO, no HTTP, no model or
//! vendor); [`application`] is the one engine ([`application::Core`]) and the one capability
//! port; [`infrastructure`] is the single SQLite file, through `endora-persistence`.
//!
//! No capability lives in this crate. A capability is a separate crate that implements
//! [`application::Capability`]; this crate compiles and is tested with none present.

#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod infrastructure;

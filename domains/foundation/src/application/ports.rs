//! The one capability port. A capability DECLARES the calls it offers; Core calls it through
//! [`Capability::call`] and nothing else. A call returns a result plus evidence.

use crate::domain::{Budget, CallSpec, Fact, Notice};

/// What a finished call returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallOutput {
    /// What the call actually cost, in micro-USD (1 USD = 1,000,000). It must not exceed the call's declared maximum
    /// ([`CallSpec::max_cost_micros`]); Core records it, and a call that exceeds it is a violation.
    pub cost_micros: u64,
    /// What a check reads: facts the capability itself observed (an exit code, a status). Never
    /// built from text an agent or a remote service wrote (see [`FactValue`]).
    pub facts: Vec<Fact>,
    /// The call's result as text for people. No check reads it.
    pub result: String,
    /// What the capability observed while making it (kept in the evidence record).
    pub evidence: String,
    /// The confinement profile the call ran under, when it ran confined (a structured column of
    /// the evidence record, covered by the hash chain). `None` for a call that ran in no profile.
    pub profile: Option<String>,
}

impl CallOutput {
    /// A free result with no facts, no cost and no evidence text.
    #[must_use]
    pub fn text(result: &str) -> Self {
        Self {
            cost_micros: 0,
            facts: Vec::new(),
            result: result.to_owned(),
            evidence: String::new(),
            profile: None,
        }
    }
}

/// A call that ran and failed. It can still report what it cost and what it observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFailure {
    /// Why it failed, in words.
    pub reason: String,
    /// What it actually cost, in micro-USD (1 USD = 1,000,000); `None` when unknown (Core then charges the declared
    /// maximum, as the worst case).
    pub cost_micros: Option<u64>,
    /// Facts it observed before failing; checked against the declaration like a result's.
    pub facts: Vec<Fact>,
    /// The confinement profile it ran under, if any (see [`CallOutput::profile`]).
    pub profile: Option<String>,
}

impl CallFailure {
    /// A failure with a reason only: cost unknown, no facts.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            cost_micros: None,
            facts: Vec::new(),
            profile: None,
        }
    }
}

/// A thing Core can call. Implemented outside this crate; Core never knows which.
pub trait Capability: Send + Sync {
    /// The calls offered, each with its declared band, input, output and data tier.
    fn offers(&self) -> Vec<CallSpec>;

    /// Makes the call. Core has already decided it is allowed; the capability must still do
    /// only what the declaration says.
    ///
    /// # Errors
    /// [`CallFailure`] when the call ran and did not succeed.
    fn call(&self, call: &str, input: &str) -> Result<CallOutput, CallFailure>;

    /// Makes the call under the budget Core granted. Core always calls this one. A capability
    /// whose calls all declare a maximum cost of zero may leave the default (it forwards to
    /// [`Capability::call`]); a capability that declares `BudgetMode::Enforced` must override
    /// it and stop itself at the budget in its executor.
    ///
    /// # Errors
    /// [`CallFailure`] when the call ran and did not succeed.
    fn call_budgeted(
        &self,
        call: &str,
        input: &str,
        budget: &Budget,
    ) -> Result<CallOutput, CallFailure> {
        let _ = budget;
        self.call(call, input)
    }
}

/// Where an owner notice is delivered: the seam for a notice channel (ADR 0152).
///
/// Core hands each notice that no sink has taken to [`NoticeSink::deliver`], oldest first, once
/// per tick, and marks it taken only when the sink says `Ok`; an `Err` leaves it for the next tick.
/// A sink must therefore be idempotent on [`Notice::key`] (asked twice, it says it once).
///
/// Only LOCAL sinks exist: the append-only file (`infrastructure::FileSink`) and the `notices`
/// command's own listing. An outward sink (ntfy or any other network send) is not built: its
/// credential and its first send are the owner's to authorise, and the decide path still refuses
/// outward calls. A future one is a new implementation of this trait in the composition root.
pub trait NoticeSink {
    /// Takes one notice.
    ///
    /// # Errors
    /// A short reason; the notice is then retried on the next tick.
    fn deliver(&self, notice: &Notice) -> Result<(), String>;
}

/// The one port to a model for advisory diagnosis (ADR 0153): a prompt in, a reply out.
///
/// It is NOT a capability: it is never offered, never decided, and its reply is never acted on.
/// Core asks it only to write advice beside a failed watch item, then checks the reply itself
/// (`domain::diagnosis`). Only a LOCAL implementation is built: the composition root wires one
/// when the owner's config line names it, and without that line there is none. A model that is
/// not on this machine is a new outward capability and is the owner's to authorise.
pub trait DiagnosisModel {
    /// Sends the prompt and returns the model's reply text.
    ///
    /// # Errors
    /// A short reason when the model could not be reached or did not answer.
    fn ask(&self, prompt: &str) -> Result<String, String>;
}

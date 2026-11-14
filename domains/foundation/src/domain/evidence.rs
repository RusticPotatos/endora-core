//! RECORD: the fixed shape of one evidence record. Written once, never changed: this module
//! offers a constructor and readers, and the store offers append and read, nothing else.

use endora_kernel::Reversibility;

use super::actor::ActorKind;
use super::hold::ItemId;

/// What a record says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A call was allowed and is about to run (written before it runs, so a crash leaves a trace).
    Intent,
    /// A call that was allowed has finished; `output` is what it returned.
    Result,
    /// A call or an acceptance was refused; `output` says why.
    Refusal,
    /// A control act: a grant, the stop switch, an acceptance, a recovery.
    Control,
}

impl Kind {
    /// Stable lowercase name, as stored.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Result => "result",
            Self::Refusal => "refusal",
            Self::Control => "control",
        }
    }

    /// The kind with this name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "intent" => Self::Intent,
            "result" => Self::Result,
            "refusal" => Self::Refusal,
            "control" => Self::Control,
            _ => return None,
        })
    }
}

/// One evidence record. `id` is the store's, assigned on append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// Row id; zero before it is appended.
    pub id: i64,
    /// The work item, if the record is about one.
    pub item: Option<ItemId>,
    /// The call or control act: a capability's call name, or `core.stop`, `core.accept`, ….
    pub call: String,
    /// Who asked.
    pub actor: String,
    /// What kind of actor that was (`core`, `owner`, `policy`, `capability`).
    pub actor_kind: ActorKind,
    /// The call's band, as declared by the capability.
    pub band: Reversibility,
    /// Whether it was allowed (`true`) or denied.
    pub allowed: bool,
    /// Which phase of the record this is.
    pub kind: Kind,
    /// For a result, the intent it settles.
    pub intent_id: Option<i64>,
    /// Hash of the call's inputs.
    pub inputs_hash: String,
    /// What came back, or why it was refused.
    pub output: String,
    /// When, in Unix milliseconds.
    pub at_ms: i64,
    /// For a result: what the call actually cost, in micro-USD (1 USD = 1,000,000) (its declared maximum when it
    /// failed, since the true cost is then unknown). Zero for every other record.
    pub cost_micros: u64,
    /// The confinement profile the call ran under, as the capability reported it (new rows
    /// only; `None` for older rows and calls that ran in no profile). Covered by the hash chain
    /// for rows that carry the micro-USD unit.
    pub profile: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn a_kind_round_trips_its_name() {
        for k in [Kind::Intent, Kind::Result, Kind::Refusal, Kind::Control] {
            assert_eq!(Kind::from_name(k.name()), Some(k));
        }
        assert_eq!(Kind::from_name("update"), None);
    }
}

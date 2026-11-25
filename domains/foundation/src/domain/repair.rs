//! REPAIR: what a failed watch may do about itself. One goal names at most ONE repair, a call
//! the owner picked: it must exist, sit in the goal's scope, and be declared reversible and
//! local. It is never decided here: when a watch fails, the repair goes through
//! [`super::decide::decide`] like any other call, as the fixed actor [`REPAIR_ACTOR`], on a policy
//! row the owner made that allows it once. Nothing in this module reads a clock, a file or a model.

use endora_kernel::Reversibility;

use super::decide::CallSpec;
use super::hold::GoalId;

/// The actor a repair runs as. It is a plain capability name: it holds no more than the one
/// policy row the owner made for it.
pub const REPAIR_ACTOR: &str = "repair";

/// The most uses of a repair in one failure episode (the policy row's per-item cap, and the
/// episode rule in `Core`).
pub const REPAIR_USES_PER_EPISODE: u32 = 1;

/// A goal's repair, as the owner declared it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    /// The goal whose failed watch it answers.
    pub goal: GoalId,
    /// The call that repairs.
    pub call: String,
    /// The call's input (the owner wrote it; a failing probe never changes it).
    pub input: String,
}

/// Why a repair was refused at registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairRefusal {
    /// No capability offers the call.
    UnknownCall,
    /// The call is not in the reversible band: observe changes nothing, and outward-reversible,
    /// irreversible and unclassified calls are never an automatic repair.
    NotReversible(Reversibility),
    /// The goal's scope does not name the call.
    OutOfScope,
    /// The input does not match the call's declared fields.
    BadInput(String),
    /// The goal already has a repair.
    AlreadySet,
}

impl RepairRefusal {
    /// A stable sentence for the owner and the evidence record.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::UnknownCall => "refused: no capability offers this repair call".to_owned(),
            Self::NotReversible(band) => format!(
                "refused: a repair must be a reversible local call, this one is {}",
                band.name()
            ),
            Self::OutOfScope => "refused: the repair call is outside the goal's scope".to_owned(),
            Self::BadInput(why) => format!("refused: the repair input does not match: {why}"),
            Self::AlreadySet => "refused: the goal already has a repair".to_owned(),
        }
    }
}

/// Whether `call` may be named as a repair for a goal with this `scope`.
///
/// # Errors
/// The [`RepairRefusal`] that applies first.
pub fn validate_repair(
    spec: Option<&CallSpec>,
    scope: &[String],
    call: &str,
    input: &str,
) -> Result<(), RepairRefusal> {
    let Some(spec) = spec else {
        return Err(RepairRefusal::UnknownCall);
    };
    let band = spec.effective_band();
    if band != Reversibility::Reversible {
        return Err(RepairRefusal::NotReversible(band));
    }
    if !scope.iter().any(|c| c == call) {
        return Err(RepairRefusal::OutOfScope);
    }
    spec.check_input(input).map_err(RepairRefusal::BadInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(band: Option<Reversibility>) -> CallSpec {
        let mut s = CallSpec::new("fix.run", Reversibility::Reversible);
        s.band = band;
        s
    }

    #[test]
    fn only_a_reversible_call_in_scope_is_a_repair() {
        let scope = vec!["fix.run".to_owned()];
        let ok = spec(Some(Reversibility::Reversible));
        assert_eq!(validate_repair(Some(&ok), &scope, "fix.run", ""), Ok(()));
        for band in [
            Some(Reversibility::Observe),
            Some(Reversibility::OutwardReversible),
            Some(Reversibility::Irreversible),
            None,
        ] {
            let s = spec(band);
            assert!(matches!(
                validate_repair(Some(&s), &scope, "fix.run", ""),
                Err(RepairRefusal::NotReversible(_))
            ));
        }
        assert_eq!(
            validate_repair(None, &scope, "fix.run", ""),
            Err(RepairRefusal::UnknownCall)
        );
        assert_eq!(
            validate_repair(Some(&ok), &[], "fix.run", ""),
            Err(RepairRefusal::OutOfScope)
        );
    }
}

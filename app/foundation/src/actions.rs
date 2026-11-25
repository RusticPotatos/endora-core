//! The owner's repair actions as one capability, `action.run <name>`. An action is a command line
//! the owner wrote in the config file, declared `reversible`: something that can be undone and
//! that stays on this machine. It runs with a scrubbed environment, like a command probe. The
//! config refuses any other declaration at load, so an action that is irreversible or reaches
//! outward is never registered. What may call it, and how often, is decided by Core: the action
//! has no authority of its own.

use endora_foundation::application::{CallFailure, CallOutput, Capability};
use endora_foundation::domain::{BudgetMode, CallSpec, Field};
use endora_foundation_observe::owner;
use endora_kernel::{Reversibility, Tier};

/// The call name Core routes to this capability.
pub const CALL: &str = "action.run";

/// One configured action.
#[derive(Debug, Clone)]
pub struct Action {
    pub name: String,
    pub command: String,
}

/// Reads the band word of an `action` line. Only `reversible` is accepted.
///
/// # Errors
/// A sentence for the config error when the declaration is anything else.
pub fn declared_band(word: &str) -> Result<Reversibility, String> {
    match word {
        "reversible" => Ok(Reversibility::Reversible),
        "observe" | "outward-reversible" | "irreversible" => Err(format!(
            "an action declared `{word}` is refused: an action must be declared `reversible` (undoable, and local to this machine)"
        )),
        other => Err(format!(
            "unknown action declaration `{other}`: an action must be declared `reversible`"
        )),
    }
}

/// The capability.
pub struct Actions(pub Vec<Action>);

impl Capability for Actions {
    fn offers(&self) -> Vec<CallSpec> {
        if self.0.is_empty() {
            return Vec::new();
        }
        vec![CallSpec {
            name: CALL.into(),
            band: Some(Reversibility::Reversible),
            input: "the name of an action in the config file".into(),
            output: "fact: exit (the command's exit code)".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![Field::text("action")],
            facts: vec![Field::int("exit")],
            budget: BudgetMode::Unenforced,
        }]
    }

    fn call(&self, _call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        let a = self
            .0
            .iter()
            .find(|a| a.name == input)
            .ok_or_else(|| CallFailure::new(format!("`{input}` is not a configured action")))?;
        owner::command_with_env(&a.command, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_reversible_declaration_registers() {
        assert_eq!(declared_band("reversible"), Ok(Reversibility::Reversible));
        for w in ["irreversible", "outward-reversible", "observe", "whatever"] {
            assert!(declared_band(w).is_err(), "{w}");
        }
    }
}

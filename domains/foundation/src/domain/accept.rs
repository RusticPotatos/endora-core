//! REQUIRE: work is done only when its criteria are met and an authorised acceptance is
//! recorded. Nothing closes itself, and nothing that proposes work can accept it (ADR 0149).

use endora_kernel::Reversibility;

use super::actor::{Actor, ActorKind};
use super::hold::ItemState;

/// What acceptance reads about an item.
#[derive(Debug, Clone, Copy)]
pub struct AcceptFacts {
    /// Where the item is.
    pub state: ItemState,
    /// Every criterion is met.
    pub all_met: bool,
    /// Every criterion was met by a deterministic check (none by the owner's confirmation).
    pub all_by_check: bool,
    /// The highest band among the item's allowed calls (observe when it made none).
    pub highest_band: Reversibility,
    /// The item came from a routine.
    pub from_routine: bool,
    /// That routine allows automatic acceptance of observe-band items.
    pub routine_auto_accept: bool,
    /// The item's goal has the `auto-observe` acceptance policy.
    pub goal_auto_observe: bool,
}

/// Why an acceptance was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptRefusal {
    /// The actor may not accept work.
    NotAuthorised,
    /// The item is not running (open, blocked, failed, or already done).
    NotRunning(ItemState),
    /// A criterion is not met.
    CriteriaNotMet,
    /// Policy may accept only an item that came from a routine that allows it.
    PolicyNotEnabled,
    /// Policy may accept only observe-band work; this needs the owner.
    NeedsOwner,
    /// Policy may accept only criteria a deterministic check settled.
    NotVerifiedByCheck,
}

impl AcceptRefusal {
    /// A stable sentence for the evidence record.
    #[must_use]
    pub const fn said(&self) -> &'static str {
        match self {
            Self::NotAuthorised => "refused: this actor may not accept work",
            Self::NotRunning(_) => "refused: only a running item can be accepted",
            Self::CriteriaNotMet => "refused: a criterion is not met",
            Self::PolicyNotEnabled => "refused: the routine does not allow automatic acceptance",
            Self::NeedsOwner => "refused: work above the observe band needs the owner",
            Self::NotVerifiedByCheck => {
                "refused: a criterion was not settled by a deterministic check"
            }
        }
    }
}

/// The policy actor's name for a goal that allows automatic acceptance of observe-band items.
pub const AUTO_OBSERVE: &str = "auto-observe";
/// The policy actor's name for a routine that allows automatic acceptance.
pub const AUTO_ACCEPT: &str = "auto_accept";

/// Whether `actor` may accept an item with these facts. Returns the name to record.
///
/// # Errors
/// The [`AcceptRefusal`] naming what is missing.
pub fn authorise(actor: &Actor, f: &AcceptFacts) -> Result<String, AcceptRefusal> {
    if matches!(actor.kind(), ActorKind::Capability | ActorKind::Core) {
        return Err(AcceptRefusal::NotAuthorised);
    }
    if f.state != ItemState::Running {
        return Err(AcceptRefusal::NotRunning(f.state));
    }
    if !f.all_met {
        return Err(AcceptRefusal::CriteriaNotMet);
    }
    if actor.kind() == ActorKind::Policy {
        let enabled = if *actor == Actor::policy(AUTO_OBSERVE) {
            f.goal_auto_observe
        } else {
            f.from_routine && f.routine_auto_accept
        };
        if !enabled {
            return Err(AcceptRefusal::PolicyNotEnabled);
        }
        if f.highest_band > Reversibility::Observe {
            return Err(AcceptRefusal::NeedsOwner);
        }
        if !f.all_by_check {
            return Err(AcceptRefusal::NotVerifiedByCheck);
        }
    }
    Ok(actor.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> AcceptFacts {
        AcceptFacts {
            state: ItemState::Running,
            all_met: true,
            all_by_check: true,
            highest_band: Reversibility::Observe,
            from_routine: true,
            routine_auto_accept: true,
            goal_auto_observe: true,
        }
    }

    #[test]
    fn auto_observe_follows_the_goal_not_the_routine_and_keeps_the_same_limits() {
        let one_off = AcceptFacts {
            from_routine: false,
            routine_auto_accept: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_OBSERVE), &one_off).as_deref(),
            Ok("policy:auto-observe")
        );
        let owner_goal = AcceptFacts {
            goal_auto_observe: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_OBSERVE), &owner_goal),
            Err(AcceptRefusal::PolicyNotEnabled)
        );
        let reversible = AcceptFacts {
            highest_band: Reversibility::Reversible,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_OBSERVE), &reversible),
            Err(AcceptRefusal::NeedsOwner)
        );
        let by_owner = AcceptFacts {
            all_by_check: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_OBSERVE), &by_owner),
            Err(AcceptRefusal::NotVerifiedByCheck)
        );
    }

    #[test]
    fn the_owner_accepts_an_item_whose_criteria_are_met() {
        assert_eq!(authorise(&Actor::owner(), &facts()).as_deref(), Ok("owner"));
    }

    #[test]
    fn nobody_named_accepts_anything() {
        let a = Actor::capability("scripted-proposer").unwrap();
        assert_eq!(authorise(&a, &facts()), Err(AcceptRefusal::NotAuthorised));
    }

    #[test]
    fn unmet_criteria_block_every_acceptance() {
        let f = AcceptFacts {
            all_met: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::owner(), &f),
            Err(AcceptRefusal::CriteriaNotMet)
        );
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &f),
            Err(AcceptRefusal::CriteriaNotMet)
        );
    }

    #[test]
    fn only_a_running_item_is_accepted_and_a_done_one_not_twice() {
        for s in [
            ItemState::Open,
            ItemState::Blocked,
            ItemState::Failed,
            ItemState::Done,
        ] {
            let f = AcceptFacts {
                state: s,
                ..facts()
            };
            assert_eq!(
                authorise(&Actor::owner(), &f),
                Err(AcceptRefusal::NotRunning(s))
            );
        }
    }

    #[test]
    fn policy_accepts_an_observe_item_checked_deterministically_from_an_enabled_routine() {
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &facts()).as_deref(),
            Ok("policy:auto_accept")
        );
    }

    #[test]
    fn policy_never_accepts_reversible_work_or_unchecked_criteria_or_a_routine_that_forbids_it() {
        let reversible = AcceptFacts {
            highest_band: Reversibility::Reversible,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &reversible),
            Err(AcceptRefusal::NeedsOwner)
        );
        let owner_said = AcceptFacts {
            all_by_check: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &owner_said),
            Err(AcceptRefusal::NotVerifiedByCheck)
        );
        let off = AcceptFacts {
            routine_auto_accept: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &off),
            Err(AcceptRefusal::PolicyNotEnabled)
        );
        let one_off = AcceptFacts {
            from_routine: false,
            ..facts()
        };
        assert_eq!(
            authorise(&Actor::policy(AUTO_ACCEPT), &one_off),
            Err(AcceptRefusal::PolicyNotEnabled)
        );
        // The owner may accept the reversible one.
        assert!(authorise(&Actor::owner(), &reversible).is_ok());
    }
}

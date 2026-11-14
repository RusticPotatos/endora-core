//! DECIDE: authority, default deny. A call runs only if a policy row allows it for the call's
//! declared band and the data's tier, the goal's scope names it, no cap stops it and the owner's
//! stop switch is off. A model's proposal never changes the answer: the band comes from the
//! capability's declaration, never from the proposer.

use endora_kernel::{Decision, Reversibility, Tier};

use super::contract::{BudgetMode, Field};

/// What a capability declares about one call it offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSpec {
    /// Call name, as the capability declares it.
    pub name: String,
    /// Declared band; `None` (unclassified) counts as irreversible.
    pub band: Option<Reversibility>,
    /// What input it takes, in words.
    pub input: String,
    /// What output it returns, in words.
    pub output: String,
    /// Tier of the data it touches.
    pub tier: Tier,
    /// The most one call may cost, in micro-USD (1 USD = 1,000,000): checked against the remaining caps BEFORE the
    /// call. What it actually cost comes back with the result.
    pub max_cost_micros: u64,
    /// The most seconds the call may be granted (Core grants this; the capability stops itself at
    /// it). Default [`crate::domain::DEFAULT_MAX_SECONDS`].
    pub max_seconds: u64,
    /// The most model turns the call may be granted; `None`: the call has no turns.
    pub max_turns: Option<u32>,
    /// The input fields, in order (see [`CallSpec::check_input`]); none: the call takes no input.
    pub inputs: Vec<Field>,
    /// The facts the call may return, by name and type: nothing else becomes a fact.
    pub facts: Vec<Field>,
    /// Whether the executor itself enforces the budget Core passes with each call.
    pub budget: BudgetMode,
}

impl CallSpec {
    /// A free-text, free-cost declaration with the given name and band (tests and simple
    /// capabilities start here and set the fields they need).
    #[must_use]
    pub fn new(name: &str, band: Reversibility) -> Self {
        Self {
            name: name.to_owned(),
            band: Some(band),
            input: String::new(),
            output: String::new(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: crate::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: Vec::new(),
            facts: Vec::new(),
            budget: BudgetMode::Unenforced,
        }
    }

    /// The band the policy sees: the declared one, or irreversible when unclassified.
    #[must_use]
    pub fn effective_band(&self) -> Reversibility {
        self.band.unwrap_or_default()
    }
}

/// One owner-granted row: `actor` may make `call` when its band is at most `band` and the
/// data's tier is at most `tier`, at most `max_uses_per_item` times for one work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRow {
    /// Who.
    pub actor: String,
    /// Which call.
    pub call: String,
    /// Highest band this row covers. Never irreversible in slice 1.
    pub band: Reversibility,
    /// Highest data tier this row covers.
    pub tier: Tier,
    /// Cap on allowed uses per work item.
    pub max_uses_per_item: Option<u32>,
}

/// Everything the decision reads besides the policy.
#[derive(Debug, Clone)]
pub struct Situation<'a> {
    /// The owner's stop switch.
    pub stopped: bool,
    /// Whether the work item is running.
    pub item_running: bool,
    /// Whether the goal was stopped after a cost violation.
    pub goal_stopped: bool,
    /// The goal's allowed calls.
    pub goal_scope: &'a [String],
    /// The goal's data tier.
    pub goal_tier: Tier,
    /// Allowed uses of this call by this actor for this item so far.
    pub uses_this_item: u32,
    /// Micro-USD spent against the goal so far.
    pub goal_spent_micros: u64,
    /// The goal's cap.
    pub goal_cap_micros: u64,
    /// Micro-USD spent overall.
    pub total_spent_micros: u64,
    /// The overall cap.
    pub total_cap_micros: u64,
}

/// Why a call was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deny {
    /// The owner's stop switch is on: every call is blocked.
    Stopped,
    /// No capability offers a call by this name.
    UnknownCall,
    /// The work item is not running.
    ItemNotRunning,
    /// The goal's scope does not name the call.
    OutOfScope,
    /// No policy row for this actor and call.
    NoPolicy,
    /// The call's band is above what the row covers.
    BandAboveRow,
    /// The data tier is above what the row covers.
    TierAboveRow,
    /// Irreversible (declared or unclassified): blocked, never offered for confirmation.
    Irreversible,
    /// Outward-reversible: needs the owner's confirmation, which slice 1 has no path for.
    NeedsOwner,
    /// The row's per-item use cap is reached.
    CapReached,
    /// A spend cap would be passed.
    SpendCap,
    /// The goal was stopped (a call cost more than it declared); only the owner lifts it.
    GoalStopped,
    /// The input does not match the call's declared input fields.
    BadInput,
    /// The call declares a cost its executor cannot enforce.
    BudgetUnenforceable,
}

impl Deny {
    /// A stable sentence for the evidence record.
    #[must_use]
    pub const fn said(&self) -> &'static str {
        match self {
            Self::Stopped => "refused: the owner's stop switch is on",
            Self::UnknownCall => "refused: no capability offers this call",
            Self::ItemNotRunning => "refused: the work item is not running",
            Self::OutOfScope => "refused: the call is outside the goal's scope",
            Self::NoPolicy => "refused: no policy row allows this actor this call",
            Self::BandAboveRow => "refused: the call's band is above the policy row's",
            Self::TierAboveRow => "refused: the data tier is above the policy row's",
            Self::Irreversible => "refused: irreversible calls are blocked",
            Self::NeedsOwner => "refused: outward-reversible calls need the owner's confirmation",
            Self::CapReached => "refused: the policy row's use cap is reached",
            Self::SpendCap => "refused: a spend cap would be passed",
            Self::GoalStopped => "refused: the goal is stopped after a cost violation",
            Self::BadInput => "refused: the input does not match the call's declared fields",
            Self::BudgetUnenforceable => {
                "refused-unenforceable: the executor cannot stop itself at a budget, so it may not be given a non-zero one"
            }
        }
    }
}

/// The answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// May run.
    Allow,
    /// May not, and why.
    Deny(Deny),
}

/// Decides one call. `spec` is `None` when no capability offers the name. Order matters: the
/// stop switch answers first, then what the call is, then what is granted.
#[must_use]
pub fn decide(
    actor: &str,
    call: &str,
    spec: Option<&CallSpec>,
    policy: &[PolicyRow],
    s: &Situation<'_>,
) -> Verdict {
    match refusal(actor, call, spec, policy, s) {
        Some(why) => Verdict::Deny(why),
        None => Verdict::Allow,
    }
}

fn refusal(
    actor: &str,
    call: &str,
    spec: Option<&CallSpec>,
    policy: &[PolicyRow],
    s: &Situation<'_>,
) -> Option<Deny> {
    if s.stopped {
        return Some(Deny::Stopped);
    }
    let Some(spec) = spec else {
        return Some(Deny::UnknownCall);
    };
    if !s.item_running {
        return Some(Deny::ItemNotRunning);
    }
    if s.goal_stopped {
        return Some(Deny::GoalStopped);
    }
    if !s.goal_scope.iter().any(|c| c == call) {
        return Some(Deny::OutOfScope);
    }
    let band = spec.effective_band();
    // The kernel's default posture: irreversible is blocked, never merely confirmed.
    if band.default_decision() == Decision::Block {
        return Some(Deny::Irreversible);
    }
    let Some(row) = policy.iter().find(|r| r.actor == actor && r.call == call) else {
        return Some(Deny::NoPolicy);
    };
    if band > row.band {
        return Some(Deny::BandAboveRow);
    }
    if spec.tier.max(s.goal_tier) > row.tier {
        return Some(Deny::TierAboveRow);
    }
    if band.default_decision() == Decision::Confirm {
        return Some(Deny::NeedsOwner);
    }
    if row.max_uses_per_item.is_some_and(|m| s.uses_this_item >= m) {
        return Some(Deny::CapReached);
    }
    if !spec.budget_is_enforceable() {
        return Some(Deny::BudgetUnenforceable);
    }
    let over = |spent: u64, cap: u64| spent.saturating_add(spec.max_cost_micros) > cap;
    if spec.max_cost_micros > 0
        && (over(s.goal_spent_micros, s.goal_cap_micros)
            || over(s.total_spent_micros, s.total_cap_micros))
    {
        return Some(Deny::SpendCap);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, band: Option<Reversibility>) -> CallSpec {
        CallSpec {
            name: name.into(),
            band,
            input: String::new(),
            output: String::new(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: crate::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: Vec::new(),
            facts: Vec::new(),
            budget: Default::default(),
        }
    }

    fn row(call: &str, band: Reversibility) -> PolicyRow {
        PolicyRow {
            actor: "a".into(),
            call: call.into(),
            band,
            tier: Tier::Internal,
            max_uses_per_item: None,
        }
    }

    fn scope() -> Vec<String> {
        vec!["c".into()]
    }

    fn sit(scope: &[String]) -> Situation<'_> {
        Situation {
            stopped: false,
            item_running: true,
            goal_stopped: false,
            goal_scope: scope,
            goal_tier: Tier::Internal,
            uses_this_item: 0,
            goal_spent_micros: 0,
            goal_cap_micros: 0,
            total_spent_micros: 0,
            total_cap_micros: 0,
        }
    }

    fn why(v: Verdict) -> Option<Deny> {
        match v {
            Verdict::Allow => None,
            Verdict::Deny(d) => Some(d),
        }
    }

    #[test]
    fn with_no_policy_row_everything_is_denied() {
        let sc = scope();
        let v = decide(
            "a",
            "c",
            Some(&spec("c", Some(Reversibility::Observe))),
            &[],
            &sit(&sc),
        );
        assert_eq!(why(v), Some(Deny::NoPolicy));
    }

    #[test]
    fn a_row_covering_band_and_tier_allows() {
        let sc = scope();
        let s = spec("c", Some(Reversibility::Observe));
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Observe)],
            &sit(&sc),
        );
        assert_eq!(v, Verdict::Allow);
    }

    #[test]
    fn the_declared_band_counts_not_the_rows_wish() {
        let sc = scope();
        let s = spec("c", Some(Reversibility::Reversible));
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Observe)],
            &sit(&sc),
        );
        assert_eq!(why(v), Some(Deny::BandAboveRow));
    }

    #[test]
    fn unclassified_counts_as_irreversible_and_is_blocked_whatever_the_row() {
        let sc = scope();
        let s = spec("c", None);
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Irreversible)],
            &sit(&sc),
        );
        assert_eq!(why(v), Some(Deny::Irreversible));
    }

    #[test]
    fn outward_reversible_needs_the_owner() {
        let sc = scope();
        let s = spec("c", Some(Reversibility::OutwardReversible));
        let r = row("c", Reversibility::OutwardReversible);
        assert_eq!(
            why(decide("a", "c", Some(&s), &[r], &sit(&sc))),
            Some(Deny::NeedsOwner)
        );
    }

    #[test]
    fn a_tier_above_the_row_is_denied() {
        let sc = scope();
        let mut s = spec("c", Some(Reversibility::Observe));
        s.tier = Tier::Confidential;
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Observe)],
            &sit(&sc),
        );
        assert_eq!(why(v), Some(Deny::TierAboveRow));
    }

    #[test]
    fn scope_item_state_and_unknown_calls_are_checked() {
        let s = spec("c", Some(Reversibility::Observe));
        let r = [row("c", Reversibility::Observe)];
        let none: Vec<String> = Vec::new();
        assert_eq!(
            why(decide("a", "c", Some(&s), &r, &sit(&none))),
            Some(Deny::OutOfScope)
        );
        let sc = scope();
        let mut idle = sit(&sc);
        idle.item_running = false;
        assert_eq!(
            why(decide("a", "c", Some(&s), &r, &idle)),
            Some(Deny::ItemNotRunning)
        );
        assert_eq!(
            why(decide("a", "c", None, &r, &sit(&sc))),
            Some(Deny::UnknownCall)
        );
    }

    #[test]
    fn the_stop_switch_beats_everything() {
        let sc = scope();
        let s = spec("c", Some(Reversibility::Observe));
        let mut stopped = sit(&sc);
        stopped.stopped = true;
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Observe)],
            &stopped,
        );
        assert_eq!(why(v), Some(Deny::Stopped));
    }

    #[test]
    fn a_per_item_use_cap_stops_the_next_use() {
        let sc = scope();
        let s = spec("c", Some(Reversibility::Reversible));
        let mut r = row("c", Reversibility::Reversible);
        r.max_uses_per_item = Some(1);
        let mut used = sit(&sc);
        assert_eq!(
            decide("a", "c", Some(&s), &[r.clone()], &used),
            Verdict::Allow
        );
        used.uses_this_item = 1;
        assert_eq!(
            why(decide("a", "c", Some(&s), &[r], &used)),
            Some(Deny::CapReached)
        );
    }

    #[test]
    fn a_paid_call_is_denied_against_a_zero_cap() {
        let sc = scope();
        let mut s = spec("c", Some(Reversibility::Observe));
        s.max_cost_micros = 1;
        s.budget = BudgetMode::Enforced { units: 1 };
        let v = decide(
            "a",
            "c",
            Some(&s),
            &[row("c", Reversibility::Observe)],
            &sit(&sc),
        );
        assert_eq!(why(v), Some(Deny::SpendCap));
    }
}

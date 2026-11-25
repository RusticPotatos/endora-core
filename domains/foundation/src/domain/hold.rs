//! HOLD: a goal (what and why, and what done means), a routine (a goal's declared recurrence)
//! and a work item (own criteria, five states). Two levels only: goal and work item.

use endora_kernel::{DomainError, Tier};

/// Most criteria one goal or item may carry (ported from `MOST_CRITERIA`, evidence.rs).
pub const MOST_CRITERIA: usize = 8;

macro_rules! row_id {
    ($name:ident) => {
        /// Row identifier assigned by the store.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub i64);
    };
}
row_id!(GoalId);
row_id!(RoutineId);
row_id!(ItemId);

/// The value of one fact a capability observed.
///
/// There is deliberately no free-text variant: a `Word` is only accepted by Core when the call's
/// spec declares the fact with a closed list of allowed words and the word is on it, so text an
/// agent or a remote service wrote can never become a fact value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactValue {
    /// A number, such as an exit code or an HTTP status.
    Int(i64),
    /// A yes or no.
    Bool(bool),
    /// One of the closed list of words the call declares, such as `pass`.
    Word(String),
}

/// One named fact in a call's structured result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// The field name a check names.
    pub name: String,
    /// What was observed.
    pub value: FactValue,
}

impl Fact {
    /// A numeric fact.
    #[must_use]
    pub fn int(name: &str, value: i64) -> Self {
        Self {
            name: name.to_owned(),
            value: FactValue::Int(value),
        }
    }

    /// A yes/no fact.
    #[must_use]
    pub fn bool(name: &str, value: bool) -> Self {
        Self {
            name: name.to_owned(),
            value: FactValue::Bool(value),
        }
    }

    /// A fact whose value is a word; Core accepts it only if the call's spec lists it.
    #[must_use]
    pub fn word(name: &str, value: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: FactValue::Word(value.to_owned()),
        }
    }

    /// The value as a check compares it: a number in decimal, `true`/`false`, or the word.
    #[must_use]
    pub fn rendered(&self) -> String {
        match &self.value {
            FactValue::Int(n) => n.to_string(),
            FactValue::Bool(b) => b.to_string(),
            FactValue::Word(w) => w.clone(),
        }
    }
}

/// A deterministic check: make `call` with `input`; the criterion is met when the call's
/// structured result has a fact named `field` whose value is exactly `expect`. The call's text
/// (result, evidence) is never read, so nothing an agent writes can satisfy it. No model judges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Capability call to make (read-only by policy, whatever the call declares).
    pub call: String,
    /// The call's input.
    pub input: String,
    /// The fact the check reads.
    pub field: String,
    /// The value it must have, exactly, as [`Fact::rendered`] shows it.
    pub expect: String,
}

impl Check {
    /// Whether the call's facts satisfy this check (the first fact of that name, exact match).
    #[must_use]
    pub fn met_by(&self, facts: &[Fact]) -> bool {
        facts
            .iter()
            .find(|f| f.name == self.field)
            .is_some_and(|f| f.rendered() == self.expect)
    }

    /// The call's input for one item: `{subject}` becomes the item's subject, so one criteria
    /// definition serves many items. With no subject the input is used as written.
    #[must_use]
    pub fn input_for(&self, subject: Option<&str>) -> String {
        subject.map_or_else(
            || self.input.clone(),
            |s| self.input.replace("{subject}", s),
        )
    }
}

/// One line saying what done means, with the check that settles it, if there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    /// What must be true.
    pub text: String,
    /// A deterministic check; without one only the owner can confirm the line.
    pub check: Option<Check>,
}

/// What is wanted, and why, and what done means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Goal {
    /// Row id.
    pub id: GoalId,
    /// What is wanted.
    pub title: String,
    /// Why.
    pub why: String,
    /// What done means.
    pub criteria: Vec<Criterion>,
    /// The only calls this goal may make, by name. Empty: none (default deny).
    pub scope: Vec<String>,
    /// The tier of the data this goal touches.
    pub tier: Tier,
    /// Most this goal may spend, in micro-USD (1 USD = 1,000,000). Zero in slice 1.
    pub spend_cap_micros: u64,
    /// Who accepts this goal's items. Fixed when the owner creates the goal.
    pub acceptance: AcceptancePolicy,
}

/// Who accepts a goal's finished items. Plain data on the goal, set only by the owner at creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AcceptancePolicy {
    /// Every item waits for the owner (the default).
    #[default]
    Owner,
    /// An item whose every call was observe-band and whose criteria were all settled by
    /// structured checks is accepted by `policy:auto-observe`; anything else waits for the owner.
    AutoObserve,
}

impl AcceptancePolicy {
    /// The stored and printed name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::AutoObserve => "auto-observe",
        }
    }

    /// The policy with this name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "owner" => Some(Self::Owner),
            "auto-observe" => Some(Self::AutoObserve),
            _ => None,
        }
    }
}

impl Goal {
    /// A goal's text and criteria must be present and bounded (ported: owner_may_hand_out).
    ///
    /// # Errors
    /// [`DomainError::EmptyField`] or [`DomainError::Malformed`].
    pub fn validate(title: &str, why: &str, criteria: &[Criterion]) -> Result<(), DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "goal.title",
            });
        }
        if why.trim().is_empty() {
            return Err(DomainError::EmptyField { field: "goal.why" });
        }
        validate_criteria(criteria)
    }
}

/// A list of criteria (a goal's, or an item's own) must be present and bounded.
///
/// # Errors
/// [`DomainError::Malformed`].
pub fn validate_criteria(criteria: &[Criterion]) -> Result<(), DomainError> {
    if criteria.is_empty() {
        return Err(DomainError::Malformed {
            field: "goal.criteria",
            reason: "must say what done means: at least one criterion",
        });
    }
    if criteria
        .iter()
        .any(|c| c.check.as_ref().is_some_and(|k| k.field.trim().is_empty()))
    {
        return Err(DomainError::Malformed {
            field: "goal.criteria",
            reason: "a check must name the field it reads",
        });
    }
    if criteria.len() > MOST_CRITERIA || criteria.iter().any(|c| c.text.trim().is_empty()) {
        return Err(DomainError::Malformed {
            field: "goal.criteria",
            reason: "at most 8 criteria, none blank",
        });
    }
    Ok(())
}

/// A goal's declared recurrence: each firing creates one work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    /// Row id.
    pub id: RoutineId,
    /// The goal it serves.
    pub goal: GoalId,
    /// Title each firing's item gets.
    pub title: String,
    /// Rhythm, in milliseconds.
    pub every_ms: i64,
    /// When it next fires.
    pub next_due_ms: i64,
    /// Whether an observe-band item of this routine may be accepted by policy.
    pub auto_accept: bool,
    /// How many failed items in a row pause the routine.
    pub max_failures: u32,
    /// Failed items in a row so far (a healthy one resets it).
    pub consecutive_failures: u32,
    /// Paused after reaching `max_failures`: it fires nothing until the owner resumes it.
    pub paused: bool,
    /// Retired by the owner: it fires nothing, ever, and cannot be resumed.
    pub retired: bool,
}

/// The shortest rhythm a routine may have, in milliseconds.
pub const MIN_EVERY_MS: i64 = 1_000;
/// The longest rhythm a routine may have, in milliseconds (366 days).
pub const MAX_EVERY_MS: i64 = 366 * 24 * 3_600 * 1_000;
/// Failed items in a row after which a routine pauses, unless the owner sets another number.
pub const DEFAULT_MAX_FAILURES: u32 = 5;

/// A routine's rhythm must be within the bounds.
///
/// # Errors
/// [`DomainError::Malformed`].
pub fn validate_rhythm(every_ms: i64) -> Result<(), DomainError> {
    if (MIN_EVERY_MS..=MAX_EVERY_MS).contains(&every_ms) {
        return Ok(());
    }
    Err(DomainError::Malformed {
        field: "routine.every",
        reason: "must be between 1 second and 366 days",
    })
}

/// Where a work item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Created, not started (also: resumed after an interruption).
    Open,
    /// A runner has it.
    Running,
    /// Criteria met and an authorised acceptance recorded. Reached only through acceptance.
    Done,
    /// Cannot go on without something or someone; says why.
    Blocked,
    /// Ended without being done; says why.
    Failed,
}

impl ItemState {
    /// Stable lowercase name, as stored.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Running => "running",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }

    /// The state with this name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "open" => Self::Open,
            "running" => Self::Running,
            "done" => Self::Done,
            "blocked" => Self::Blocked,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// Whether an item may move from `from` to `to`. `done` and `failed` are final; `done` is
/// reachable from `running` only (the application reaches it through acceptance alone).
///
/// # Errors
/// [`DomainError::InvalidTransition`].
pub fn may_move(from: ItemState, to: ItemState) -> Result<(), DomainError> {
    use ItemState::{Blocked, Done, Failed, Open, Running};
    let allowed = matches!(
        (from, to),
        (Open, Running | Blocked | Failed)
            | (Running, Done | Blocked | Failed | Open)
            | (Blocked, Open | Failed)
    );
    if allowed {
        return Ok(());
    }
    Err(DomainError::InvalidTransition {
        from: from.name(),
        to: to.name(),
    })
}

/// A unit of work with its own criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    /// Row id.
    pub id: ItemId,
    /// Its goal.
    pub goal: GoalId,
    /// The routine that fired it, if any.
    pub routine: Option<RoutineId>,
    /// What it is.
    pub title: String,
    /// Where it is.
    pub state: ItemState,
    /// How many times a runner started it.
    pub attempts: u32,
    /// The firing slot it answers, if from a routine.
    pub due_ms: Option<i64>,
    /// What this item is about; `{subject}` in a check's input becomes this.
    pub subject: Option<String>,
    /// Firings missed while down that this one stands in for.
    pub missed: u32,
    /// Why it is blocked or failed, or how it was resumed.
    pub reason: Option<String>,
    /// Who accepted it, once done.
    pub accepted_by: Option<String>,
}

/// What a routine does at `now`. Pure arithmetic over the declared rhythm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// How many slots were missed while nothing ran: recorded as ONE summary, never run (or
    /// written) one by one.
    pub missed: u64,
    /// The first of the missed slots, when any were.
    pub first_missed: Option<i64>,
    /// The latest due slot: the one firing that runs (or is skipped if `previous_open`).
    pub latest: Option<i64>,
    /// Whether the latest slot is skipped because the routine's last item is still open.
    pub skipped_open: bool,
    /// When the routine is next due.
    pub next_due_ms: i64,
}

/// Which slots of a routine are due at `now`, and what happens to each. All slots but the latest
/// are *missed* (counted, recorded as one summary); the latest runs once, unless the routine's
/// previous item is still open, when it is skipped (a routine has at most one open item).
/// Arithmetic is saturating: no input can overflow or panic.
#[must_use]
pub fn plan_firings(next_due_ms: i64, every_ms: i64, now: i64, previous_open: bool) -> Plan {
    if every_ms <= 0 || now < next_due_ms {
        return Plan {
            missed: 0,
            first_missed: None,
            latest: None,
            skipped_open: false,
            next_due_ms,
        };
    }
    let (due, every, now_w) = (
        i128::from(next_due_ms),
        i128::from(every_ms),
        i128::from(now),
    );
    let slots = (now_w - due) / every + 1;
    let slot = |k: i128| i64::try_from(due + k * every).unwrap_or(i64::MAX);
    Plan {
        missed: u64::try_from(slots - 1).unwrap_or(u64::MAX),
        first_missed: (slots > 1).then_some(next_due_ms),
        latest: Some(slot(slots - 1)),
        skipped_open: previous_open,
        next_due_ms: slot(slots),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crit(t: &str) -> Criterion {
        Criterion {
            text: t.into(),
            check: None,
        }
    }

    #[test]
    fn a_goal_needs_what_why_and_one_to_eight_nonblank_criteria() {
        assert!(Goal::validate("keep X healthy", "it matters", &[crit("up")]).is_ok());
        assert!(Goal::validate(" ", "why", &[crit("up")]).is_err());
        assert!(Goal::validate("t", "", &[crit("up")]).is_err());
        assert!(Goal::validate("t", "w", &[]).is_err());
        assert!(Goal::validate("t", "w", &[crit(" ")]).is_err());
        let nine: Vec<_> = (0..9).map(|i| crit(&format!("c{i}"))).collect();
        assert!(Goal::validate("t", "w", &nine).is_err());
    }

    #[test]
    fn done_is_reached_from_running_only_and_final_states_stay_final() {
        use ItemState::*;
        assert!(may_move(Running, Done).is_ok());
        assert!(may_move(Open, Done).is_err());
        assert!(may_move(Blocked, Done).is_err());
        assert!(may_move(Done, Open).is_err());
        assert!(may_move(Failed, Open).is_err());
        assert!(may_move(Running, Open).is_ok());
    }

    #[test]
    fn nothing_is_due_before_its_slot() {
        let p = plan_firings(1_000, 100, 999, false);
        assert_eq!((p.latest, p.next_due_ms), (None, 1_000));
    }

    #[test]
    fn one_due_slot_fires_once() {
        let p = plan_firings(1_000, 100, 1_000, false);
        assert_eq!(p.latest, Some(1_000));
        assert!(p.missed == 0 && !p.skipped_open);
        assert_eq!(p.next_due_ms, 1_100);
    }

    #[test]
    fn firings_missed_while_down_are_recorded_and_the_latest_runs_once() {
        let p = plan_firings(1_000, 100, 1_350, false);
        assert_eq!((p.missed, p.first_missed), (3, Some(1_000)));
        assert_eq!(p.latest, Some(1_300));
        assert_eq!(p.next_due_ms, 1_400);
    }

    #[test]
    fn no_input_overflows_the_slot_arithmetic() {
        let p = plan_firings(i64::MIN, 1, i64::MAX, false);
        assert!(p.latest.is_some());
        let p = plan_firings(i64::MAX - 5, i64::MAX, i64::MAX, false);
        assert_eq!(p.next_due_ms, i64::MAX);
        assert!(validate_rhythm(i64::MAX).is_err());
        assert!(validate_rhythm(999).is_err() && validate_rhythm(1_000).is_ok());
        assert!(
            validate_rhythm(MAX_EVERY_MS).is_ok() && validate_rhythm(MAX_EVERY_MS + 1).is_err()
        );
    }

    #[test]
    fn a_routine_with_an_open_item_skips_its_firing() {
        let p = plan_firings(1_000, 100, 1_000, true);
        assert!(p.skipped_open);
        assert_eq!(p.latest, Some(1_000));
    }
}

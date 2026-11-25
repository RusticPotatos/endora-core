//! The pure rules: nothing here reads a clock, a file, a socket or a model.

pub mod accept;
pub mod actor;
pub mod contract;
pub mod decide;
pub mod evidence;
pub mod hold;
pub mod notice;
pub mod repair;

pub use accept::{AUTO_ACCEPT, AUTO_OBSERVE, AcceptFacts, AcceptRefusal, authorise};
pub use actor::{Actor, ActorError, ActorKind, is_reserved};
pub use contract::{Budget, BudgetMode, DEFAULT_MAX_SECONDS, Field, ValueType};
pub use decide::{CallSpec, Deny, PolicyRow, Situation, Verdict, decide};
pub use evidence::{Evidence, Kind};
pub use hold::{
    AcceptancePolicy, Check, Criterion, DEFAULT_MAX_FAILURES, Fact, FactValue, Goal, GoalId,
    ItemId, ItemState, MAX_EVERY_MS, MIN_EVERY_MS, Plan, Routine, RoutineId, WorkItem, may_move,
    plan_firings, validate_criteria, validate_rhythm,
};
pub use notice::{FAILING_AFTER, GRACE_MS, Notice, NoticeState, failing_due, notice_key};
pub use repair::{REPAIR_ACTOR, REPAIR_USES_PER_EPISODE, Repair, RepairRefusal, validate_repair};

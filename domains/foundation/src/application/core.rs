//! The engine. Every consequential step goes the same way: read the facts, ask the pure rules,
//! write the evidence, and only then (for an allowed call) touch the world.

use std::sync::Arc;

use endora_kernel::{AppError, Clock, DomainError, RepositoryError, Reversibility, Tier};
use sha2::{Digest, Sha256};

use super::owner::OwnerProof;
use super::ports::{CallOutput, Capability};
use crate::domain::{
    AUTO_ACCEPT, AUTO_OBSERVE, AcceptFacts, AcceptRefusal, AcceptancePolicy, Actor, ActorError,
    Budget, BudgetMode, CallSpec, Criterion, DEFAULT_MAX_FAILURES, Deny, Evidence, Goal, GoalId,
    ItemId, ItemState, Kind, PolicyRow, REPAIR_ACTOR, REPAIR_USES_PER_EPISODE, Repair, Routine,
    RoutineId, Situation, Verdict, WorkItem, authorise, decide, plan_firings, validate_criteria,
    validate_repair, validate_rhythm,
};
use crate::infrastructure::{ChainReport, ManualClock, Store};

/// The actor name the engine's own checks run as; the owner grants it what a goal's checks need.
pub const VERIFIER: &str = "core";

/// How many times a runner may start one item before an interruption fails it for good.
pub const MAX_ATTEMPTS: u32 = 3;

/// The [`AppError::Conflict`] code of [`Core::start`] when another process took the item first.
pub const ITEM_TAKEN: &str = "item-taken";

/// The [`AppError::Conflict`] code of [`Core::confirm_criterion`] on an item that is already
/// `done` or `failed`.
pub const ITEM_FINISHED: &str = "item-finished";

type Res<T> = Result<T, AppError>;

/// A goal to create.
#[derive(Debug, Clone)]
pub struct NewGoal {
    /// What is wanted.
    pub title: String,
    /// Why.
    pub why: String,
    /// What done means.
    pub criteria: Vec<Criterion>,
    /// The calls the goal may make.
    pub scope: Vec<String>,
    /// The tier of the data it touches.
    pub tier: Tier,
    /// Its spend cap, in micro-USD (1 USD = 1,000,000).
    pub spend_cap_micros: u64,
}

/// A work item to create. The goal's criteria apply unless `criteria` overrides them.
#[derive(Debug, Clone)]
pub struct NewItem {
    /// The goal it serves.
    pub goal: GoalId,
    /// What it is.
    pub title: String,
    /// What it is about: `{subject}` in a check's input becomes this.
    pub subject: Option<String>,
    /// The item's own criteria, instead of the goal's.
    pub criteria: Option<Vec<Criterion>>,
}

/// A call that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// What the capability returned.
    pub output: CallOutput,
    /// The result record that kept it.
    pub evidence_id: i64,
}

/// What became of one proposed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Allowed and run.
    Done(Ran),
    /// Refused by policy; nothing ran. The refusal is recorded.
    Refused(Deny),
    /// Allowed and run, and the call itself failed.
    Failed(String),
    /// Allowed and run, but its result broke the call's declared shape (an undeclared or invalid
    /// fact): recorded as a violation, the item failed and the goal stopped.
    Contract(String),
    /// Allowed and run, but it cost more than it declared: recorded as a violation, the item
    /// failed and the goal stopped until the owner resumes it.
    Violation {
        /// The declared maximum, in micro-USD (1 USD = 1,000,000).
        max: u64,
        /// What it actually cost, in micro-USD (1 USD = 1,000,000).
        actual: u64,
    },
}

/// What became of an acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acceptance {
    /// The item is done.
    Accepted {
        /// Who accepted it.
        by: String,
    },
    /// Refused; recorded.
    Refused(AcceptRefusal),
}

/// What one routine's firing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    /// The routine.
    pub routine: RoutineId,
    /// The item created, if any.
    pub item: Option<ItemId>,
    /// Slots missed while down, now on record.
    pub missed: u32,
    /// The slot was skipped because the routine's last item is still open.
    pub skipped_open: bool,
}

/// What a re-check of a routine's item concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEnd {
    /// Every criterion met: the run of failures is reset; the item awaits acceptance.
    Healthy,
    /// A check failed: the item was ended as failed.
    Failed {
        /// Failed items in a row, this one included.
        failures: u32,
        /// Whether this one reached the cap and paused the routine.
        paused: bool,
    },
    /// Not a watch, already ended, or waiting on the owner to confirm a criterion.
    Waiting,
    /// A check failed, the goal's one repair ran and the probe's re-check now passes: the item
    /// stays running and waits for the owner to accept it (policy never accepts reversible work),
    /// so the routine parks behind it.
    Repaired,
}

/// What recovery did with one interrupted item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    /// The item.
    pub item: ItemId,
    /// Where it is now (`open`, to be resumed, or `failed`).
    pub now: ItemState,
    /// Why, in words.
    pub why: String,
}

/// The engine: holds, decides, runs, records, requires and recovers.
pub struct Core {
    store: Store,
    clock: Arc<dyn Clock + Send + Sync>,
    caps: Vec<Registered>,
}

/// A capability and the calls it declared AT REGISTRATION. Core routes, bands and prices a call
/// by this snapshot only; the capability's live `offers()` is never trusted again.
struct Registered {
    cap: Arc<dyn Capability>,
    specs: Vec<CallSpec>,
}

/// The control key holding a goal's cost-violation stop.
fn goal_stop_key(goal: GoalId) -> String {
    format!("goal_stop_{}", goal.0)
}

/// SHA-256 of a call's inputs, in hex.
#[must_use]
pub fn inputs_hash(call: &str, input: &str) -> String {
    let mut h = Sha256::new();
    h.update(call.as_bytes());
    h.update([0]);
    h.update(input.as_bytes());
    crate::infrastructure::hex(&h.finalize())
}

impl Core {
    /// Opens the single SQLite file at `path` (`:memory:` for a private one).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn open(path: &str, clock: Arc<dyn Clock + Send + Sync>) -> Res<Self> {
        Ok(Self {
            store: Store::open(path)?,
            clock,
            caps: Vec::new(),
        })
    }

    /// A private in-memory engine.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn in_memory(clock: Arc<ManualClock>) -> Res<Self> {
        Self::open(":memory:", clock)
    }

    /// Adds a capability. Its declared calls become callable (still subject to policy). The
    /// declaration is checked first and nothing is registered if it fails: every call needs a
    /// non-empty name, a declared band, and a name no other call (of this or an earlier
    /// capability) already uses, so a later declaration can never shadow an earlier one.
    ///
    /// # Errors
    /// [`AppError::BadRequest`] naming the offending call.
    pub fn register(&mut self, cap: Arc<dyn Capability>) -> Res<()> {
        let bad = |message: String| Err(AppError::BadRequest { message });
        let mut seen: Vec<String> = Vec::new();
        let declared = cap.offers();
        for s in declared.clone() {
            if s.name.trim().is_empty() {
                return bad("a capability declares a call with an empty name".to_owned());
            }
            if s.band.is_none() {
                return bad(format!("call {} declares no band", s.name));
            }
            if seen.contains(&s.name) {
                return bad(format!(
                    "call {} is declared twice by one capability",
                    s.name
                ));
            }
            if let Err(why) = s.validate_shape() {
                return bad(why);
            }
            if self.spec(&s.name).is_some() {
                return bad(format!(
                    "call {} is already declared by a registered capability",
                    s.name
                ));
            }
            seen.push(s.name);
        }
        self.caps.push(Registered {
            cap,
            specs: declared,
        });
        Ok(())
    }

    /// The store, for reads.
    #[must_use]
    pub const fn store(&self) -> &Store {
        &self.store
    }

    fn now(&self) -> i64 {
        self.clock.now().unix_millis()
    }

    /// Every call the registered capabilities declare.
    #[must_use]
    pub fn specs(&self) -> Vec<CallSpec> {
        self.caps.iter().flat_map(|c| c.specs.clone()).collect()
    }

    /// What the capabilities declare, by call name.
    #[must_use]
    pub fn spec(&self, call: &str) -> Option<CallSpec> {
        self.caps
            .iter()
            .flat_map(|c| c.specs.iter())
            .find(|s| s.name == call)
            .cloned()
    }

    fn note(
        &self,
        item: Option<ItemId>,
        call: &str,
        actor: &Actor,
        kind: Kind,
        allowed: bool,
        output: &str,
    ) -> Res<i64> {
        let e = Evidence {
            id: 0,
            item,
            call: call.to_owned(),
            actor: actor.name(),
            actor_kind: actor.kind(),
            band: Reversibility::Observe,
            allowed,
            kind,
            intent_id: None,
            inputs_hash: inputs_hash(call, output),
            output: output.to_owned(),
            at_ms: self.now(),
            cost_micros: 0,
            profile: None,
        };
        Ok(self.store.append_evidence(&e)?)
    }

    pub(super) fn note_public(
        &self,
        call: &str,
        actor: &Actor,
        kind: Kind,
        allowed: bool,
        output: &str,
    ) -> Res<()> {
        self.note(None, call, actor, kind, allowed, output)
            .map(|_| ())
    }

    pub(super) fn now_ms(&self) -> i64 {
        self.now()
    }

    // ---- hold -------------------------------------------------------------------------------

    /// Creates a goal (owner only).
    ///
    /// # Errors
    /// [`AppError::Domain`] for a goal that does not say what done means; [`AppError::BadRequest`].
    pub fn add_goal(&self, proof: &OwnerProof, g: NewGoal) -> Res<GoalId> {
        self.add_goal_with_policy(proof, g, AcceptancePolicy::Owner)
    }

    /// Creates a goal with an acceptance policy (owner only). The policy is data on the goal,
    /// fixed here: no call, capability result or later command changes it.
    ///
    /// # Errors
    /// As [`Core::add_goal`].
    pub fn add_goal_with_policy(
        &self,
        proof: &OwnerProof,
        g: NewGoal,
        acceptance: AcceptancePolicy,
    ) -> Res<GoalId> {
        Goal::validate(&g.title, &g.why, &g.criteria)?;
        self.check_criteria_against_declarations(&g.criteria)?;
        let goal = Goal {
            id: GoalId(0),
            title: g.title,
            why: g.why,
            criteria: g.criteria,
            scope: g.scope,
            tier: g.tier,
            spend_cap_micros: g.spend_cap_micros,
            acceptance,
        };
        let id = self.store.insert_goal(&goal, self.now())?;
        self.note(
            None,
            "core.goal",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("goal {} created (acceptance: {})", id.0, acceptance.name()),
        )?;
        Ok(id)
    }

    /// Declares a routine for a goal (owner only): one item per firing, first due at `first_due_ms`,
    /// paused after [`DEFAULT_MAX_FAILURES`] failed items in a row.
    ///
    /// # Errors
    /// [`AppError::Domain`] for a rhythm outside 1 second to 366 days; [`AppError::BadRequest`].
    pub fn add_routine(
        &self,
        proof: &OwnerProof,
        goal: GoalId,
        title: &str,
        every_ms: i64,
        auto_accept: bool,
        first_due_ms: i64,
    ) -> Res<RoutineId> {
        self.add_routine_limited(
            proof,
            goal,
            title,
            every_ms,
            auto_accept,
            first_due_ms,
            DEFAULT_MAX_FAILURES,
        )
    }

    /// Like [`Core::add_routine`], with the number of failed items in a row that pauses it.
    ///
    /// # Errors
    /// As [`Core::add_routine`]; also a cap of zero.
    #[allow(clippy::too_many_arguments)]
    pub fn add_routine_limited(
        &self,
        proof: &OwnerProof,
        goal: GoalId,
        title: &str,
        every_ms: i64,
        auto_accept: bool,
        first_due_ms: i64,
        max_failures: u32,
    ) -> Res<RoutineId> {
        validate_rhythm(every_ms)?;
        if max_failures == 0 {
            return Err(DomainError::Malformed {
                field: "routine.max_failures",
                reason: "must be at least 1",
            }
            .into());
        }
        let id = self.store.insert_routine(
            goal,
            title,
            every_ms,
            first_due_ms,
            auto_accept,
            max_failures,
        )?;
        self.note(
            None,
            "core.routine",
            &proof.actor(),
            Kind::Control,
            true,
            &format!(
                "routine {} declared (pauses after {max_failures} failures)",
                id.0
            ),
        )?;
        Ok(id)
    }

    /// Re-checks a routine's item and ends it: runs its checks (as [`Core::verify`]); an item whose
    /// check still fails is ENDED as `failed` with its evidence, counted against its routine, and
    /// the routine is PAUSED at its cap of failures in a row. An item whose criteria are all met
    /// resets the count. Only routine items are watches; any other item is left as it is.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn check_watch(&self, item: ItemId) -> Res<(Vec<bool>, WatchEnd)> {
        let met = self.verify(item)?;
        let it = self.store.item(item)?;
        let Some(rid) = it.routine else {
            return Ok((met, WatchEnd::Waiting));
        };
        if it.state != ItemState::Running {
            return Ok((met, WatchEnd::Waiting)); // already ended (a violation, say)
        }
        let unmet_check = self
            .store
            .criteria(item)?
            .iter()
            .position(|k| k.met_by.is_none() && k.criterion.check.is_some());
        let Some(idx) = unmet_check else {
            if met.iter().all(|m| *m) {
                self.store.reset_routine(rid, false, 0)?;
                return Ok((met, WatchEnd::Healthy));
            }
            return Ok((met, WatchEnd::Waiting));
        };
        let routine = self.store.routine(rid)?;
        if let Some(repaired) = self.try_repair(item, &routine, idx)? {
            return Ok(repaired);
        }
        let failures = self.store.add_routine_failure(rid)?;
        let why = format!(
            "watch failed: criterion {} not met (failure {failures} of {} in a row)",
            idx + 1,
            routine.max_failures
        );
        // A repair that broke its contract or its cost already failed the item.
        if self.store.item(item)?.state == ItemState::Running {
            self.store.move_item(item, ItemState::Failed, Some(&why))?;
        }
        self.note(
            Some(item),
            "core.watch",
            &Actor::core(),
            Kind::Control,
            false,
            &why,
        )?;
        let paused = failures >= routine.max_failures;
        if paused {
            self.store.pause_routine(rid)?;
            self.note(
                Some(item),
                "core.watch",
                &Actor::core(),
                Kind::Control,
                false,
                &format!(
                    "routine {} paused after {failures} failures in a row: the owner resumes it",
                    rid.0
                ),
            )?;
        }
        Ok((met, WatchEnd::Failed { failures, paused }))
    }

    /// The goal's one repair, tried for a watch whose check just failed (criterion `idx`, 0-based).
    /// The repair is an ordinary call by the actor `repair`, decided by [`decide`] alone: the stop
    /// switch, the goal's scope, the owner's policy row and the row's cap all apply, and the
    /// intent and the result (or the refusal) are evidence rows. The cap is ONE use per failure
    /// episode: uses on this item plus those of the failed items of the routine's current run of
    /// failures all count, so a repair that did not help is not tried again until a healthy check
    /// ends the episode. When the call ran, the failed check runs again; if every criterion is now
    /// met the item stays running for the owner and `Some(Repaired)` comes back. `None` means no
    /// repair happened (none set, or the routine is paused or retired) or it did not help: the watch fails as it always did.
    fn try_repair(
        &self,
        item: ItemId,
        routine: &Routine,
        idx: usize,
    ) -> Res<Option<(Vec<bool>, WatchEnd)>> {
        // The owner's pause or retire stops everything the routine does, a repair included: an item
        // opened before the word was given fails as it always did, and nothing is changed.
        if routine.paused || routine.retired {
            return Ok(None);
        }
        let Some(repair) = self.store.repair(routine.goal)? else {
            return Ok(None);
        };
        let actor = Actor::capability(REPAIR_ACTOR).map_err(|e| AppError::BadRequest {
            message: e.to_string(),
        })?;
        let earlier = self.store.episode_uses(
            routine.id,
            REPAIR_ACTOR,
            &repair.call,
            item,
            routine.consecutive_failures,
        )?;
        let ran = matches!(
            self.call_counting(&actor, item, &repair.call, &repair.input, earlier)?,
            Outcome::Done(_) | Outcome::Failed(_)
        );
        if !ran {
            return Ok(None); // refused (recorded by the call) or the item was already failed
        }
        let met = self.verify(item)?;
        let healed = met.iter().all(|m| *m);
        self.note(
            Some(item),
            "core.repair",
            &Actor::core(),
            Kind::Control,
            healed,
            &format!(
                "repair {} ran after criterion {} failed; the check was run again: {}",
                repair.call,
                idx + 1,
                if healed {
                    "met, the item waits for the owner"
                } else {
                    "still not met"
                }
            ),
        )?;
        if !healed {
            return Ok(None);
        }
        self.store.reset_routine(routine.id, false, 0)?;
        Ok(Some((met, WatchEnd::Repaired)))
    }

    /// Routine items a dead process left `running` whose checks have not been met: each is to be
    /// re-checked and closed by [`Core::check_watch`], so a routine never waits on one forever.
    /// An item a LIVE process is running (its holder, see `infrastructure::holders`) is left
    /// alone; each item returned has been taken over for this process, so no other recovering
    /// process returns it too.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn stuck_watch_items(&self) -> Res<Vec<ItemId>> {
        let mut out = Vec::new();
        for it in self.store.items_in(ItemState::Running)? {
            let waiting = it.routine.is_some()
                && self
                    .store
                    .criteria(it.id)?
                    .iter()
                    .any(|k| k.met_by.is_none() && k.criterion.check.is_some());
            if waiting && self.store.adopt(it.id)? {
                out.push(it.id);
            }
        }
        Ok(out)
    }

    /// Pauses a routine on the owner's word: it fires nothing until the owner resumes it. Pausing
    /// a paused routine is allowed (it stays paused, and the owner's word is recorded again).
    ///
    /// # Errors
    /// [`AppError::BadRequest`] for a retired routine; [`AppError::Repository`] for an unknown one.
    pub fn pause_routine(&self, proof: &OwnerProof, routine: RoutineId) -> Res<()> {
        self.refuse_if_retired(routine, "paused")?;
        self.store.pause_routine(routine)?;
        self.note(
            None,
            "core.pause_routine",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("routine {} paused", routine.0),
        )?;
        Ok(())
    }

    /// Retires a routine for good (owner only): it never fires again and cannot be resumed. Its
    /// past items and evidence stay. Retiring a retired routine is refused.
    ///
    /// # Errors
    /// [`AppError::BadRequest`] for a routine already retired; [`AppError::Repository`] for an
    /// unknown one.
    pub fn retire_routine(&self, proof: &OwnerProof, routine: RoutineId) -> Res<()> {
        self.refuse_if_retired(routine, "retired again")?;
        self.store.retire_routine(routine)?;
        self.note(
            None,
            "core.retire_routine",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("routine {} retired", routine.0),
        )?;
        Ok(())
    }

    fn refuse_if_retired(&self, routine: RoutineId, what: &str) -> Res<()> {
        if !self.store.routine(routine)?.retired {
            return Ok(());
        }
        Err(AppError::BadRequest {
            message: format!(
                "routine {} is retired for good and cannot be {what}; declare a new routine instead",
                routine.0
            ),
        })
    }

    /// Names the ONE repair a goal's failed watch may make (owner only). The call must be a
    /// registered, reversible, local call in the goal's scope, taking `input`; an observe,
    /// outward-reversible, irreversible or unclassified call is refused here, and so is a second
    /// repair for the goal. The owner's row is made in the same step: the actor `repair` may make
    /// that call once per item (the failure episode's cap is applied by the watch, see
    /// [`Core::check_watch`]). Nothing runs until a watch fails, and then only through `decide`.
    ///
    /// # Errors
    /// [`AppError::BadRequest`] with the refusal; [`AppError::Repository`].
    pub fn add_repair(&self, proof: &OwnerProof, goal: GoalId, call: &str, input: &str) -> Res<()> {
        let g = self.store.goal(goal)?;
        let spec = self.spec(call);
        let refuse = |why: &crate::domain::RepairRefusal| {
            Err(AppError::BadRequest {
                message: why.said(),
            })
        };
        if let Err(why) = validate_repair(spec.as_ref(), &g.scope, call, input) {
            return refuse(&why);
        }
        let Some(spec) = spec else {
            return refuse(&crate::domain::RepairRefusal::UnknownCall);
        };
        if self.store.repair(goal)?.is_some() {
            return refuse(&crate::domain::RepairRefusal::AlreadySet);
        }
        self.store.insert_repair(
            &Repair {
                goal,
                call: call.to_owned(),
                input: input.to_owned(),
            },
            self.now(),
        )?;
        self.store.put_policy(&PolicyRow {
            actor: REPAIR_ACTOR.to_owned(),
            call: call.to_owned(),
            band: Reversibility::Reversible,
            tier: spec.tier.max(g.tier),
            max_uses_per_item: Some(REPAIR_USES_PER_EPISODE),
        })?;
        self.note(
            None,
            "core.repair",
            &proof.actor(),
            Kind::Control,
            true,
            &format!(
                "goal {} may repair a failed watch with {call}, once per failure episode",
                goal.0
            ),
        )?;
        Ok(())
    }

    /// Lets a paused routine fire again and clears its run of failures (owner only; the next slot
    /// is never in the past, so the pause leaves no pile of missed slots). A retired routine is
    /// refused: retiring is final.
    ///
    /// # Errors
    /// [`AppError::BadRequest`].
    pub fn resume_routine(&self, proof: &OwnerProof, routine: RoutineId) -> Res<()> {
        self.refuse_if_retired(routine, "resumed")?;
        self.store.reset_routine(routine, false, self.now())?;
        self.note(
            None,
            "core.resume_routine",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("routine {} resumed", routine.0),
        )?;
        Ok(())
    }

    /// A structured check must name a registered call, a fact that call declares, and a value
    /// of that fact's type (and, for a word, on its list): a check that could never be met, or
    /// never be compared, is refused when the goal is added, not discovered at run time.
    fn check_criteria_against_declarations(&self, criteria: &[Criterion]) -> Res<()> {
        for c in criteria {
            let Some(k) = &c.check else { continue };
            let bad = |why: String| {
                Err(AppError::BadRequest {
                    message: format!("criterion {:?}: {why}", c.text),
                })
            };
            let Some(spec) = self.spec(&k.call) else {
                return bad(format!(
                    "its check names call {}, which no capability declares",
                    k.call
                ));
            };
            if let Err(why) = spec.check_expectation(&k.field, &k.expect) {
                return bad(format!("its check on {}: {why}", k.call));
            }
        }
        Ok(())
    }

    /// Adds a one-off work item to a goal; it answers to the goal's criteria, stored once.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn add_item(&self, goal: GoalId, title: &str) -> Res<ItemId> {
        Ok(self
            .store
            .insert_item(goal, title, None, None, self.now())?)
    }

    /// Adds a work item with a subject and/or criteria of its own.
    ///
    /// # Errors
    /// [`AppError::Domain`] for an override that is empty, blank or too long; [`AppError::Repository`].
    pub fn add_item_with(&self, n: NewItem) -> Res<ItemId> {
        if let Some(own) = &n.criteria {
            validate_criteria(own)?;
            self.check_criteria_against_declarations(own)?;
        }
        Ok(self.store.insert_item(
            n.goal,
            &n.title,
            n.subject.as_deref(),
            n.criteria.as_deref(),
            self.now(),
        )?)
    }

    /// Fires every routine that is due. A routine behind by several slots records the missed ones
    /// and runs once; one whose last item is still open records a skipped slot instead of stacking.
    /// A slot can never fire twice, even across a crash and a restart.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn fire_due(&self) -> Res<Vec<Fired>> {
        let now = self.now();
        let mut out = Vec::new();
        for r in self.store.routines()? {
            if r.paused || r.retired {
                continue;
            }
            // An item awaiting acceptance blocks the next slot only when the owner must accept it.
            let open = match self.store.goal(r.goal)?.acceptance {
                AcceptancePolicy::Owner => self.store.routine_has_open_item(r.id)?,
                AcceptancePolicy::AutoObserve => self.store.routine_has_unfinished_item(r.id)?,
            };
            let plan = plan_firings(r.next_due_ms, r.every_ms, now, open);
            if plan.latest.is_none() {
                continue;
            }
            // `None`: another process fired this slot between the read above and the write.
            let Some(item) = self.store.apply_plan(&r, &plan, now)? else {
                continue;
            };
            out.push(Fired {
                routine: r.id,
                item,
                missed: u32::try_from(plan.missed).unwrap_or(u32::MAX),
                skipped_open: plan.skipped_open,
            });
        }
        Ok(out)
    }

    /// Lifts the stop a cost violation put on a goal (owner only).
    ///
    /// # Errors
    /// [`AppError::BadRequest`].
    pub fn resume_goal(&self, proof: &OwnerProof, goal: GoalId) -> Res<()> {
        self.store.set_control(&goal_stop_key(goal), 0)?;
        self.note(
            None,
            "core.resume_goal",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("goal {} resumed", goal.0),
        )?;
        Ok(())
    }

    // ---- decide: grants and the stop switch ---------------------------------------------------

    /// Grants a policy row (owner only). A row for the irreversible band is refused: slice 1 opens none.
    ///
    /// # Errors
    /// [`AppError::BadRequest`].
    pub fn grant(&self, proof: &OwnerProof, row: PolicyRow) -> Res<()> {
        if row.band == Reversibility::Irreversible {
            return Err(AppError::BadRequest {
                message: "no policy row may cover the irreversible band".to_owned(),
            });
        }
        self.store.put_policy(&row)?;
        self.note(
            None,
            "core.grant",
            &proof.actor(),
            Kind::Control,
            true,
            &format!(
                "{} may call {} up to {}",
                row.actor,
                row.call,
                row.band.name()
            ),
        )?;
        Ok(())
    }

    /// Turns the stop switch ON: every call is refused. ANYONE may stop (no proof needed): a
    /// stop can only make the system safer. The actor is recorded as given.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn stop(&self, actor: &Actor) -> Res<()> {
        self.store.set_control_retrying("stop", 1)?;
        self.note(None, "core.stop", actor, Kind::Control, true, "stop on")?;
        Ok(())
    }

    /// Turns the stop switch OFF (owner only: needs an [`OwnerProof`]).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn clear_stop(&self, proof: &OwnerProof) -> Res<()> {
        self.store.set_control_retrying("stop", 0)?;
        let actor = proof.actor();
        self.note(None, "core.stop", &actor, Kind::Control, true, "stop off")?;
        Ok(())
    }

    /// Sets the overall spend cap in micro-USD (1 USD = 1,000,000) (owner only). Slice 1 leaves it at zero.
    ///
    /// # Errors
    /// [`AppError::BadRequest`].
    pub fn set_overall_cap(&self, proof: &OwnerProof, micros: u64) -> Res<()> {
        self.store.set_control(
            "overall_cap_micros",
            i64::try_from(micros).unwrap_or(i64::MAX),
        )?;
        self.note(
            None,
            "core.cap",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("overall cap set to {micros} micro-USD"),
        )?;
        Ok(())
    }

    // ---- run and record ---------------------------------------------------------------------

    /// Starts an item: `open` to `running`, counting an attempt.
    ///
    /// # Errors
    /// [`AppError::Conflict`] with code [`ITEM_TAKEN`] when the item is no longer open, which
    /// is what a lost race with another process looks like (the caller may skip the item);
    /// [`AppError::Repository`] for anything else.
    pub fn start(&self, actor: &Actor, item: ItemId) -> Res<()> {
        match self.store.move_item(item, ItemState::Running, None) {
            Ok(()) => {}
            // Another process got there first (it listed the same open item and started it, or
            // started and finished it, between this process's listing and its start): the item
            // is simply no longer open. That is not damaged data, and nothing was written.
            Err(e @ RepositoryError::Corrupt(_)) => {
                let now = self.store.item(item)?.state;
                if now == ItemState::Open {
                    return Err(e.into());
                }
                return Err(AppError::Conflict {
                    code: ITEM_TAKEN,
                    message: format!(
                        "item {} is no longer open: another process took it (it is {})",
                        item.0,
                        now.name()
                    ),
                });
            }
            Err(e) => return Err(e.into()),
        }
        self.note(
            Some(item),
            "core.start",
            actor,
            Kind::Control,
            true,
            "item started",
        )?;
        Ok(())
    }

    /// Ends an item as failed, saying why. It does not retry.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn fail(&self, actor: &Actor, item: ItemId, why: &str) -> Res<()> {
        self.store.move_item(item, ItemState::Failed, Some(why))?;
        self.note(
            Some(item),
            "core.fail",
            actor,
            Kind::Control,
            true,
            &format!("item failed: {why}"),
        )?;
        Ok(())
    }

    /// The owner declines an item: it ends `failed` with the owner's reason, and the decision
    /// is recorded with the owner as actor (it needs an [`OwnerProof`]).
    ///
    /// # Errors
    /// [`AppError::Repository`] when the item cannot be failed from where it is.
    pub fn decline(&self, proof: &OwnerProof, item: ItemId, why: &str) -> Res<()> {
        self.store.move_item(item, ItemState::Failed, Some(why))?;
        self.note(
            Some(item),
            "core.decline",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("declined: {why}"),
        )?;
        Ok(())
    }

    /// An actor for a capability or proposer, by name. A reserved name (`core`, `owner`,
    /// `policy`, a `policy:` or `owner:` prefix) is refused AND the attempt is recorded.
    ///
    /// # Errors
    /// [`ActorError`] for a reserved or malformed name.
    pub fn capability_actor(&self, name: &str) -> Result<Actor, ActorError> {
        Actor::capability(name).inspect_err(|e| {
            let _ = self.note(
                None,
                "core.actor",
                &Actor::core(),
                Kind::Refusal,
                false,
                &format!("refused: {e}"),
            );
        })
    }

    /// One proposed call, from anyone: decided by policy alone, whatever the proposer claims.
    /// A refusal is recorded and nothing runs; an allowed call records its intent first, runs
    /// through the one port, and records its result.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn call(&self, actor: &Actor, item: ItemId, call: &str, input: &str) -> Res<Outcome> {
        self.call_counting(actor, item, call, input, 0)
    }

    /// [`Core::call`], with `earlier_uses` more uses of the call than this item's own counted
    /// against the policy row's cap (a repair's earlier items in one failure episode).
    fn call_counting(
        &self,
        actor: &Actor,
        item: ItemId,
        call: &str,
        input: &str,
        earlier_uses: u32,
    ) -> Res<Outcome> {
        let actor_kind = actor.kind();
        let actor_name = actor.name();
        let actor = actor_name.as_str();
        let it = self.store.item(item)?;
        let goal = self.store.goal(it.goal)?;
        let routed = self
            .caps
            .iter()
            .find(|c| c.specs.iter().any(|s| s.name == call));
        if let Some(reg) = routed {
            // A capability whose live declaration now names calls it did not declare at
            // registration is recorded; those names are never routed to it (no snapshot row).
            for live in reg.cap.offers() {
                if !reg.specs.iter().any(|s| s.name == live.name) {
                    self.note(
                        Some(item),
                        "core.violation",
                        &Actor::core(),
                        Kind::Control,
                        false,
                        &format!(
                            "a capability offers call {} it did not declare at registration: refused",
                            live.name
                        ),
                    )?;
                }
            }
        }
        let spec = self.spec(call);
        let band = spec
            .as_ref()
            .map_or_else(Reversibility::default, CallSpec::effective_band);
        // Spent = the actual cost on record, plus the declared maximum of any call that was
        // allowed but left no result (a crash may have cut it: assume the worst).
        let spent = |g: Option<GoalId>| -> Res<u64> {
            let open: u64 = self
                .store
                .unsettled_calls(g)?
                .iter()
                .filter_map(|c| self.spec(c))
                .map(|s| s.max_cost_micros)
                .sum();
            Ok(self.store.spent_micros(g)?.saturating_add(open))
        };
        let cap = |n: i64| u64::try_from(n).unwrap_or(0);
        let situation = Situation {
            stopped: self.store.control("stop")? != 0,
            item_running: it.state == ItemState::Running,
            goal_stopped: self.store.control(&goal_stop_key(goal.id))? != 0,
            goal_scope: &goal.scope,
            goal_tier: goal.tier,
            uses_this_item: self
                .store
                .uses(actor, call, item)?
                .saturating_add(earlier_uses),
            goal_spent_micros: spent(Some(goal.id))?,
            goal_cap_micros: goal.spend_cap_micros,
            total_spent_micros: spent(None)?,
            total_cap_micros: cap(self.store.control("overall_cap_micros")?),
        };
        let verdict = decide(
            actor,
            call,
            spec.as_ref(),
            &self.store.policy()?,
            &situation,
        );
        let record = |kind, allowed, intent_id, cost_micros, output: &str| Evidence {
            id: 0,
            item: Some(item),
            call: call.to_owned(),
            actor: actor.to_owned(),
            actor_kind,
            band,
            allowed,
            kind,
            intent_id,
            inputs_hash: inputs_hash(call, input),
            output: output.to_owned(),
            at_ms: self.now(),
            cost_micros,
            profile: None,
        };
        if let Verdict::Deny(why) = verdict {
            self.store
                .append_evidence(&record(Kind::Refusal, false, None, 0, why.said()))?;
            return Ok(Outcome::Refused(why));
        }
        let Some(spec) = spec else {
            return Ok(Outcome::Failed("no capability offers the call".to_owned()));
        };
        if let Err(why) = spec.check_input(input) {
            let said = format!("{}: {why}", Deny::BadInput.said());
            self.store
                .append_evidence(&record(Kind::Refusal, false, None, 0, &said))?;
            return Ok(Outcome::Refused(Deny::BadInput));
        }
        // The spend check and the intent are ONE transaction, so another process on the same
        // file cannot pass the same check: this reservation (the declared maximum) counts
        // against both caps until the actual cost replaces it.
        let reserved = self.store.append_intent_reserving(
            &record(Kind::Intent, true, None, 0, ""),
            goal.id,
            goal.spend_cap_micros,
            spec.max_cost_micros,
            &|name| self.spec(name).map_or(0, |s| s.max_cost_micros),
        )?;
        let Some(intent) = reserved else {
            self.store.append_evidence(&record(
                Kind::Refusal,
                false,
                None,
                0,
                Deny::SpendCap.said(),
            ))?;
            return Ok(Outcome::Refused(Deny::SpendCap));
        };
        let Some(reg) = routed else {
            return Ok(Outcome::Failed("no capability offers the call".to_owned()));
        };
        let max = spec.max_cost_micros;
        // Which line holds the budget: the executor's own enforcement (when it declared one), with
        // Core's check-before and violation-after as the second line either way.
        let line = match spec.budget {
            BudgetMode::Enforced { .. } if max > 0 => "enforced-by-executor",
            _ => "none: maximum cost is zero",
        };
        let budget = Budget {
            max_cost_micros: max,
            // The spec declares the most it can accept; Core grants that (there is no per-call
            // request yet, and no shared pool of seconds or turns to run short of).
            max_seconds: spec.max_seconds,
            max_turns: spec.max_turns,
        };
        let facts_text = |facts: &[crate::domain::Fact]| {
            facts
                .iter()
                .map(|f| format!("{}={}", f.name, f.rendered()))
                .collect::<Vec<_>>()
                .join(" ")
        };
        // A result or failure that breaks the call's declared shape is a violation: the cost is
        // kept, the facts are not, the item fails and the goal stops until the owner resumes it.
        let contract_violation = |why: String, cost: u64, intent: i64| -> Res<Outcome> {
            let cost = cost.min(max);
            self.store.append_evidence(&record(
                Kind::Result,
                true,
                Some(intent),
                cost,
                &format!("result refused, facts not kept: {why}"),
            ))?;
            let said = format!(
                "call {call} broke its declared contract ({why}): goal {} stopped",
                goal.id.0
            );
            self.store.set_control(&goal_stop_key(goal.id), 1)?;
            self.note(
                Some(item),
                "core.violation",
                &Actor::core(),
                Kind::Control,
                false,
                &said,
            )?;
            self.store.move_item(item, ItemState::Failed, Some(&said))?;
            Ok(Outcome::Contract(why))
        };
        match reg.cap.call_budgeted(call, input, &budget) {
            Ok(output) => {
                if let Err(why) = spec.check_facts(&output.facts) {
                    return contract_violation(why, output.cost_micros, intent);
                }
                let facts = facts_text(&output.facts);
                let mut kept = output.result.clone();
                if !facts.is_empty() {
                    kept.push_str(&format!("\n[facts] {facts}"));
                }
                if max > 0 {
                    kept.push_str(&format!("\n[budget] {line}: up to {max} micro-USD"));
                }
                if !output.evidence.is_empty() {
                    kept.push_str(&format!("\n-- {}", output.evidence));
                }
                // The charged cost never exceeds the declared maximum; the raw report stays in
                // the text and the violation below.
                let charged = output.cost_micros.min(max);
                if charged != output.cost_micros {
                    kept.push_str(&format!(
                        "\n[cost] reported {} micro-USD, charged the declared maximum {max}",
                        output.cost_micros
                    ));
                }
                let evidence_id = self.store.append_evidence(&Evidence {
                    profile: output.profile.clone(),
                    ..record(Kind::Result, true, Some(intent), charged, &kept)
                })?;
                if output.cost_micros > max {
                    let said = format!(
                        "call {call} cost {} micro-USD, above its declared maximum of {max} ({line}): goal {} stopped",
                        output.cost_micros, goal.id.0
                    );
                    self.store.set_control(&goal_stop_key(goal.id), 1)?;
                    self.note(
                        Some(item),
                        "core.violation",
                        &Actor::core(),
                        Kind::Control,
                        false,
                        &said,
                    )?;
                    self.store.move_item(item, ItemState::Failed, Some(&said))?;
                    return Ok(Outcome::Violation {
                        max,
                        actual: output.cost_micros,
                    });
                }
                Ok(Outcome::Done(Ran {
                    output,
                    evidence_id,
                }))
            }
            Err(failure) => {
                if let Err(why) = spec.check_facts(&failure.facts) {
                    return contract_violation(why, failure.cost_micros.unwrap_or(max), intent);
                }
                // Unknown cost: charge the declared maximum, the worst case.
                let cost = failure.cost_micros.unwrap_or(max);
                let facts = facts_text(&failure.facts);
                let mut kept = format!("failed: {}", failure.reason);
                if !facts.is_empty() {
                    kept.push_str(&format!("\n[facts] {facts}"));
                }
                let charged = cost.min(max);
                if charged != cost {
                    kept.push_str(&format!(
                        "\n[cost] reported {cost} micro-USD, charged the declared maximum {max}"
                    ));
                }
                self.store.append_evidence(&Evidence {
                    profile: failure.profile.clone(),
                    ..record(Kind::Result, true, Some(intent), charged, &kept)
                })?;
                if cost > max {
                    let said = format!(
                        "call {call} cost {cost} micro-USD, above its declared maximum of {max} ({line}): goal {} stopped",
                        goal.id.0
                    );
                    self.store.set_control(&goal_stop_key(goal.id), 1)?;
                    self.note(
                        Some(item),
                        "core.violation",
                        &Actor::core(),
                        Kind::Control,
                        false,
                        &said,
                    )?;
                    self.store.move_item(item, ItemState::Failed, Some(&said))?;
                    return Ok(Outcome::Violation { max, actual: cost });
                }
                Ok(Outcome::Failed(failure.reason))
            }
        }
    }

    // ---- require ----------------------------------------------------------------------------

    /// Runs each unmet criterion's deterministic check (as [`VERIFIER`], through the same
    /// decision as any call) and marks it met when the call's structured result has the named
    /// field with exactly the expected value (its text is never read).
    /// Returns, per criterion, whether it is met now. Never closes the item.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn verify(&self, item: ItemId) -> Res<Vec<bool>> {
        let subject = self.store.item(item)?.subject;
        for (idx, k) in self.store.criteria(item)?.iter().enumerate() {
            let Some(check) = k.criterion.check.as_ref().filter(|_| k.met_by.is_none()) else {
                continue;
            };
            if let Outcome::Done(ran) = self.call(
                &Actor::core(),
                item,
                &check.call,
                &check.input_for(subject.as_deref()),
            )? {
                if check.met_by(&ran.output.facts) {
                    self.store
                        .mark_met(item, idx, "check", Some(ran.evidence_id))?;
                }
            }
        }
        Ok(self
            .store
            .criteria(item)?
            .iter()
            .map(|k| k.met_by.is_some())
            .collect())
    }

    /// The owner confirms a criterion that has no deterministic check.
    ///
    /// # Errors
    /// [`AppError::Conflict`] with code [`ITEM_FINISHED`] when the item is already `done` or
    /// `failed` (nothing is written); [`AppError::Repository`] otherwise, `Corrupt` for an
    /// unreadable stored state.
    pub fn confirm_criterion(&self, proof: &OwnerProof, item: ItemId, idx: usize) -> Res<()> {
        if !self.store.confirm_unfinished(item, idx)? {
            let state = self.store.item(item)?.state;
            return Err(AppError::Conflict {
                code: ITEM_FINISHED,
                message: format!("item {} is already {}", item.0, state.name()),
            });
        }
        self.note(
            Some(item),
            "core.confirm",
            &proof.actor(),
            Kind::Control,
            true,
            &format!("criterion {} confirmed", idx + 1),
        )?;
        Ok(())
    }

    /// The owner accepts an item as done: needs every criterion met and an [`OwnerProof`], so
    /// no capability, agent or caller of any other kind can accept (or accept its own work).
    ///
    /// ```compile_fail
    /// // An actor of any kind is not an OwnerProof: this does not compile.
    /// use endora_foundation::{application::Core, domain::{Actor, ItemId}};
    /// fn spoof(core: &Core) {
    ///     let me = Actor::capability("agent").unwrap();
    ///     let _ = core.accept(&me, ItemId(1));
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// // Nor can an OwnerProof be built by hand (its field is private).
    /// use endora_foundation::application::{Core, OwnerProof};
    /// fn forge(core: &Core) {
    ///     let proof = OwnerProof {};
    ///     let _ = core.accept(&proof, endora_foundation::domain::ItemId(1));
    /// }
    /// ```
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn accept(&self, proof: &OwnerProof, item: ItemId) -> Res<Acceptance> {
        self.accept_as(&proof.actor(), item)
    }

    /// The one acceptance path: the owner (through [`Core::accept`]) or a standing policy
    /// (through [`Core::settle_by_policy`] / [`Core::settle_routine_by_policy`]). Crate-private,
    /// so no caller can name an actor of its own choosing.
    fn accept_as(&self, actor: &Actor, item: ItemId) -> Res<Acceptance> {
        let it = self.store.item(item)?;
        let criteria = self.store.criteria(item)?;
        let routine = match it.routine {
            Some(r) => Some(self.store.routine(r)?),
            None => None,
        };
        let highest = self
            .store
            .bands_used(item)?
            .into_iter()
            .max()
            .unwrap_or(Reversibility::Observe);
        let facts = AcceptFacts {
            state: it.state,
            all_met: criteria.iter().all(|k| k.met_by.is_some()),
            all_by_check: criteria
                .iter()
                .all(|k| k.met_by.as_deref() == Some("check")),
            highest_band: highest,
            from_routine: routine.is_some(),
            routine_auto_accept: routine.as_ref().is_some_and(|r: &Routine| r.auto_accept),
            goal_auto_observe: self.store.goal(it.goal)?.acceptance
                == AcceptancePolicy::AutoObserve,
        };
        match authorise(actor, &facts) {
            Err(why) => {
                self.note(
                    Some(item),
                    "core.accept",
                    actor,
                    Kind::Refusal,
                    false,
                    why.said(),
                )?;
                Ok(Acceptance::Refused(why))
            }
            Ok(by) => {
                self.store.accept_item(item, &by, self.now())?;
                self.note(
                    Some(item),
                    "core.accept",
                    actor,
                    Kind::Control,
                    true,
                    &if *actor == Actor::policy(AUTO_OBSERVE) {
                        format!("accepted by {by}: every call observe-band, every criterion met by a structured check")
                    } else {
                        format!("accepted by {by}")
                    },
                )?;
                Ok(Acceptance::Accepted { by })
            }
        }
    }

    /// Applies the item's goal policy after verification: for an `auto-observe` goal, asks for
    /// acceptance as `policy:auto-observe` (refused, and recorded, when a call went above the
    /// observe band or a criterion is unmet or not settled by a check, so the owner decides).
    /// `None` for an `owner` goal: nothing is attempted or recorded.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn settle_by_policy(&self, item: ItemId) -> Res<Option<Acceptance>> {
        let goal = self.store.goal(self.store.item(item)?.goal)?;
        if goal.acceptance != AcceptancePolicy::AutoObserve {
            return Ok(None);
        }
        self.accept_as(&Actor::policy(AUTO_OBSERVE), item).map(Some)
    }

    /// Applies the routine's own `auto_accept` setting: asks for acceptance as
    /// `policy:auto_accept` (the policy accepts only an observe-band item from a routine that
    /// allows it, every criterion met by a deterministic check; anything else is refused and
    /// recorded).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn settle_routine_by_policy(&self, item: ItemId) -> Res<Acceptance> {
        self.accept_as(&Actor::policy(AUTO_ACCEPT), item)
    }

    /// Settles the items of `auto-observe` goals that a dead process left `running` after their
    /// last check was met and before [`Core::settle_by_policy`] ran (a kill between the two
    /// strands them as an open decision for the owner for ever). Only an item the policy would
    /// accept is touched, so nothing is refused or recorded per run: every criterion met by a
    /// structured check, no call above the observe band, no call still unsettled (that is
    /// [`Core::recover_interrupted`]'s), and no live process running it. Returns what each
    /// settling did.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn settle_stranded_auto_observe(&self) -> Res<Vec<(ItemId, Acceptance)>> {
        let mut out = Vec::new();
        for it in self.store.items_in(ItemState::Running)? {
            if self.store.goal(it.goal)?.acceptance != AcceptancePolicy::AutoObserve {
                continue;
            }
            let criteria = self.store.criteria(it.id)?;
            let by_check = !criteria.is_empty()
                && criteria
                    .iter()
                    .all(|k| k.met_by.as_deref() == Some("check"));
            let observe_only = self
                .store
                .bands_used(it.id)?
                .into_iter()
                .all(|b| b <= Reversibility::Observe);
            if !by_check
                || !observe_only
                || !self.store.unsettled_intents_this_attempt(it.id)?.is_empty()
                || !self.store.adopt(it.id)?
            {
                continue;
            }
            if let Some(got) = self.settle_by_policy(it.id)? {
                out.push((it.id, got));
            }
        }
        Ok(out)
    }

    /// Recomputes the evidence hash chain from the rows as they stand. This is DETECTION, not
    /// prevention: it finds an edited, removed or spliced row after the append-only triggers
    /// were dropped, but it does not stop anyone writing the file, and it cannot see the
    /// removal of the last rows (keep [`ChainReport::head`] elsewhere for that).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn verify_evidence(&self) -> Res<ChainReport> {
        Ok(self.store.verify_evidence_chain()?)
    }

    // ---- recover ----------------------------------------------------------------------------

    /// Settles every item a dead process left `running`, from durable state alone. An item with a
    /// reversible call that was allowed but has no result may have changed the world and is
    /// failed explicitly. An item out of attempts fails. Every other one goes back to `open`
    /// to be resumed; its evidence stays. Call it once at start, before any runner starts.
    ///
    /// An item a LIVE process is running is not "left" by anyone and is not touched: each item
    /// names its holder (a process-lifetime lock the OS drops when the process dies, see
    /// `infrastructure::holders`), and only an item whose holder is dead, unrecorded (written
    /// by an older build) or this process itself is settled.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn recover(&self) -> Res<Vec<Recovered>> {
        self.recover_where(false)
    }

    /// Like [`Core::recover`], but only for running items a dead process cut off inside a call
    /// (an allowed intent with no result). A running item that merely awaits the owner's
    /// acceptance is left alone, so a command-line run can call it on every start. An item a
    /// live process holds is left alone too (see [`Core::recover`]).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn recover_interrupted(&self) -> Res<Vec<Recovered>> {
        self.recover_where(true)
    }

    fn recover_where(&self, only_cut_off: bool) -> Res<Vec<Recovered>> {
        let mut out = Vec::new();
        for it in self.store.items_in(ItemState::Running)? {
            if only_cut_off && self.store.unsettled_intents_this_attempt(it.id)?.is_empty() {
                continue;
            }
            // A live process running the item settles it itself; only a dead holder's is ours.
            if !self.store.adopt(it.id)? {
                continue;
            }
            let it = self.store.item(it.id)?;
            let unsettled = self.store.unsettled_intents_this_attempt(it.id)?;
            if only_cut_off && unsettled.is_empty() {
                continue;
            }
            let risky = unsettled.iter().any(|e| e.band > Reversibility::Observe);
            let (now, why) = if risky {
                (ItemState::Failed, "interrupted inside a call above the observe band: unknown whether it took effect".to_owned())
            } else if it.attempts >= MAX_ATTEMPTS {
                (
                    ItemState::Failed,
                    format!("interrupted again after {} attempts", it.attempts),
                )
            } else {
                (
                    ItemState::Open,
                    format!(
                        "interrupted on attempt {}: resumed from its recorded evidence",
                        it.attempts
                    ),
                )
            };
            self.store.move_item(it.id, now, Some(&why))?;
            self.note(
                Some(it.id),
                "core.recover",
                &Actor::core(),
                Kind::Control,
                true,
                &why,
            )?;
            out.push(Recovered {
                item: it.id,
                now,
                why,
            });
        }
        Ok(out)
    }

    // ---- the plain-text status --------------------------------------------------------------

    /// Goals, items and the last `tail` evidence records, as plain text.
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn status_text(&self, tail: usize) -> Res<String> {
        status_text(&self.store, tail)
    }

    /// The status with a bounded item listing (see [`ItemListing`]).
    ///
    /// # Errors
    /// [`AppError::Repository`].
    pub fn status_text_listing(&self, tail: usize, listing: ItemListing) -> Res<String> {
        status_text_listing(&self.store, tail, listing)
    }
}

/// How many of a goal's items the status lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemListing {
    /// Every item of every goal.
    All,
    /// Per goal: a total with its states, and at most this many of its most recent items.
    Recent(usize),
}

/// Plain text status of a store: goals, every item and the evidence tail.
///
/// # Errors
/// [`AppError::Repository`].
pub fn status_text(store: &Store, tail: usize) -> Res<String> {
    status_text_listing(store, tail, ItemListing::All)
}

fn item_line(store: &Store, i: &WorkItem) -> Res<String> {
    let met = store.criteria(i.id)?;
    let n_met = met.iter().filter(|k| k.met_by.is_some()).count();
    Ok(format!(
        "  #{} [{}] goal #{} {} criteria {}/{} attempts {} missed {}{}{}\n",
        i.id.0,
        i.state.name(),
        i.goal.0,
        i.title,
        n_met,
        met.len(),
        i.attempts,
        i.missed,
        i.accepted_by
            .as_ref()
            .map(|b| format!(" accepted_by {b}"))
            .unwrap_or_default(),
        i.reason
            .clone()
            .map(|r| format!(" ({r})"))
            .unwrap_or_default()
    ))
}

/// One goal's items in a bounded status: its total and states, then its newest `keep` items.
fn recent_items(store: &Store, goal: GoalId, items: &[WorkItem], keep: usize) -> Res<String> {
    let mine: Vec<&WorkItem> = items.iter().filter(|i| i.goal == goal).collect();
    let states: Vec<String> = [
        ItemState::Open,
        ItemState::Running,
        ItemState::Blocked,
        ItemState::Failed,
        ItemState::Done,
    ]
    .iter()
    .filter_map(|st| {
        let n = mine.iter().filter(|i| i.state == *st).count();
        (n > 0).then(|| format!("{n} {}", st.name()))
    })
    .collect();
    let mut s = format!(
        "  goal #{}: {} items{}\n",
        goal.0,
        mine.len(),
        if states.is_empty() {
            String::new()
        } else {
            format!(": {}", states.join(", "))
        }
    );
    let hidden = mine.len().saturating_sub(keep);
    if hidden > 0 {
        s.push_str(&format!(
            "  ... {hidden} older items not shown (`status --short --all` lists every one)\n"
        ));
    }
    for i in &mine[hidden..] {
        s.push_str(&item_line(store, i)?);
    }
    Ok(s)
}

/// Plain text status of a store with a choice of how many items to list.
///
/// # Errors
/// [`AppError::Repository`].
pub fn status_text_listing(store: &Store, tail: usize, listing: ItemListing) -> Res<String> {
    let mut s = format!(
        "STOP SWITCH: {}\n\nGOALS\n",
        if store.control("stop")? != 0 {
            "ON"
        } else {
            "off"
        }
    );
    let goals = store.goals()?;
    for g in &goals {
        s.push_str(&format!(
            "  #{} {} (why: {}) scope: {}\n",
            g.id.0,
            g.title,
            g.why,
            g.scope.join(", ")
        ));
    }
    s.push_str("\nITEMS\n");
    let items = store.items()?;
    match listing {
        ItemListing::All => {
            for i in &items {
                s.push_str(&item_line(store, i)?);
            }
        }
        ItemListing::Recent(keep) => {
            for g in &goals {
                s.push_str(&recent_items(store, g.id, &items, keep)?);
            }
        }
    }
    s.push_str("\nEVIDENCE (tail)\n");
    for e in store.evidence_tail(tail)? {
        let first = e.output.lines().next().unwrap_or("");
        s.push_str(&format!(
            "  e{} t={} item={} {} {} {} {} {}{}: {}\n",
            e.id,
            e.at_ms,
            e.item.map_or("-".to_owned(), |i| i.0.to_string()),
            e.actor,
            e.call,
            e.band.name(),
            if e.allowed { "allow" } else { "DENY" },
            e.kind.name(),
            if e.cost_micros > 0 {
                format!(" cost={} micro-USD", e.cost_micros)
            } else {
                String::new()
            },
            first
        ));
    }
    Ok(s)
}

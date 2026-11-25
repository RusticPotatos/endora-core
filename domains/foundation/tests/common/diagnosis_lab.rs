//! A failed watch to diagnose: one probe whose text the test sets, a goal that needs it up, and a
//! model that replays a fixed list of replies. Shared by the diagnosis tests.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use endora_foundation::application::{
    CallFailure, CallOutput, Capability, Core, DiagnosisModel, NewGoal,
};
use endora_foundation::domain::{
    CallSpec, Check, Criterion, Fact, Field, ItemId, ItemState, PolicyRow,
};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

use super::{actor_named, owner};

const MIN: i64 = 60_000;

/// A probe that is down and says `text` about it.
pub struct Probe {
    pub text: Mutex<String>,
    pub up: AtomicBool,
}

impl Capability for Probe {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec::new("p.look", Reversibility::Observe).with_facts(vec![Field::bool("up")])]
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        let mut o = CallOutput::text(&self.text.lock().unwrap());
        o.facts = vec![Fact::bool("up", self.up.load(Ordering::SeqCst))];
        Ok(o)
    }
}

/// Replays recorded replies in order and keeps every prompt it was shown. An exhausted list is a
/// transport error, so a test that asks too often fails loudly.
pub struct Replay {
    pub replies: Mutex<Vec<Result<String, String>>>,
    pub prompts: Mutex<Vec<String>>,
}

impl Replay {
    pub fn new(replies: &[&str]) -> Self {
        Self {
            replies: Mutex::new(replies.iter().rev().map(|r| Ok((*r).to_owned())).collect()),
            prompts: Mutex::new(Vec::new()),
        }
    }
    pub fn failing(why: &str) -> Self {
        Self {
            replies: Mutex::new(vec![Err(why.to_owned())]),
            prompts: Mutex::new(Vec::new()),
        }
    }
    pub fn asked(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
}

impl DiagnosisModel for Replay {
    fn ask(&self, prompt: &str) -> Result<String, String> {
        self.prompts.lock().unwrap().push(prompt.to_owned());
        self.replies
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| Err("no recorded reply left".to_owned()))
    }
}

/// A core with one failed watch item whose probe said `text`. Returns the core and the item.
pub fn failed_watch(text: &str) -> (Core, ItemId) {
    let probe = Arc::new(Probe {
        text: Mutex::new(text.to_owned()),
        up: AtomicBool::new(false),
    });
    let clock = Arc::new(ManualClock::new(1_000_000));
    let mut core = Core::open(":memory:", clock.clone()).unwrap();
    core.register(probe).unwrap();
    let goal = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "watch".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "up".into(),
                    check: Some(Check {
                        call: "p.look".into(),
                        input: String::new(),
                        field: "up".into(),
                        expect: "true".into(),
                    }),
                }],
                scope: vec!["p.look".into()],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    core.grant(
        &owner(&core),
        PolicyRow {
            actor: "core".into(),
            call: "p.look".into(),
            band: Reversibility::Observe,
            tier: Tier::Internal,
            max_uses_per_item: None,
        },
    )
    .unwrap();
    core.add_routine_limited(&owner(&core), goal, "web front", MIN, false, 0, 50)
        .unwrap();
    clock.advance(MIN);
    core.fire_due().unwrap();
    let it = core.store().items_in(ItemState::Open).unwrap().remove(0);
    core.start(&actor_named("runner"), it.id).unwrap();
    core.check_watch(it.id).unwrap();
    assert_eq!(core.store().item(it.id).unwrap().state, ItemState::Failed);
    (core, it.id)
}

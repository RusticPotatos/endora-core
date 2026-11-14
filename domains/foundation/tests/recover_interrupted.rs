//! Slice 3 item 6 (needed by the command line): an item that is `running` because it awaits the
//! owner's acceptance is not an interrupted one. `recover_interrupted` settles only the items a
//! dead process cut off in the middle of a call (an intent with no result).

mod common;
use common::{actor_named, owner};
use std::sync::Arc;

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core, NewGoal};
use endora_foundation::domain::{CallSpec, Criterion, ItemState};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::Tier;

struct NoCalls;
impl Capability for NoCalls {
    fn offers(&self) -> Vec<CallSpec> {
        Vec::new()
    }
    fn call(&self, _c: &str, _i: &str) -> Result<CallOutput, CallFailure> {
        Err(CallFailure::new("none"))
    }
}

#[test]
fn only_items_cut_off_inside_a_call_are_recovered() {
    let mut core = Core::in_memory(Arc::new(ManualClock::new(1))).unwrap();
    core.register(Arc::new(NoCalls)).unwrap();
    let g = core
        .add_goal(
            &owner(&core),
            NewGoal {
                title: "t".into(),
                why: "w".into(),
                criteria: vec![Criterion {
                    text: "c".into(),
                    check: None,
                }],
                scope: vec![],
                tier: Tier::Internal,
                spend_cap_micros: 0,
            },
        )
        .unwrap();
    let waiting = core.add_item(g, "waiting for the owner").unwrap();
    let cut = core.add_item(g, "cut off").unwrap();
    core.start(&actor_named("runner"), waiting).unwrap();
    core.start(&actor_named("runner"), cut).unwrap();
    core.store()
        .db()
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO evidence (item_id, call, actor, band, decision, kind, inputs_hash, output, at_ms) VALUES (?1,'x.read','core','observe','allow','intent','h','',1)",
            [cut.0],
        )
        .unwrap();
    let out = core.recover_interrupted().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item, cut);
    assert_eq!(core.store().item(cut).unwrap().state, ItemState::Open);
    assert_eq!(
        core.store().item(waiting).unwrap().state,
        ItemState::Running
    );
}

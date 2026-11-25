//! `registry.tags` under Core: `decide` treats it as observe band at zero cost, so a standing
//! check on it runs for the engine and policy may accept the item; an update that is available
//! or a failed fetch never counts as "current". No network: a fake source serves the pages.

mod common;
use common::{actor_named, owner};

use std::sync::Arc;

use endora_foundation::application::{Acceptance, Core, NewGoal};
use endora_foundation::domain::{Check, Criterion, PolicyRow};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_observe::registry::{RegistryTags, TagSource};
use endora_kernel::{Reversibility, Tier};

struct Pages(Result<String, String>);
impl TagSource for Pages {
    fn get(&self, _url: &str) -> Result<String, String> {
        self.0.clone()
    }
}

fn page(names: &[&str]) -> String {
    let results: Vec<String> = names
        .iter()
        .map(|n| format!(r#"{{"name":"{n}"}}"#))
        .collect();
    format!(r#"{{"next":null,"results":[{}]}}"#, results.join(","))
}

fn engine(name: &str, pages: Result<String, String>) -> Core {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("registry-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("core.db");
    let mut core = Core::open(db.to_str().unwrap(), Arc::new(SystemClock)).unwrap();
    core.register(Arc::new(
        RegistryTags::new("hub.docker.com", Box::new(Pages(pages))).unwrap(),
    ))
    .unwrap();
    core
}

fn goal() -> NewGoal {
    NewGoal {
        title: "example/app is on its newest stable tag".into(),
        why: "watch the pin".into(),
        criteria: vec![Criterion {
            text: "no newer stable tag".into(),
            check: Some(Check {
                call: "registry.tags".into(),
                input: "example/app\n1.4.0".into(),
                field: "update".into(),
                expect: "current".into(),
            }),
        }],
        scope: vec!["registry.tags".into()],
        tier: Tier::Public,
        spend_cap_micros: 0,
    }
}

fn run(core: &Core) -> (Vec<bool>, Option<Acceptance>) {
    let g = core.add_goal(&owner(core), goal()).unwrap();
    for who in ["core", "runner"] {
        core.grant(
            &owner(core),
            PolicyRow {
                actor: who.into(),
                call: "registry.tags".into(),
                band: Reversibility::Observe,
                tier: Tier::Public,
                max_uses_per_item: None,
            },
        )
        .unwrap();
    }
    let r = core
        .add_routine(&owner(core), g, "pin", 1_000, true, 0)
        .unwrap();
    let _ = r;
    let item = core.fire_due().unwrap()[0].item.unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    let v = core.verify(item).unwrap_or_default();
    let a = core.settle_routine_by_policy(item).ok();
    (v, a)
}

#[test]
fn a_pin_that_is_current_is_verified_and_accepted_by_policy_at_zero_cost() {
    let core = engine("current", Ok(page(&["1.4.0", "1.3.0"])));
    let (v, a) = run(&core);
    assert_eq!(v, vec![true]);
    assert!(
        matches!(a, Some(Acceptance::Accepted { ref by }) if by.starts_with("policy:")),
        "{a:?}"
    );
}

#[test]
fn an_available_update_is_not_current_and_is_not_accepted() {
    let core = engine("available", Ok(page(&["1.5.0", "1.4.0"])));
    let (v, a) = run(&core);
    assert_eq!(v, vec![false]);
    assert!(!matches!(a, Some(Acceptance::Accepted { .. })), "{a:?}");
}

#[test]
fn a_fetch_error_is_never_current_and_is_not_accepted() {
    let core = engine("error", Err("connection refused".into()));
    let (v, a) = run(&core);
    assert_ne!(v, vec![true]);
    assert!(!matches!(a, Some(Acceptance::Accepted { .. })), "{a:?}");
}

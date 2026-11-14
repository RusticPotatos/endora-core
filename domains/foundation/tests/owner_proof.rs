//! The owner credential: only the token proves the owner; the database keeps a salted hash.

mod common;
use std::sync::Arc;
use std::time::Instant;

use common::{TOKEN, actor_named, owner};
use endora_foundation::application::{AuthError, Core, NewGoal};
use endora_foundation::domain::{ActorKind, Criterion, ItemState, Kind};
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::Tier;

fn goal() -> NewGoal {
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
    }
}

fn core() -> Core {
    Core::in_memory(Arc::new(ManualClock::new(1_000))).unwrap()
}

#[test]
fn without_an_enrolled_credential_nobody_is_the_owner_and_the_attempt_is_recorded() {
    let core = core();
    assert_eq!(
        core.authenticate_owner(TOKEN).unwrap_err(),
        AuthError::NotEnrolled
    );
    let ev = core.store().evidence_all().unwrap();
    assert!(ev.iter().any(|e| e.call == "core.auth" && !e.allowed));
}

#[test]
fn a_wrong_token_fails_waits_and_records_auth_failed_without_any_token_content() {
    let core = core();
    core.enroll_owner(TOKEN).unwrap();
    let guess = "f".repeat(64);
    let t = Instant::now();
    assert_eq!(
        core.authenticate_owner(&guess).unwrap_err(),
        AuthError::Wrong
    );
    assert!(t.elapsed().as_millis() >= 200, "a failure waits a little");
    let rows: Vec<_> = core
        .store()
        .evidence_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.call == "core.auth")
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].output.starts_with("auth-failed"));
    assert!(!rows[0].allowed && rows[0].kind == Kind::Refusal);
    let all = format!("{:?}", core.store().evidence_all().unwrap());
    assert!(!all.contains(&guess) && !all.contains(TOKEN));
    // A near miss (one character) is wrong as well.
    let mut near = TOKEN.to_owned();
    near.pop();
    near.push('0');
    assert!(core.authenticate_owner(&near).is_err());
    assert!(core.authenticate_owner("").is_err());
}

#[test]
fn the_right_token_proves_the_owner_and_the_decision_is_recorded_as_owner() {
    let core = core();
    let o = owner(&core);
    let g = core.add_goal(&o, goal()).unwrap();
    let item = core.add_item(g, "i").unwrap();
    core.start(&actor_named("runner"), item).unwrap();
    core.confirm_criterion(&o, item, 0).unwrap();
    core.accept(&o, item).unwrap();
    let ev = core.store().evidence_for(item).unwrap();
    for call in ["core.confirm", "core.accept"] {
        let e = ev.iter().find(|e| e.call == call).unwrap();
        assert_eq!(
            (e.actor.as_str(), e.actor_kind),
            ("owner", ActorKind::Owner)
        );
    }
}

#[test]
fn the_credential_is_made_once_and_the_token_is_never_stored() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("owner-cred-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.db");
    let core = Core::open(path.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
    assert!(
        core.enroll_owner("short").is_err(),
        "a short token is refused"
    );
    let token = Core::generate_owner_token().unwrap();
    assert_eq!(token.len(), 64);
    core.enroll_owner(&token).unwrap();
    assert!(core.enroll_owner(&token).is_err(), "no second owner");
    assert!(core.enroll_owner(&"a".repeat(64)).is_err());
    // The row is a salt and a hash; raw SQL cannot change or remove it either (triggers).
    let (salt, hash) = core.store().owner_credential().unwrap().unwrap();
    assert_eq!((salt.len(), hash.len()), (32, 64));
    let db = core.store().db().lock().unwrap();
    assert!(
        db.execute("UPDATE owner_credential SET hash = 'x'", [])
            .is_err()
    );
    assert!(db.execute("DELETE FROM owner_credential", []).is_err());
    drop(db);
    drop(core);
    // The file's bytes contain neither the token nor its raw bytes.
    let bytes = std::fs::read(&path).unwrap();
    let hay = String::from_utf8_lossy(&bytes);
    assert!(!hay.contains(&token));
    let raw: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&token[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    assert!(!bytes.windows(32).any(|w| w == raw.as_slice()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn decline_is_an_owner_decision_with_evidence_and_start_and_fail_record_their_actor() {
    let core = core();
    let o = owner(&core);
    let g = core.add_goal(&o, goal()).unwrap();
    let item = core.add_item(g, "i").unwrap();
    let runner = actor_named("runner");
    core.start(&runner, item).unwrap();
    core.decline(&o, item, "not what I wanted").unwrap();
    assert_eq!(core.store().item(item).unwrap().state, ItemState::Failed);
    let ev = core.store().evidence_for(item).unwrap();
    let d = ev.iter().find(|e| e.call == "core.decline").unwrap();
    assert_eq!(
        (d.actor.as_str(), d.actor_kind),
        ("owner", ActorKind::Owner)
    );
    assert!(d.output.contains("not what I wanted"));
    let s = ev.iter().find(|e| e.call == "core.start").unwrap();
    assert_eq!(
        (s.actor.as_str(), s.actor_kind),
        ("runner", ActorKind::Capability)
    );
    // fail records its actor too.
    let item2 = core.add_item(g, "j").unwrap();
    core.start(&runner, item2).unwrap();
    core.fail(&runner, item2, "broke").unwrap();
    let f = core
        .store()
        .evidence_for(item2)
        .unwrap()
        .into_iter()
        .find(|e| e.call == "core.fail")
        .unwrap();
    assert_eq!(f.actor, "runner");
}

#[test]
fn anyone_may_stop_only_the_owner_clears_it() {
    let core = core();
    core.stop(&actor_named("anyone")).unwrap();
    assert_eq!(core.store().control("stop").unwrap(), 1);
    core.clear_stop(&owner(&core)).unwrap();
    assert_eq!(core.store().control("stop").unwrap(), 0);
}

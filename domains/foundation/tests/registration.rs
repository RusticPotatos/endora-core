//! Slice 3 item 2: a capability's declaration is checked when it is registered. A duplicate
//! call name was silently shadowed; an empty name or an unclassified band was accepted.

use std::sync::Arc;

use endora_foundation::application::{CallFailure, CallOutput, Capability, Core};
use endora_foundation::domain::CallSpec;
use endora_foundation::infrastructure::ManualClock;
use endora_kernel::{Reversibility, Tier};

struct Decl(Vec<(&'static str, Option<Reversibility>)>);

impl Capability for Decl {
    fn offers(&self) -> Vec<CallSpec> {
        self.0
            .iter()
            .map(|(name, band)| CallSpec {
                name: (*name).into(),
                band: *band,
                input: String::new(),
                output: String::new(),
                tier: Tier::Internal,
                max_cost_micros: 0,
                max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
                max_turns: None,
                inputs: Vec::new(),
                facts: Vec::new(),
                budget: Default::default(),
            })
            .collect()
    }
    fn call(&self, _call: &str, _input: &str) -> Result<CallOutput, CallFailure> {
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: Vec::new(),
            result: "ok".into(),
            evidence: String::new(),
        })
    }
}

fn core() -> Core {
    Core::in_memory(Arc::new(ManualClock::new(1))).unwrap()
}

const OBS: Option<Reversibility> = Some(Reversibility::Observe);

fn says(r: Result<(), endora_kernel::AppError>) -> String {
    r.expect_err("must be rejected").to_string()
}

#[test]
fn a_capability_with_two_calls_of_one_name_is_rejected() {
    let mut c = core();
    let e = says(c.register(Arc::new(Decl(vec![("a.read", OBS), ("a.read", OBS)]))));
    assert!(e.contains("a.read") && e.contains("twice"), "{e}");
    assert!(
        c.spec("a.read").is_none(),
        "nothing of a rejected capability is registered"
    );
}

#[test]
fn two_capabilities_declaring_one_call_name_are_rejected_and_the_first_stays() {
    let mut c = core();
    c.register(Arc::new(Decl(vec![(
        "dup.call",
        Some(Reversibility::Irreversible),
    )])))
    .unwrap();
    let e = says(c.register(Arc::new(Decl(vec![("dup.call", OBS)]))));
    assert!(e.contains("dup.call") && e.contains("already"), "{e}");
    assert_eq!(
        c.spec("dup.call").unwrap().band,
        Some(Reversibility::Irreversible)
    );
}

#[test]
fn an_empty_name_is_rejected() {
    for name in ["", "   "] {
        let e = says(core().register(Arc::new(Decl(vec![(name, OBS)]))));
        assert!(e.contains("empty"), "{e}");
    }
}

#[test]
fn an_undeclared_band_is_rejected() {
    let e = says(core().register(Arc::new(Decl(vec![("x.unclassified", None)]))));
    assert!(e.contains("x.unclassified") && e.contains("band"), "{e}");
}

#[test]
fn a_valid_declaration_registers() {
    let mut c = core();
    c.register(Arc::new(Decl(vec![
        ("a.read", OBS),
        ("a.act", Some(Reversibility::Reversible)),
    ])))
    .unwrap();
    assert!(c.spec("a.read").is_some() && c.spec("a.act").is_some());
}

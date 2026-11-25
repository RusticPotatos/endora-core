//! The done test for advisory diagnosis (ADR 0153): a frozen set of 20 SYNTHETIC incidents is
//! replayed through the Foundation path (a failed watch item, its own evidence, the quote check
//! and the entailment check) with recorded replies standing in for the model, and at least 18 of
//! the 20 must end verified (a verified answer, or an explicit not-found), the rest refused, and
//! nothing unsupported accepted.
//!
//! What this measures: the verifier and the Foundation path. It does not measure a model: the
//! recorded replies are authored, not captured (no model runs in a test), so a live model's rate
//! is still unmeasured here. Every host in the set is a documentation address or `example.com`.

mod common;
use common::diagnosis_lab::{Replay, failed_watch};

use endora_foundation::domain::diagnosis::DiagnosisOutcome;
use serde_json::Value;

const SET: &str = include_str!("fixtures/diagnosis/incidents.json");

#[test]
fn the_frozen_synthetic_set_reaches_18_of_20_verified_and_refuses_the_rest() {
    let set: Value = serde_json::from_str(SET).unwrap();
    let cases = set["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    let (mut verified, mut refused) = (0, 0);
    let mut mismatches: Vec<(String, String, DiagnosisOutcome, Vec<String>)> = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let (core, item) = failed_watch(case["log"].as_str().unwrap());
        let doc = core
            .diagnosis_sources(item)
            .unwrap()
            .into_iter()
            .find(|(_, t)| t.contains(case["log"].as_str().unwrap().lines().next().unwrap()))
            .map(|(n, _)| n)
            .unwrap();
        let replies: Vec<String> = case["replies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().replace("DOC", &doc))
            .collect();
        let refs: Vec<&str> = replies.iter().map(String::as_str).collect();
        let model = Replay::new(&refs);
        let d = core.diagnose_item(item, &model).unwrap();
        let expect = case["expect"].as_str().unwrap();
        match (expect, d.outcome) {
            ("verified", DiagnosisOutcome::Verified) => {
                assert_eq!(d.answer.as_deref(), case["gold"].as_str(), "{id}");
                verified += 1;
            }
            ("abstain", DiagnosisOutcome::NotFound) => verified += 1,
            ("refused", DiagnosisOutcome::Refused) => {
                assert_eq!(d.answer, None, "{id}: a refused answer is not kept");
                refused += 1;
            }
            (want, got) => mismatches.push((id.to_owned(), want.to_owned(), got, d.faults.clone())),
        }
    }
    // A case the Foundation verified although the gold says it should not be is an unsupported accept.
    let unsupported = mismatches
        .iter()
        .filter(|m| m.2 == DiagnosisOutcome::Verified)
        .count();
    println!(
        "replay: {verified}/20 verified, {refused} refused, {unsupported} unsupported accepted"
    );
    assert!(mismatches.is_empty(), "{mismatches:?}");
    assert!(verified >= 18, "{verified}/20");
    assert_eq!(refused, 20 - verified);
    assert_eq!(unsupported, 0);
}

#[test]
fn the_set_holds_nothing_personal_or_real() {
    for banned in [
        "192.168.", "10.0.", "172.16.", ".local", ".lan", "/home/", "@gmail", "token=",
    ] {
        assert!(!SET.contains(banned), "{banned}");
    }
    // Every address is in a documentation range.
    for (i, _) in SET.match_indices("192.0.2.") {
        assert!(SET[i..].starts_with("192.0.2."));
    }
}

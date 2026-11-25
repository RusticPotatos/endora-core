//! `owner::command_text`: what an owner's command printed, whole up to a stated cap, with its
//! exit code. It exists for the adapter capability (ADR 0154), which must never parse a text that
//! was cut short or that a failed command left behind.

use endora_foundation_observe::owner;

#[test]
fn a_command_that_prints_less_than_the_cap_is_returned_whole_with_its_exit_code() {
    let p = owner::command_text("printf one\\ntwo\\n", &[], 100).unwrap();
    assert_eq!(p.code, Some(0));
    assert_eq!(p.text, "one\ntwo\n");
    assert!(!p.over);
}

#[test]
fn a_text_over_the_cap_is_flagged_not_silently_cut() {
    let p = owner::command_text("seq 1 1000", &[], 50).unwrap();
    assert!(p.over, "the cap was passed and says so");
    assert!(p.text.len() <= 50);
}

#[test]
fn a_nonzero_exit_is_reported_with_the_text() {
    let p = owner::command_text("false", &[], 100).unwrap();
    assert_eq!(p.code, Some(1));
}

#[test]
fn output_larger_than_a_pipe_does_not_hang_the_command() {
    // 200 KB is more than a pipe holds; the reader must keep draining while the command runs.
    let p = owner::command_text("seq 1 40000", &[], 1_000_000).unwrap();
    assert_eq!(p.code, Some(0));
    assert!(p.text.len() > 150_000 && !p.over, "{} bytes", p.text.len());
}

#[test]
fn a_command_that_cannot_start_is_a_failure() {
    assert!(owner::command_text("/nonexistent/x", &[], 100).is_err());
    assert!(owner::command_text("   ", &[], 100).is_err());
}

//! The cause of a failed turn, read from the provider's words, and the retries it allows.

use super::failure_cause::cause_of;
use crate::domain::FailureCause;
use crate::domain::runs::{retry_after_failure, retry_decision};

const NOW: i64 = 1_794_000_000_000;

#[test]
fn each_provider_answer_is_told_by_its_status_and_words() {
    let cases: [(&str, FailureCause); 13] = [
        // A bare 401 is the turn's own token refused, not the provider's key: retried as before.
        ("HQ answered 401", FailureCause::Other),
        ("the relay answered Claude Code 401", FailureCause::Other),
        (
            "Claude Code failed (success): API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}",
            FailureCause::Credential,
        ),
        (
            "the relay answered Claude Code 402",
            FailureCause::Quota { resets_at: None },
        ),
        (
            "API Error: 400 Your credit balance is too low to access the Anthropic API",
            FailureCause::Quota { resets_at: None },
        ),
        (
            "HTTP 429 insufficient_quota: you exceeded your current quota",
            FailureCause::Quota { resets_at: None },
        ),
        (
            "Claude AI usage limit reached|1794010800",
            FailureCause::Quota {
                resets_at: Some(1_794_010_800_000),
            },
        ),
        (
            "API Error: 429 {\"error\":{\"type\":\"rate_limit_error\",\"message\":\"slow down\"}}",
            FailureCause::RateLimit { retry_after: None },
        ),
        (
            "status 429 Too Many Requests; retry-after: 45",
            FailureCause::RateLimit {
                retry_after: Some(45_000),
            },
        ),
        (
            "API Error: 529 {\"type\":\"overloaded_error\"}",
            FailureCause::Server,
        ),
        ("the endpoint returned status 503", FailureCause::Server),
        (
            "Claude Code failed (error_during_execution): boom",
            FailureCause::Other,
        ),
        ("exited with code 1", FailureCause::Other),
    ];
    for (said, expected) in cases {
        assert_eq!(cause_of(said, NOW), expected, "{said}");
    }
}

#[test]
fn a_number_inside_a_longer_number_is_not_a_status() {
    for said in [
        "run 14290 failed",
        "turn 5030 ended (exit 1)",
        "took 4010ms",
    ] {
        assert_eq!(cause_of(said, NOW), FailureCause::Other, "{said}");
    }
}

#[test]
fn a_reset_time_is_taken_in_seconds_or_milliseconds_and_only_when_plausible() {
    let at = |said: &str| cause_of(said, NOW);
    let reopens = |resets_at| FailureCause::Quota {
        resets_at: Some(resets_at),
    };
    assert_eq!(
        at("usage limit reached|1794003600"),
        reopens(1_794_003_600_000)
    );
    assert_eq!(
        at("usage limit: {\"resets_at\": 1794003600000}"),
        reopens(1_794_003_600_000)
    );
    // Not a time: a clock reading with no epoch, or one a month away.
    assert_eq!(at("usage limit reached, resets at 3pm"), reopens_nothing());
    assert_eq!(at("usage limit reached|1799999999"), reopens_nothing());
    assert_eq!(at("usage limit reached|0"), reopens_nothing());
}

fn reopens_nothing() -> FailureCause {
    FailureCause::Quota { resets_at: None }
}

#[test]
fn a_rate_limit_asking_for_hours_is_a_window_not_a_pause() {
    assert_eq!(
        cause_of("429 too many requests, retry-after: 7200", NOW),
        FailureCause::Quota {
            resets_at: Some(NOW + 7_200_000)
        }
    );
}

// ---- Quota failures, replayed offline ------------------------------------------------------
//
// What a real quota failure looks like: a short description of the provider's refusal, with
// no reset time. So each `RECORDED` line is such a description, at a SYNTHETIC time (an
// arbitrary epoch base, with the same spacing a run of failures inside one window has: three
// half-hour ticks, then one after the window closed, then one a day later); the window's
// close is also SYNTHETIC, a stand-in 23_400 s after the first failure. Every fixture in
// `SYNTHETIC` is invented to cover the other causes.

/// (seconds since the epoch, the provider's phrase): three failures half an hour apart inside
/// one window, one after it closed, and one a day later. The times are synthetic.
const RECORDED: [(i64, &str); 5] = [
    (
        1_700_000_000,
        "failed at provider usage limit before prompt",
    ),
    (
        1_700_001_800,
        "failed at provider usage limit before prompt",
    ),
    (
        1_700_003_600,
        "failed at provider usage limit before prompt",
    ),
    (
        1_700_028_860,
        "failed at provider usage limit before executing its instructions",
    ),
    (1_700_219_840, "failed at the OpenAI limit before executing"),
];

/// Invented (SYNTHETIC) failures for the causes `RECORDED` has none of.
const SYNTHETIC: [(&str, FailureCause); 6] = [
    (
        "API Error: 401 authentication_error",
        FailureCause::Credential,
    ),
    (
        "API Error: 429 rate_limit_error",
        FailureCause::RateLimit { retry_after: None },
    ),
    ("API Error: 529 overloaded_error", FailureCause::Server),
    ("API Error: 500 api_error", FailureCause::Server),
    ("Claude Code failed (error_max_turns)", FailureCause::Other),
    ("exited with code 2", FailureCause::Other),
];

#[test]
fn replaying_the_recorded_quota_failures_retries_nothing_inside_a_closed_window() {
    // SYNTHETIC window: closed 23_400 s after the first failure, covering the first three.
    let resets_at_ms = 1_700_023_400_000;
    let mut inside_old = 0;
    let mut inside_new = 0;
    for (at, said) in &RECORDED[..3] {
        let failed = at * 1_000;
        // The same failure, now said with the window the provider would have given.
        let cause = cause_of(
            &format!("{said}; resets_at: {}", resets_at_ms / 1_000),
            failed,
        );
        assert_eq!(
            cause,
            FailureCause::Quota {
                resets_at: Some(resets_at_ms)
            }
        );
        for attempts in 1..=2 {
            let old = retry_after_failure(attempts, failed).unwrap();
            let new = retry_decision(cause, attempts, failed).unwrap();
            inside_old += i32::from(old < resets_at_ms);
            inside_new += i32::from(new < resets_at_ms);
        }
    }
    assert_eq!(inside_new, 0, "no retry inside the closed window");
    assert_eq!(inside_old, 6, "the fixed minute retried 6 times inside it");
}

#[test]
fn the_recorded_failures_all_get_a_cause_other_than_other() {
    let told: Vec<FailureCause> = RECORDED
        .iter()
        .map(|(at, said)| cause_of(said, at * 1_000))
        .collect();
    let named = told.iter().filter(|c| **c != FailureCause::Other).count();
    assert_eq!(
        named,
        RECORDED.len(),
        "5 of 5 recorded failures are a quota: {told:?}"
    );
    // With no window recorded, none is retried inside the conservative hold.
    for ((at, _), cause) in RECORDED.iter().zip(told) {
        let due = retry_decision(cause, 1, at * 1_000).unwrap();
        assert!(due >= at * 1_000 + crate::domain::runs::UNKNOWN_QUOTA_HOLD_MS);
    }
}

#[test]
fn the_synthetic_fixtures_are_told_as_named() {
    // SYNTHETIC: invented to cover the causes `RECORDED` has no failure of; two are `Other`
    // on purpose, so this is a check of each answer, not of a share.
    for (said, expected) in SYNTHETIC {
        assert_eq!(cause_of(said, NOW), expected, "{said}");
    }
}

//! Why a turn failed, read from the provider's words.
//!
//! A failed turn reaches the run logic as one line, the runner's `why`: the worker's exit and
//! its last words, which carry whatever the provider said (a status, an error type, sometimes a
//! reset time). The relay drops the provider's headers, so a `Retry-After` header never gets
//! this far; only what the text carries does. This reads that text into a
//! [`FailureCause`]; the decision about what to do is the domain's
//! ([`retry_decision`](crate::domain::runs::retry_decision)).

use crate::domain::FailureCause;
use crate::domain::work::Millis;

/// The furthest ahead a reset time is believed: a week, the longest plan window.
const FURTHEST_RESET_MS: Millis = 7 * 24 * 60 * 60_000;
/// The longest a throttle's `retry-after` is a pause; past it, it is a window.
const LONGEST_PAUSE_MS: Millis = 60 * 60_000;

/// The cause of a failure, from its words, `now` being when it was read. A quota is told before
/// a throttle, since some providers answer a spent quota with a 429.
#[must_use]
pub(crate) fn cause_of(said: &str, now: Millis) -> FailureCause {
    let said = said.to_ascii_lowercase();
    if is_credential(&said) {
        return FailureCause::Credential;
    }
    if is_quota(&said) {
        return FailureCause::Quota {
            resets_at: reset_time(&said, now),
        };
    }
    if is_rate_limit(&said) {
        return rate_limit(&said, now);
    }
    if is_server(&said) {
        return FailureCause::Server;
    }
    FailureCause::Other
}

/// A refused key, by the provider's own words. A bare 401 is not enough: the node's relay and
/// HQ answer 401 too when a turn's own token is no longer good (the turn is over), and a new
/// attempt gets a new token, so that is retried as it always was.
fn is_credential(said: &str) -> bool {
    contains_any(
        said,
        &[
            "authentication_error",
            "invalid x-api-key",
            "invalid api key",
            "incorrect api key",
            "api key was rejected",
            "permission_error",
        ],
    )
}

fn is_quota(said: &str) -> bool {
    has_status(said, 402)
        || contains_any(
            said,
            &[
                "insufficient_quota",
                "quota",
                "credit balance",
                "out of credits",
                "usage limit",
                "weekly limit",
                "daily limit",
                "hour limit",
                "plan limit",
                "spending limit",
                "openai limit",
            ],
        )
}

fn is_rate_limit(said: &str) -> bool {
    has_status(said, 429) || contains_any(said, &["rate_limit", "rate limit", "too many requests"])
}

fn is_server(said: &str) -> bool {
    [500, 502, 503, 504, 529]
        .iter()
        .any(|s| has_status(said, *s))
        || contains_any(
            said,
            &[
                "overloaded",
                "internal server error",
                "bad gateway",
                "service unavailable",
                "gateway timeout",
            ],
        )
}

/// A throttle: with a long `retry-after` it is a window, held like a quota's.
fn rate_limit(said: &str, now: Millis) -> FailureCause {
    let Some(seconds) = number_after(said, &["retry-after", "retry_after", "retry after"]) else {
        return FailureCause::RateLimit { retry_after: None };
    };
    let asked = i64::try_from(seconds)
        .unwrap_or(i64::MAX / 1_000)
        .saturating_mul(1_000);
    if asked > LONGEST_PAUSE_MS {
        return believed(now + asked, now).map_or(
            FailureCause::RateLimit { retry_after: None },
            |at| FailureCause::Quota {
                resets_at: Some(at),
            },
        );
    }
    FailureCause::RateLimit {
        retry_after: Some(asked),
    }
}

/// When a spent quota reopens, if the words give an epoch time in seconds or milliseconds.
fn reset_time(said: &str, now: Millis) -> Option<Millis> {
    let epoch = number_after(
        said,
        &[
            "limit reached|",
            "resets_at",
            "reset_at",
            "resets at",
            "resets_in_epoch",
        ],
    )?;
    let epoch = i64::try_from(epoch).ok()?;
    // Ten digits is seconds, thirteen is milliseconds; anything else is not an epoch time.
    let at = match epoch {
        1_000_000_000..=9_999_999_999 => epoch * 1_000,
        1_000_000_000_000..=9_999_999_999_999 => epoch,
        _ => return None,
    };
    believed(at, now)
}

/// `at` when it is no more than a week ahead.
fn believed(at: Millis, now: Millis) -> Option<Millis> {
    (at <= now + FURTHEST_RESET_MS).then_some(at)
}

/// The first number that follows any of `keys`, past separators (`"`, `:`, `=`, spaces, `|`).
fn number_after(said: &str, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let at = said.find(key)? + key.len();
        let rest = said[at..].trim_start_matches(|c: char| " \":=|".contains(c));
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// Whether `status` stands alone in the words: not inside a longer number.
fn has_status(said: &str, status: u16) -> bool {
    let wanted = status.to_string();
    said.match_indices(&wanted).any(|(at, _)| {
        let before = said[..at].chars().next_back();
        let after = said[at + wanted.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

fn contains_any(said: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| said.contains(n))
}

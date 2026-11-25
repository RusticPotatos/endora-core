//! Why a turn failed, read from the provider's answer and words.
//!
//! A failed turn reaches the run logic as one line, the runner's `why`: the worker's exit and
//! its last words, which carry whatever the provider said (a status, an error type, sometimes a
//! reset time). Beside it may come the provider's own answer, as the node's relay read its
//! headers (`Retry-After`, the rate-limit reset): when it names a time, that time is told
//! before the words. Without it (an older node, a provider that sent none) the text alone is
//! read, as before. This reads both into a [`FailureCause`]; the decision about what to do is
//! the domain's ([`retry_decision`](crate::domain::runs::retry_decision)).

use crate::domain::work::Millis;
use crate::domain::{FailureCause, ProviderAnswer};

/// The furthest ahead a reset time is believed: a week, the longest plan window.
const FURTHEST_RESET_MS: Millis = 7 * 24 * 60 * 60_000;
/// The longest a throttle's `retry-after` is a pause; past it, it is a window.
const LONGEST_PAUSE_MS: Millis = 60 * 60_000;

/// The cause of a failure, from the provider's answer if it named a time and from the words
/// for the rest, `now` being when it was read. A refused key is the words' alone: no answer
/// softens it.
#[must_use]
pub(crate) fn cause_of(said: &str, answer: Option<ProviderAnswer>, now: Millis) -> FailureCause {
    let cause = cause_of_words(said, now);
    let Some(answer) = answer else {
        return cause;
    };
    if cause == FailureCause::Credential {
        return cause;
    }
    told_by(answer, cause, now).unwrap_or(cause)
}

/// What the provider's headers say of a failure the words read as `cause`: a quota whose reset
/// they name, or a throttle whose wait they name. `None` when they name no usable time, which
/// leaves the words to say. Believed no further than a week ahead, as the words' are.
fn told_by(answer: ProviderAnswer, cause: FailureCause, now: Millis) -> Option<FailureCause> {
    if answer.status == 402 || matches!(cause, FailureCause::Quota { .. }) {
        let by_words = match cause {
            FailureCause::Quota { resets_at } => resets_at,
            _ => None,
        };
        let by_header = answer.resets_at.and_then(|at| believed(at, now));
        return Some(FailureCause::Quota {
            resets_at: by_header.or(by_words),
        });
    }
    if answer.status != 429 && !matches!(cause, FailureCause::RateLimit { .. }) {
        return None;
    }
    // The later of what it asked and when its limit reopens: waiting for the earlier would
    // only be refused again.
    let asked = [answer.retry_after, answer.resets_at.map(|at| at - now)]
        .into_iter()
        .flatten()
        .max()
        .filter(|wait| *wait > 0)?;
    Some(throttle(asked, now))
}

/// The cause of a failure, from its words, `now` being when it was read. A quota is told before
/// a throttle, since some providers answer a spent quota with a 429.
fn cause_of_words(said: &str, now: Millis) -> FailureCause {
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
    throttle(asked, now)
}

/// A throttle that asked to wait `asked` milliseconds: a pause, or, past an hour, a window.
fn throttle(asked: Millis, now: Millis) -> FailureCause {
    if asked > LONGEST_PAUSE_MS {
        return believed(now.saturating_add(asked), now).map_or(
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
    (at <= now.saturating_add(FURTHEST_RESET_MS)).then_some(at)
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

//! Owner notices (ADR 0152): the facts a watch's change tells the owner, and the rule for when to
//! tell them. A notice holds the routine's name, a state and a time, and nothing a probe printed.
//! Pure: no clock, file or socket; the caller passes the time.

use super::hold::RoutineId;

/// Consecutive failed checks a watch needs before it is called failing.
pub const FAILING_AFTER: u32 = 3;

/// How long a run of failures must have lasted before it is said, in milliseconds (10 minutes:
/// longer than a nightly reboot, as the house watch's grace is, ADR 0148).
pub const GRACE_MS: i64 = 600_000;

/// The longest routine name a notice line carries, in characters.
const MAX_NAME: usize = 80;

/// What a notice says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeState {
    /// The watch has failed for the grace, at least [`FAILING_AFTER`] times in a row.
    Failing,
    /// A watch that was said to be failing has passed its check again.
    Recovered,
}

impl NoticeState {
    /// The stored and printed word.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Failing => "failing",
            Self::Recovered => "recovered",
        }
    }

    /// Reads the stored word back.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "failing" => Some(Self::Failing),
            "recovered" => Some(Self::Recovered),
            _ => None,
        }
    }
}

/// One outbox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Row id (the owner acknowledges by it).
    pub id: i64,
    /// The idempotent id: one per routine, state and failing run. A sink that is asked twice for
    /// the same key says it once.
    pub key: String,
    /// The routine concerned.
    pub routine: RoutineId,
    /// Its name when the notice was made.
    pub routine_title: String,
    /// What happened.
    pub state: NoticeState,
    /// When, in Unix milliseconds.
    pub at_ms: i64,
    /// When a sink took it, once one has.
    pub delivered_ms: Option<i64>,
    /// How many deliveries failed so far.
    pub attempts: u32,
    /// When the owner acknowledged it, once they have.
    pub acked_ms: Option<i64>,
}

impl Notice {
    /// The one line a text sink writes: the key, the time, the state and the routine's name. The
    /// name is the owner's own text, made safe to print (no control characters, no quote, bounded).
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "key={} at={} state={} routine={} name=\"{}\"",
            self.key,
            utc(self.at_ms),
            self.state.name(),
            self.routine.0,
            printable(&self.routine_title)
        )
    }
}

/// The routine's name as one safe printed word group.
fn printable(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || c == '"' || c == '\\' {
                ' '
            } else {
                c
            }
        })
        .take(MAX_NAME)
        .collect()
}

/// The idempotent id of a notice: the routine, the state and when the failing run began.
#[must_use]
pub fn notice_key(routine: RoutineId, state: NoticeState, since_ms: i64) -> String {
    format!("r{}-{}-{since_ms}", routine.0, state.name())
}

/// Whether a failing run is to be said now: not said yet, at least [`FAILING_AFTER`] failures in
/// a row, and the run (first failure to `now_ms`) at least [`GRACE_MS`] old.
#[must_use]
pub const fn failing_due(consecutive: u32, since_ms: i64, now_ms: i64, announced: bool) -> bool {
    !announced && consecutive >= FAILING_AFTER && now_ms.saturating_sub(since_ms) >= GRACE_MS
}

/// Unix milliseconds as `YYYY-MM-DDTHH:MM:SSZ` (UTC), for people reading a notice.
#[must_use]
pub fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_is_right_at_the_epoch_a_leap_day_and_a_known_instant() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_795_580_199_001), "2026-11-25T04:16:39Z");
    }

    #[test]
    fn the_grace_and_the_count_both_have_to_be_reached() {
        assert!(!failing_due(2, 0, GRACE_MS, false));
        assert!(!failing_due(3, 0, GRACE_MS - 1, false));
        assert!(failing_due(3, 0, GRACE_MS, false));
        assert!(!failing_due(9, 0, GRACE_MS * 9, true));
    }
}

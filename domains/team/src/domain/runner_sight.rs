//! Whether a runner is asking HQ for work: the one rule behind the owner's "runner down"
//! warning. HQ never reaches out to a runner, so the only sign of one is its asking; when
//! every runner stops or loses the network, nothing on HQ moves and the owner blames the
//! model. The rule is pure: the runner's door notes the time of each signed request, and the
//! snapshot asks here what that time means now.

use super::work::Millis;

/// How long after its last request HQ still calls the runner up, in seconds. The runner
/// (`app/runner`) asks every 2 s, in the same loop that stops, checks and starts turns, and
/// each Docker call it waits on may take up to 30 s, so one slow step can pass without a
/// request. 90 s is three such waits: a runner that is merely busy is not reported down, and a
/// runner that is gone is reported within a minute and a half.
pub const STALE_AFTER_SECS: i64 = 90;

/// What HQ can say of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerState {
    /// It asked within [`STALE_AFTER_SECS`].
    Up,
    /// It asked before, and not within [`STALE_AFTER_SECS`].
    Stale,
    /// No request since HQ started. HQ keeps this in memory only, so a runner that polls
    /// within seconds of HQ starting leaves this state at once.
    Never,
}

impl RunnerState {
    /// The word on the wire.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Stale => "stale",
            Self::Never => "never",
        }
    }
}

/// The runner as HQ sees it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerSight {
    /// Whether it is asking.
    pub state: RunnerState,
    /// Whole seconds since its last request; `None` when there has been none.
    pub last_poll_secs: Option<i64>,
}

/// The runner as HQ sees it at `now`, given the time of its last signed request. A clock that
/// stepped back counts as a request just made, never as one from the future.
#[must_use]
pub fn runner_sight(last_seen: Option<Millis>, now: Millis) -> RunnerSight {
    let Some(seen) = last_seen else {
        return RunnerSight {
            state: RunnerState::Never,
            last_poll_secs: None,
        };
    };
    let secs = (now - seen).max(0) / 1_000;
    let state = if secs <= STALE_AFTER_SECS {
        RunnerState::Up
    } else {
        RunnerState::Stale
    };
    RunnerSight {
        state,
        last_poll_secs: Some(secs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Millis = 1_800_000_000_000;

    fn after(secs: i64) -> Millis {
        T0 + secs * 1_000
    }

    #[test]
    fn no_request_yet_is_never_with_no_age() {
        let sight = runner_sight(None, after(500));
        assert_eq!(sight.state, RunnerState::Never);
        assert_eq!(sight.last_poll_secs, None);
    }

    #[test]
    fn a_request_just_made_is_up() {
        let sight = runner_sight(Some(T0), after(2));
        assert_eq!(sight.state, RunnerState::Up);
        assert_eq!(sight.last_poll_secs, Some(2));
    }

    #[test]
    fn up_holds_through_the_threshold_and_stale_begins_after_it() {
        let at = |secs| runner_sight(Some(T0), after(secs)).state;
        assert_eq!(
            at(STALE_AFTER_SECS),
            RunnerState::Up,
            "exactly at the limit"
        );
        assert_eq!(at(STALE_AFTER_SECS + 1), RunnerState::Stale);
    }

    #[test]
    fn stale_says_how_long_the_runner_has_been_quiet() {
        let sight = runner_sight(Some(T0), after(600));
        assert_eq!(sight.state, RunnerState::Stale);
        assert_eq!(sight.last_poll_secs, Some(600));
    }

    #[test]
    fn a_clock_that_stepped_back_is_not_a_request_from_the_future() {
        let sight = runner_sight(Some(after(10)), T0);
        assert_eq!(sight.state, RunnerState::Up);
        assert_eq!(sight.last_poll_secs, Some(0));
    }

    #[test]
    fn the_wire_names_are_the_three_the_console_reads() {
        let names =
            [RunnerState::Up, RunnerState::Stale, RunnerState::Never].map(RunnerState::name);
        assert_eq!(names, ["up", "stale", "never"]);
    }
}

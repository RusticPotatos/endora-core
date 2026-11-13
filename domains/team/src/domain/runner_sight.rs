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

/// The name a runner is listed under when its request says none: an older runner, or one
/// whose name was nothing but characters a name may not hold.
pub const UNNAMED: &str = "unnamed";

/// The most characters of a runner's name HQ keeps.
pub const MAX_NAME_LEN: usize = 32;

/// The most runners HQ lists. A runner's name is untrusted input from a signed caller, so the
/// map that holds them is bounded; the house has two runners.
pub const MAX_RUNNERS: usize = 16;

/// A runner's name as HQ keeps it: ASCII letters, digits, `.`, `_` and `-` of what it said, no
/// more than [`MAX_NAME_LEN`] of them, and [`UNNAMED`] when none is left. The one place a
/// name is made safe to list and to show, whatever the caller sent.
#[must_use]
pub fn clean_name(raw: &str) -> String {
    let name: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(MAX_NAME_LEN)
        .collect();
    if name.is_empty() {
        return UNNAMED.to_owned();
    }
    name
}

/// One runner, listed by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSight {
    /// The name it says, as [`clean_name`] made it.
    pub name: String,
    /// What HQ can say of it now.
    pub sight: RunnerSight,
}

/// The time of each named runner's last signed request, bounded to [`MAX_RUNNERS`] names. HQ
/// keeps it in memory only, and never forgets a name short of being bounded out or restarted:
/// a runner stopped on purpose stays listed as stale until HQ restarts.
#[derive(Debug, Default)]
pub struct Sightings {
    last: Vec<(String, Millis)>,
}

impl Sightings {
    /// Notes that the runner called `raw_name` made a signed request at `at`. Never goes back in
    /// time. A new name, with [`MAX_RUNNERS`] already kept, takes the place of the one quiet
    /// longest.
    pub fn note(&mut self, raw_name: &str, at: Millis) {
        let name = clean_name(raw_name);
        if let Some((_, seen)) = self.last.iter_mut().find(|(n, _)| *n == name) {
            *seen = (*seen).max(at);
            return;
        }
        if self.last.len() >= MAX_RUNNERS {
            let oldest = self
                .last
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, seen))| *seen)
                .map(|(i, _)| i);
            if let Some(oldest) = oldest {
                self.last.swap_remove(oldest);
            }
        }
        self.last.push((name, at));
    }

    /// Every runner HQ has heard from, as it is at `now`: the quietest first, ties by name.
    #[must_use]
    pub fn all(&self, now: Millis) -> Vec<NamedSight> {
        let mut all: Vec<(Millis, NamedSight)> = self
            .last
            .iter()
            .map(|(name, seen)| {
                (
                    *seen,
                    NamedSight {
                        name: name.clone(),
                        sight: runner_sight(Some(*seen), now),
                    },
                )
            })
            .collect();
        all.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.name.cmp(&y.name)));
        all.into_iter().map(|(_, sight)| sight).collect()
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

    // ---- Which runner (ADR 0142 follow-up): the name each runner says, kept per name. ----

    #[test]
    fn a_name_keeps_letters_digits_dot_underscore_and_hyphen_and_at_most_32_characters() {
        assert_eq!(clean_name("runner-a"), "runner-a");
        assert_eq!(clean_name("Pi_5.lan"), "Pi_5.lan");
        assert_eq!(clean_name("run ner/a;b%20\r\n"), "runnerab20");
        assert_eq!(clean_name(&"x".repeat(80)), "x".repeat(MAX_NAME_LEN));
        assert_eq!(clean_name("naïve-é"), "nave-");
    }

    #[test]
    fn a_name_with_nothing_left_is_unnamed() {
        for raw in ["", "   ", "/;%", "é"] {
            assert_eq!(clean_name(raw), UNNAMED, "{raw:?}");
        }
    }

    #[test]
    fn each_name_has_its_own_last_request_and_the_quietest_is_listed_first() {
        let mut sightings = Sightings::default();
        sightings.note("runner-a", T0);
        sightings.note("runner-b", after(100));
        sightings.note("runner-a", after(10));
        let all = sightings.all(after(120));
        let seen: Vec<(&str, RunnerState, Option<i64>)> = all
            .iter()
            .map(|n| (n.name.as_str(), n.sight.state, n.sight.last_poll_secs))
            .collect();
        assert_eq!(
            seen,
            [
                ("runner-a", RunnerState::Stale, Some(110)),
                ("runner-b", RunnerState::Up, Some(20)),
            ],
            "one stale beside one up; the quietest first"
        );
    }

    #[test]
    fn a_later_request_never_moves_a_name_back_in_time() {
        let mut sightings = Sightings::default();
        sightings.note("runner-a", after(50));
        sightings.note("runner-a", after(5));
        assert_eq!(sightings.all(after(60))[0].sight.last_poll_secs, Some(10));
    }

    #[test]
    fn a_name_is_cleaned_where_it_is_noted() {
        let mut sightings = Sightings::default();
        sightings.note("Runner A!", T0);
        sightings.note("RunnerA", after(1));
        sightings.note("", after(2));
        let names: Vec<String> = sightings
            .all(after(3))
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"RunnerA".to_owned()));
        assert!(names.contains(&UNNAMED.to_owned()));
    }

    #[test]
    fn at_most_sixteen_names_are_kept_and_the_oldest_is_dropped_for_a_new_one() {
        let mut sightings = Sightings::default();
        for i in 0..MAX_RUNNERS {
            sightings.note(&format!("r{i:02}"), after(i64::try_from(i).unwrap()));
        }
        assert_eq!(sightings.all(after(20)).len(), MAX_RUNNERS);
        // A name already kept never evicts another.
        sightings.note("r03", after(30));
        assert_eq!(sightings.all(after(31)).len(), MAX_RUNNERS);
        assert!(sightings.all(after(31)).iter().any(|n| n.name == "r00"));
        // A seventeenth drops the one quiet longest: r00.
        sightings.note("newcomer", after(40));
        let all = sightings.all(after(41));
        assert_eq!(all.len(), MAX_RUNNERS);
        assert!(all.iter().all(|n| n.name != "r00"));
        assert!(all.iter().any(|n| n.name == "newcomer"));
    }

    #[test]
    fn nothing_noted_lists_nothing() {
        assert!(Sightings::default().all(T0).is_empty());
    }
}

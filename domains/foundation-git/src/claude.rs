//! The first REAL executor: the Claude Code command line (`claude -p`), run as the agent of
//! `git.change`. This module only builds the command line from the budget Core granted and reads
//! the CLI's own report back; the confinement, process-group kill and evidence live in the rest
//! of the crate.
//!
//! What the CLI enforces itself (version 2.1.322, see `docs/foundation/README.md`):
//! - `--max-budget-usd`: stops the run when the spend reaches the cap. The check happens between
//!   steps, so a run can end ABOVE the cap by up to one step; Core records that as a violation.
//! - `--max-turns` (hidden from `--help`, present in the binary): stops after that many turns.
//! - wall-clock: not a CLI flag; the capability kills the whole process group at the deadline.

use std::path::PathBuf;
use std::time::Duration;

use endora_foundation::domain::Budget;

/// The most micro-USD per call this executor claims to enforce. The flag takes any amount; this
/// is the ceiling the owner allowed for one real run (0.50 USD), so a declared maximum above it
/// is refused by Core as unenforceable. The CLI checks between steps, so a run can end one step
/// above its cap: `Enforced` means "stops at, or one step above, the grant".
pub const MOST_ENFORCEABLE_MICROS: u64 = 500_000;

/// The fixed system prompt: it names the task and its limits. The per-job instruction follows as
/// the user message.
pub const SYSTEM_PROMPT: &str = "You are a coding agent working in the current directory, a small \
git repository. Your task: make the repository's own tests pass by fixing the code under test. \
Run the tests with `sh test.sh`. Never edit the tests or anything under tests/ or scripts/. Only \
touch files in this directory. You have file tools and a restricted shell, no network tools. \
Work in as few steps as you can and finish with one short sentence.";

/// The Claude Code command line as an executor.
#[derive(Debug, Clone)]
pub struct ClaudeCode {
    /// The `claude` binary (an absolute path; the sandbox lets the child read exactly this file).
    pub binary: PathBuf,
    /// The exact model id.
    pub model: String,
    /// Extra command-line words, added last before the prompt (for example more allowed tools).
    pub extra_args: Vec<String>,
}

/// What runs the agent of `git.change`.
#[derive(Debug, Clone, Default)]
pub enum AgentExecutor {
    /// The configured command line (a scripted stand-in in tests; no budget).
    #[default]
    Command,
    /// The Claude Code CLI under the `real-agent` confinement profile.
    ClaudeCode(ClaudeCode),
}

/// The per-call ceilings the executor adds to what Core grants (Core's `Budget` carries a fixed
/// time and no turns today; see the slice 6 report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeLimits {
    /// The declared maximum cost of one `git.change`, in micro-USD.
    pub max_micros: u64,
    /// Most turns when Core grants none.
    pub max_turns: u32,
}

impl Default for ClaudeLimits {
    fn default() -> Self {
        Self {
            max_micros: 300_000,
            max_turns: 8,
        }
    }
}

/// What one run is actually limited to: the smaller of Core's grant and the executor's ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Granted {
    /// Micro-USD.
    pub micros: u64,
    /// Turns.
    pub turns: u32,
    /// Seconds.
    pub seconds: u64,
}

/// The limits for one run.
#[must_use]
pub fn granted(b: &Budget, limits: ClaudeLimits, wall: Duration) -> Granted {
    Granted {
        micros: b.max_cost_micros.min(limits.max_micros),
        turns: b
            .max_turns
            .map_or(limits.max_turns, |t| t.min(limits.max_turns)),
        seconds: b.max_seconds.min(wall.as_secs()),
    }
}

/// Dollars with six decimals (exact), from micro-USD.
#[must_use]
pub fn usd(micros: u64) -> String {
    format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
}

/// The tools the agent may use without asking; everything else is denied (`dontAsk`).
const ALLOWED: &str = "Read,Edit,Write,Glob,Grep,Bash(sh test.sh),Bash(sh ./test.sh),Bash(cat *),Bash(ls *),Bash(git diff *),Bash(git status *)";

/// The command line (program first) for one run. No secret is ever in it.
#[must_use]
pub fn claude_args(c: &ClaudeCode, g: &Granted, task: &str) -> Vec<String> {
    let mut a: Vec<String> = vec![
        c.binary.display().to_string(),
        "--print".into(),
        "--output-format".into(),
        "json".into(),
        "--model".into(),
        c.model.clone(),
        "--max-budget-usd".into(),
        usd(g.micros),
        "--max-turns".into(),
        g.turns.to_string(),
        "--system-prompt".into(),
        SYSTEM_PROMPT.into(),
        // No user, project or local settings, no hooks, no plugins, no skills, no MCP servers.
        "--restricted".into(),
        "--safe-mode".into(),
        "--strict-mcp-config".into(),
        "--disable-slash-commands".into(),
        "--no-session-persistence".into(),
        // Only these built-in tools exist: no web tools, no task or notebook tools.
        "--tools".into(),
        "Read,Edit,Write,Glob,Grep,Bash".into(),
        "--permission-mode".into(),
        "dontAsk".into(),
        // Nobody answers a prompt: anything not allowed below is denied. (The scrubbed
        // environment forces the default mode, so this flag is what makes it unattended.)
        "--permission-prompts".into(),
        "none".into(),
        "--allowed-tools".into(),
        ALLOWED.into(),
    ];
    a.extend(c.extra_args.iter().cloned());
    // `--` so the prompt is never swallowed by a list-valued option.
    a.push("--".into());
    a.push(if task.trim().is_empty() {
        "Make the repository's tests pass.".to_owned()
    } else {
        task.to_owned()
    });
    a
}

/// Why the CLI stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// It finished its work.
    Finished,
    /// It hit `--max-turns`.
    MaxTurns,
    /// It hit `--max-budget-usd`.
    MaxBudget,
    /// Any other error.
    Error,
}

impl Stop {
    /// The fixed word recorded as the `agent_stop` fact.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::MaxTurns => "max_turns",
            Self::MaxBudget => "max_budget",
            Self::Error => "error",
        }
    }
}

/// What the CLI reported about its own run.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The CLI-reported total cost in micro-USD, rounded UP.
    pub cost_micros: u64,
    /// The CLI-reported cost in dollars, as a number.
    pub cost_usd: f64,
    /// Turns the CLI says it took.
    pub turns: Option<u32>,
    /// Why it stopped.
    pub stop: Stop,
    /// Its final text.
    pub text: String,
}

/// Micro-USD, rounded up, from dollars (the tiny subtraction absorbs binary-float noise: 0.30 is
/// 300,000 micro-USD, not 300,001).
#[must_use]
pub fn micros_up(usd: f64) -> u64 {
    if !usd.is_finite() || usd <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (usd * 1_000_000.0 - 1e-3).ceil().max(0.0) as u64
    }
}

/// Reads the CLI's `--output-format json` output: one result object, or an array of messages
/// with the result object last.
///
/// # Errors
/// When no result object with a cost can be found.
pub fn parse_report(stdout: &str) -> Result<Report, String> {
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("not JSON: {e}"))?;
    let obj = match &v {
        serde_json::Value::Array(items) => items
            .iter()
            .rev()
            .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("result")),
        o if o.get("type").and_then(|t| t.as_str()) == Some("result") => Some(o),
        _ => None,
    }
    .ok_or("no result object")?;
    let cost_usd = obj
        .get("total_cost_usd")
        .and_then(serde_json::Value::as_f64)
        .ok_or("the result has no total_cost_usd")?;
    let subtype = obj.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
    let is_error = obj
        .get("is_error")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stop = match (subtype, is_error) {
        ("error_max_turns", _) => Stop::MaxTurns,
        ("error_max_budget_usd", _) => Stop::MaxBudget,
        ("success", false) => Stop::Finished,
        _ => Stop::Error,
    };
    Ok(Report {
        cost_micros: micros_up(cost_usd),
        cost_usd,
        turns: obj
            .get("num_turns")
            .and_then(serde_json::Value::as_u64)
            .and_then(|t| u32::try_from(t).ok()),
        stop,
        text: obj
            .get("result")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc() -> ClaudeCode {
        ClaudeCode {
            binary: "/opt/claude".into(),
            model: "claude-haiku-5-5".into(),
            extra_args: vec!["--effort".into(), "low".into()],
        }
    }

    #[test]
    fn the_command_line_carries_the_granted_limits_and_closes_every_door() {
        let g = Granted {
            micros: 300_000,
            turns: 8,
            seconds: 180,
        };
        let a = claude_args(&cc(), &g, "fix add");
        let pair = |x: &str, y: &str| a.windows(2).any(|w| w[0] == x && w[1] == y);
        assert!(pair("--max-budget-usd", "0.300000") && pair("--max-turns", "8"));
        assert!(pair("--model", "claude-haiku-5-5") && pair("--output-format", "json"));
        assert!(pair("--permission-mode", "dontAsk"));
        for flag in [
            "--print",
            "--restricted",
            "--safe-mode",
            "--strict-mcp-config",
        ] {
            assert!(a.iter().any(|x| x == flag), "{flag}");
        }
        let tools = a.iter().position(|x| x == "--tools").map(|i| &a[i + 1]);
        let tools = tools.expect("a tool list");
        for banned in ["Web", "Task", "Notebook", "Mcp"] {
            assert!(!tools.contains(banned), "{banned}");
        }
        // The extra words come before the `--` and the prompt is last.
        let dd = a.iter().position(|x| x == "--").unwrap();
        assert_eq!(a[dd + 1], "fix add");
        assert!(a.iter().position(|x| x == "--effort").unwrap() < dd);
        assert_eq!(a.len(), dd + 2);
        assert!(!a.join(" ").to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn the_run_gets_the_smaller_of_the_grant_and_the_ceiling() {
        let l = ClaudeLimits {
            max_micros: 300_000,
            max_turns: 8,
        };
        let b = |c, t, s| Budget {
            max_cost_micros: c,
            max_seconds: s,
            max_turns: t,
        };
        let w = Duration::from_secs(180);
        let g = granted(&b(300_000, None, 600), l, w);
        assert_eq!((g.micros, g.turns, g.seconds), (300_000, 8, 180));
        let g = granted(&b(50_000, Some(2), 60), l, w);
        assert_eq!((g.micros, g.turns, g.seconds), (50_000, 2, 60));
        let g = granted(&b(900_000, Some(50), 600), l, w);
        assert_eq!((g.micros, g.turns), (300_000, 8));
    }

    #[test]
    fn dollars_become_micro_usd_rounded_up() {
        assert_eq!(micros_up(0.30), 300_000);
        assert_eq!(micros_up(0.0001), 100);
        assert_eq!(micros_up(0.0015103), 1_511);
        assert_eq!(micros_up(0.3000001), 300_001);
        assert_eq!(micros_up(0.0), 0);
        assert_eq!(micros_up(-1.0), 0);
        assert_eq!(micros_up(f64::NAN), 0);
        assert_eq!(usd(50_000), "0.050000");
        assert_eq!(usd(1_300_000), "1.300000");
        assert_eq!(usd(1_511), "0.001511");
    }

    #[test]
    fn the_report_is_read_from_the_clis_json() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":3,"total_cost_usd":0.0412,"result":"done"}"#;
        let r = parse_report(ok).unwrap();
        assert_eq!(
            (r.cost_micros, r.turns, r.stop),
            (41_200, Some(3), Stop::Finished)
        );
        let turns = r#"[{"type":"system"},{"type":"result","subtype":"error_max_turns","is_error":false,"num_turns":1,"total_cost_usd":0.01}]"#;
        assert_eq!(parse_report(turns).unwrap().stop, Stop::MaxTurns);
        let budget = r#"{"type":"result","subtype":"error_max_budget_usd","total_cost_usd":0.02}"#;
        assert_eq!(parse_report(budget).unwrap().stop, Stop::MaxBudget);
        let bad = r#"{"type":"result","subtype":"success","is_error":true,"total_cost_usd":0.0}"#;
        assert_eq!(parse_report(bad).unwrap().stop, Stop::Error);
        assert!(parse_report("garbage").is_err());
        assert!(parse_report(r#"{"type":"result"}"#).is_err());
        assert!(parse_report(r#"{"type":"other","total_cost_usd":1}"#).is_err());
    }
}

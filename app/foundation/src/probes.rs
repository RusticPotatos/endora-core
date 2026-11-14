//! The configured probes as one read-only capability, `probe.run <name>`. A probe is something
//! the owner wrote in the config file (a tcp port, an http path, a command line), never something
//! an agent proposes. Its facts come from what this code observes: whether the connection opened,
//! the status line it parsed, the exit code. Text the remote or the command printed stays in the
//! result for people to read and is never a fact.

use endora_foundation::application::{CallFailure, CallOutput, Capability};
use endora_foundation::domain::{BudgetMode, CallSpec, Field};
use endora_foundation_observe::owner;
use endora_kernel::{Reversibility, Tier};

/// How a probe looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tcp,
    Http,
    Command,
}

/// One configured probe.
#[derive(Debug, Clone)]
pub struct Probe {
    pub name: String,
    pub kind: Kind,
    pub target: String,
    pub interval_secs: i64,
}

/// The capability.
/// The probes and the environment variable names a command probe may inherit (default none).
pub struct Probes(pub Vec<Probe>, pub Vec<String>);

impl Capability for Probes {
    fn offers(&self) -> Vec<CallSpec> {
        if self.0.is_empty() {
            return Vec::new();
        }
        vec![CallSpec {
            name: "probe.run".into(),
            band: Some(Reversibility::Observe),
            input: "the name of a probe in the config file".into(),
            output: "facts: up (tcp, http), status (http), exit (command)".into(),
            tier: Tier::Internal,
            max_cost_micros: 0,
            max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
            max_turns: None,
            inputs: vec![Field::text("probe")],
            facts: vec![Field::bool("up"), Field::int("status"), Field::int("exit")],
            budget: BudgetMode::Unenforced,
        }]
    }

    fn call(&self, _call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        let p = self
            .0
            .iter()
            .find(|p| p.name == input)
            .ok_or_else(|| CallFailure::new(format!("`{input}` is not a configured probe")))?;
        match p.kind {
            Kind::Tcp => Ok(owner::tcp(&p.target)),
            Kind::Http => Ok(owner::http(&p.target)),
            Kind::Command => owner::command_with_env(&p.target, &self.1),
        }
    }
}

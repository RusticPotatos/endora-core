//! Capability A for the foundation: **observe and verify**. Read-only probes (a stub HTTP check
//! against a loopback service, and a declared script in a sandbox directory) plus one capped
//! reversible action (restart a service the sandbox owns). It lives outside the domain crate:
//! it implements the one port and is registered with `Core::register`; Core never names it.
//!
//! `service.wipe` is declared irreversible: a dummy target so an adversarial proposer has
//! something real to be refused, and so the refusal can be shown to have no effect.

pub mod owner;
pub mod scrub;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use endora_foundation::application::{CallFailure, CallOutput, Capability};
use endora_foundation::domain::{BudgetMode, CallSpec, Fact, Field};
use endora_kernel::{Reversibility, Tier};

/// Most bytes of a probe's output kept.
const MOST_OUTPUT: usize = 2048;

/// How long a probe or a script may take.
const PATIENCE: Duration = Duration::from_secs(10);

/// What the capability may touch.
#[derive(Debug, Clone)]
pub struct Config {
    /// The only directory scripts run in.
    pub sandbox: PathBuf,
    /// Declared read-only probes: `(name, script file in the sandbox)`.
    pub probes: Vec<(String, String)>,
    /// The restart script's file name in the sandbox.
    pub restart_script: String,
}

/// The capability.
pub struct ObserveVerify {
    config: Config,
    env_allow: Vec<String>,
}

impl ObserveVerify {
    /// A capability confined to `config`.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            env_allow: Vec::new(),
        }
    }

    /// Names of environment variables the scripts may inherit; every other variable is cleared
    /// (see [`scrub`]). The default is none.
    #[must_use]
    pub fn with_env_allow(mut self, names: Vec<String>) -> Self {
        self.env_allow = names;
        self
    }

    fn run_script(&self, file: &str) -> Result<(String, Option<i32>), CallFailure> {
        if file.contains('/') || file.contains("..") {
            return Err(CallFailure::new(format!(
                "`{file}` is not a plain file name"
            )));
        }
        let mut cmd = Command::new("sh");
        cmd.arg(self.config.sandbox.join(file))
            .current_dir(&self.config.sandbox);
        scrub::apply(&mut cmd, &self.config.sandbox, &self.env_allow);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CallFailure::new(e.to_string()))?;
        let started = Instant::now();
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) if started.elapsed() > PATIENCE => {
                    let _ = child.kill();
                    return Err(CallFailure::new("the script took too long"));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(CallFailure::new(e.to_string())),
            }
        };
        let mut text = String::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut text);
        }
        text.truncate(MOST_OUTPUT);
        Ok((text.trim().to_owned(), code))
    }

    fn probe_http(input: &str) -> Result<CallOutput, CallFailure> {
        let (host_port, path) = input.split_once('/').map_or((input, ""), |(h, p)| (h, p));
        let addr: SocketAddr = host_port
            .to_socket_addrs()
            .map_err(|e| CallFailure::new(format!("bad address: {e}")))?
            .next()
            .ok_or_else(|| CallFailure::new("no address"))?;
        if !addr.ip().is_loopback() {
            return Err(CallFailure::new("only loopback addresses may be probed"));
        }
        let mut conn = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
            .map_err(|e| CallFailure::new(format!("connect: {e}")))?;
        conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
        write!(
            conn,
            "GET /{path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n"
        )
        .map_err(|e| CallFailure::new(e.to_string()))?;
        let mut raw = Vec::new();
        let _ = conn.take(MOST_OUTPUT as u64).read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw);
        let status = text.split_whitespace().nth(1).unwrap_or("none");
        // The status line is read by this code; the body is the remote's text and is not a fact.
        let facts = status
            .parse::<i64>()
            .map(|n| vec![Fact::int("status", n)])
            .unwrap_or_default();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts,
            result: format!("status={status} body={body}"),
            evidence: format!("GET {addr}/{path}"),
        })
    }
}

fn spec(name: &str, band: Reversibility, input: &str, output: &str) -> CallSpec {
    CallSpec {
        name: name.into(),
        band: Some(band),
        input: input.into(),
        output: output.into(),
        tier: Tier::Internal,
        max_cost_micros: 0,
        max_seconds: endora_foundation::domain::DEFAULT_MAX_SECONDS,
        max_turns: None,
        inputs: Vec::new(),
        facts: Vec::new(),
        budget: BudgetMode::Unenforced,
    }
}

impl Capability for ObserveVerify {
    fn offers(&self) -> Vec<CallSpec> {
        vec![
            spec(
                "probe.http",
                Reversibility::Observe,
                "loopback host:port/path",
                "status=<code> body=<text>",
            )
            .with_inputs(vec![Field::text("target")])
            .with_facts(vec![Field::int("status")]),
            spec(
                "probe.command",
                Reversibility::Observe,
                "the name of a declared probe",
                "the probe's stdout",
            )
            .with_inputs(vec![Field::text("probe")])
            .with_facts(vec![Field::int("exit")]),
            spec(
                "service.restart",
                Reversibility::Reversible,
                "`restart`",
                "the restart script's stdout",
            )
            .with_inputs(vec![Field::text("action")])
            .with_facts(vec![Field::int("exit")]),
            spec(
                "service.wipe",
                Reversibility::Irreversible,
                "a file name in the sandbox",
                "nothing: deletes it",
            )
            .with_inputs(vec![Field::text("file")]),
        ]
    }

    fn call(&self, call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        match call {
            "probe.http" => Self::probe_http(input),
            "probe.command" => {
                let (_, file) = self
                    .config
                    .probes
                    .iter()
                    .find(|(n, _)| n == input)
                    .ok_or_else(|| {
                        CallFailure::new(format!("`{input}` is not a declared probe"))
                    })?;
                let (result, code) = self.run_script(file)?;
                Ok(CallOutput {
                    cost_micros: 0,
                    profile: None,
                    facts: code
                        .map(|c| Fact::int("exit", i64::from(c)))
                        .into_iter()
                        .collect(),
                    result,
                    evidence: format!("ran probe `{input}` in the sandbox"),
                })
            }
            "service.restart" => {
                let (result, code) = self.run_script(&self.config.restart_script.clone())?;
                Ok(CallOutput {
                    cost_micros: 0,
                    profile: None,
                    facts: code
                        .map(|c| Fact::int("exit", i64::from(c)))
                        .into_iter()
                        .collect(),
                    result,
                    evidence: "ran the restart script".into(),
                })
            }
            "service.wipe" => {
                if input.contains('/') || input.contains("..") {
                    return Err(CallFailure::new("not a plain file name"));
                }
                std::fs::remove_file(self.config.sandbox.join(input))
                    .map_err(|e| CallFailure::new(e.to_string()))?;
                Ok(CallOutput {
                    cost_micros: 0,
                    profile: None,
                    facts: Vec::new(),
                    result: "wiped".into(),
                    evidence: format!("removed {input}"),
                })
            }
            other => Err(CallFailure::new(format!("`{other}` is not offered"))),
        }
    }
}

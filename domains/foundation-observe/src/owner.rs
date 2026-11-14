//! Probes the OWNER wrote in a config file (a tcp port, an http path, a command line): never
//! something an agent proposes, so unlike `probe.http` they are not limited to loopback or to a
//! sandbox. Facts come from what this code observes (the connection opened, the status line it
//! parsed, the exit code); text the remote or the command printed stays in the result for people
//! and is never a fact. One implementation, used by the command line.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use endora_foundation::application::{CallFailure, CallOutput};
use endora_foundation::domain::Fact;

const OWNER_PATIENCE: Duration = Duration::from_secs(10);
const OWNER_MOST_TEXT: usize = 2_000;

fn connect(target: &str) -> Result<TcpStream, String> {
    let addr = target
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("no address")?;
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| e.to_string())
}

/// An http probe: `up` and the parsed `status` of `GET /path` on `host:port/path`.
#[must_use]
pub fn http(target: &str) -> CallOutput {
    let (host, path) = target.split_once('/').unwrap_or((target, ""));
    let text;
    let mut facts = vec![Fact::bool("up", false)];
    match connect(host) {
        Ok(mut conn) => {
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let sent = write!(
                conn,
                "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
            );
            let mut raw = Vec::new();
            if sent.is_ok() {
                let _ = conn.take(OWNER_MOST_TEXT as u64).read_to_end(&mut raw);
            }
            text = String::from_utf8_lossy(&raw).into_owned();
            if let Some(n) = text
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<i64>().ok())
            {
                facts = vec![Fact::bool("up", true), Fact::int("status", n)];
            }
        }
        Err(e) => text = e,
    }
    CallOutput {
        cost_micros: 0,
        profile: None,
        facts,
        result: format!("http {target}"),
        evidence: text,
    }
}

/// A command probe: its `exit` code (the command line is the owner's, run unconfined).
///
/// # Errors
/// [`CallFailure`] when it cannot start or takes too long.
pub fn command(target: &str) -> Result<CallOutput, CallFailure> {
    command_with_env(target, &[])
}

/// Like [`command`], with the named environment variables passed through (all others cleared).
///
/// # Errors
/// [`CallFailure`] when it cannot start or takes too long.
pub fn command_with_env(target: &str, env_allow: &[String]) -> Result<CallOutput, CallFailure> {
    let mut words = target.split_whitespace();
    let program = words
        .next()
        .ok_or_else(|| CallFailure::new("empty command"))?;
    let mut cmd = Command::new(program);
    cmd.args(words);
    // The owner's command line, but not the owner's secrets: a minimal environment (the working
    // directory stays where the process is, HOME is a path that holds nothing).
    crate::scrub::apply(&mut cmd, std::path::Path::new("/var/empty"), env_allow);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| CallFailure::new(e.to_string()))?;
    let started = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if started.elapsed() > OWNER_PATIENCE => {
                let _ = child.kill();
                return Err(CallFailure::new("the command took too long"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(CallFailure::new(e.to_string())),
        }
    };
    let mut out = String::new();
    if let Some(s) = child.stdout.take() {
        let _ = s.take(OWNER_MOST_TEXT as u64).read_to_string(&mut out);
    }
    Ok(CallOutput {
        cost_micros: 0,
        profile: None,
        facts: code
            .map(|c| Fact::int("exit", i64::from(c)))
            .into_iter()
            .collect(),
        result: format!("command {program}"),
        evidence: out.trim().to_owned(),
    })
}

/// A tcp probe: whether a connection to `target` (`host:port`) opens.
#[must_use]
pub fn tcp(target: &str) -> CallOutput {
    let r = connect(target);
    CallOutput {
        cost_micros: 0,
        profile: None,
        facts: vec![Fact::bool("up", r.is_ok())],
        result: format!("tcp {target} up={}", r.is_ok()),
        evidence: r.err().unwrap_or_default(),
    }
}

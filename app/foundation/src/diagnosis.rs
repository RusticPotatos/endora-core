//! The local model for advisory diagnosis (ADR 0153): one blocking HTTP call to an Ollama
//! `/api/generate` endpoint, over a plain TCP socket, to a LOOPBACK address only. It is built
//! but OFF: nothing constructs it unless the owner's config has a `diagnosis-model` line, and
//! the line refuses any host that is not this machine. No TLS and no new dependency: a loopback
//! socket needs neither. The reply is text for `Core::diagnose_item` to check; this file decides
//! nothing.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use endora_foundation::application::DiagnosisModel;

/// The longest reply read (bytes): a runaway server cannot fill memory.
const MAX_REPLY: usize = 1_000_000;
/// How long one call may take.
const TIMEOUT: Duration = Duration::from_secs(120);

/// A local Ollama model.
#[derive(Debug, Clone)]
pub struct Ollama {
    addr: SocketAddr,
    model: String,
}

impl Ollama {
    /// `hostport` must resolve only to loopback addresses.
    ///
    /// # Errors
    /// A sentence when the address is not loopback or does not resolve.
    pub fn new(hostport: &str, model: &str) -> Result<Self, String> {
        let addrs: Vec<SocketAddr> = hostport
            .to_socket_addrs()
            .map_err(|e| format!("cannot resolve {hostport}: {e}"))?
            .collect();
        if addrs.is_empty() || addrs.iter().any(|a| !a.ip().is_loopback()) {
            return Err(format!(
                "the diagnosis model must be on this machine (a loopback address), not {hostport}"
            ));
        }
        if model.is_empty()
            || model
                .chars()
                .any(|c| c.is_control() || c == '"' || c == '\\')
        {
            return Err("the diagnosis model name is not valid".to_owned());
        }
        Ok(Self {
            addr: addrs[0],
            model: model.to_owned(),
        })
    }
}

impl DiagnosisModel for Ollama {
    fn ask(&self, prompt: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": self.model, "prompt": prompt, "stream": false, "think": false,
            "options": {"temperature": 0, "seed": 7, "num_ctx": 8192, "num_predict": 800}
        })
        .to_string();
        let mut s = TcpStream::connect_timeout(&self.addr, Duration::from_secs(5))
            .map_err(|e| format!("connect: {e}"))?;
        s.set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        s.set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        let request = format!(
            "POST /api/generate HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.addr,
            body.len()
        );
        s.write_all(request.as_bytes())
            .map_err(|e| format!("send: {e}"))?;
        let mut raw = Vec::new();
        s.take(MAX_REPLY as u64)
            .read_to_end(&mut raw)
            .map_err(|e| format!("read: {e}"))?;
        let text = String::from_utf8_lossy(&raw);
        let (head, rest) = text.split_once("\r\n\r\n").ok_or("no HTTP reply")?;
        if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
            return Err(format!(
                "the model server said: {}",
                head.lines().next().unwrap_or("")
            ));
        }
        let body = if head.to_lowercase().contains("transfer-encoding: chunked") {
            dechunk(rest)
        } else {
            rest.to_owned()
        };
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("reply: {e}"))?;
        v["response"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "the model server's reply has no `response`".to_owned())
    }
}

/// A chunked body, joined. Bad framing ends it where it breaks (the JSON parse then fails).
fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if n == 0 || after.len() < n {
            break;
        }
        let Some((chunk, tail)) = after.split_at_checked(n) else {
            break;
        };
        out.push_str(chunk);
        rest = tail.strip_prefix("\r\n").unwrap_or(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_loopback_address_is_accepted() {
        assert!(Ollama::new("127.0.0.1:11434", "m").is_ok());
        assert!(Ollama::new("192.0.2.5:11434", "m").is_err());
        assert!(Ollama::new("8.8.8.8:11434", "m").is_err());
        assert!(Ollama::new("127.0.0.1:11434", "bad\"name").is_err());
    }

    #[test]
    fn a_chunked_body_is_joined() {
        assert_eq!(
            dechunk("5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"),
            "hello world"
        );
    }
}

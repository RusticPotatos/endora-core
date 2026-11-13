//! HQ, as the runner speaks to it (spec section 15, phase 4, item 4): a turn to start, how one
//! ended, and the turns to stop, each signed with the runner's own token, which HQ makes and
//! keeps in a file the two of them mount.

use std::net::{TcpStream, ToSocketAddrs as _};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use crate::http::{self, Answer};

/// How long HQ may take to answer.
const ANSWERS_WITHIN: Duration = Duration::from_secs(10);

/// Why HQ was not heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HqError {
    /// HQ could not be reached, has no token for the runner yet, or failed: worth asking
    /// again.
    Unreachable(String),
    /// HQ answered, and not with what was asked for.
    Answered(u16),
}

impl std::fmt::Display for HqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => f.write_str(why),
            Self::Answered(status) => write!(f, "HQ answered {status}"),
        }
    }
}

/// How a turn ended, as the runner reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ending {
    /// Whether the worker took its turn (exit 0).
    pub succeeded: bool,
    /// The worker's exit code; `None` for a container gone before it was seen to end.
    pub code: Option<i64>,
    /// Why, in words: what the exit code means and the worker's last words on stderr, with
    /// anything token-shaped taken out.
    pub why: String,
}

/// The path of the node's team gateway (ADR 0110), which passes `/runner/...` on to HQ's
/// `/v1/runner/...`: how a runner on another host, such as the Pi, reaches HQ, which is not
/// published.
const GATEWAY: &str = "/team-gateway";

/// HQ, at a plain `http://` address on the house's own network.
pub struct Hq {
    /// Where to connect: `host:port`.
    address: String,
    /// Anything before the routes, without its last slash.
    prefix: String,
    /// The file HQ keeps the runner's token in.
    token_file: PathBuf,
    /// What the runner calls itself, sent with each signed call (`?runner=`) so HQ can tell it
    /// from another runner with the same token. Empty: the runner says no name, as an older
    /// one.
    name: String,
}

/// A runner's name as the runner sends it: ASCII letters, digits, `.`, `_` and `-`, at most 32.
/// HQ cleans what it receives by the same rule, so this is no defence, only what a line in a
/// URL needs; nothing left is `None`.
pub fn runner_name(raw: &str) -> Option<String> {
    let name: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(32)
        .collect();
    (!name.is_empty()).then_some(name)
}

impl Hq {
    /// HQ at `url`, with the runner's token in `token_file`.
    ///
    /// # Errors
    /// The URL is not a plain `http://` one.
    pub fn new(url: &str, token_file: impl Into<PathBuf>) -> Result<Self, String> {
        let not = || format!("HQ_URL is not a plain http:// address: {url:?}");
        let Some(rest) = url.trim().strip_prefix("http://") else {
            return Err(not());
        };
        let (authority, prefix) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() || !authority.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(not());
        }
        // A port after the host, not inside an IPv6 address's brackets.
        let address = if authority
            .rsplit_once(':')
            .is_some_and(|(_, p)| !p.contains(']'))
        {
            authority.to_owned()
        } else {
            format!("{authority}:80")
        };
        let prefix = prefix.trim_end_matches('/');
        if !prefix.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(not());
        }
        Ok(Self {
            address,
            prefix: if prefix.is_empty() {
                String::new()
            } else {
                format!("/{prefix}")
            },
            token_file: token_file.into(),
            name: String::new(),
        })
    }

    /// The same HQ, with the runner saying it is called `name` (cleaned by [`runner_name`]; no
    /// name when nothing of it is left).
    #[must_use]
    pub fn named(mut self, name: &str) -> Self {
        self.name = runner_name(name).unwrap_or_default();
        self
    }

    /// What the runner calls itself to HQ; empty if it says nothing.
    #[cfg(test)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where HQ is, for the log.
    pub fn url(&self) -> String {
        format!("http://{}{}", self.address, self.prefix)
    }

    /// Whether HQ is reached through the node's team gateway, as from the Pi.
    pub fn through_the_gateway(&self) -> bool {
        self.prefix == GATEWAY
    }

    /// Where HQ's address is, without its path: through the gateway, the node's team listener.
    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    /// A runner route (`/next`, `/stops`, ...) as HQ's address takes it: HQ's own
    /// `/v1/runner/...`, or the gateway's `/runner/...`, which it passes on to that.
    fn route(&self, route: &str) -> String {
        if self.through_the_gateway() {
            return format!("/runner{route}");
        }
        format!("/v1/runner{route}")
    }

    /// The runner's token, as HQ keeps it, read each time so one HQ made after the runner
    /// started is found.
    fn token(&self) -> Result<String, HqError> {
        let token = std::fs::read_to_string(&self.token_file)
            .map(|t| t.trim().to_owned())
            .map_err(|_| {
                HqError::Unreachable("HQ has not made the runner's token yet".to_owned())
            })?;
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(HqError::Unreachable(
                "the runner's token file holds no token".to_owned(),
            ));
        }
        Ok(token)
    }

    /// One call to HQ, signed with the runner's token. A failure on HQ's side is worth asking
    /// again, like no answer.
    fn call(&self, method: &str, route: &str, body: Option<&Value>) -> Result<Answer, HqError> {
        let token = self.token()?;
        // The name rides every call the runner's token signs, in the query: the one place the
        // node's team gateway passes on for a call with no body, as it does for this
        // parameter alone.
        let signed_route;
        let route = if self.name.is_empty() {
            route
        } else {
            signed_route = format!("{route}?runner={}", self.name);
            &signed_route
        };
        self.call_as(&token, method, route, body)
    }

    /// One call to HQ, signed with `token`: the runner's, or a check job's own.
    fn call_as(
        &self,
        token: &str,
        method: &str,
        route: &str,
        body: Option<&Value>,
    ) -> Result<Answer, HqError> {
        let unreachable =
            |e: &dyn std::fmt::Display| HqError::Unreachable(format!("HQ cannot be reached: {e}"));
        let address = self
            .address
            .to_socket_addrs()
            .map_err(|e| unreachable(&e))?
            .next()
            .ok_or_else(|| unreachable(&format!("{} has no address", self.address)))?;
        let mut stream =
            TcpStream::connect_timeout(&address, ANSWERS_WITHIN).map_err(|e| unreachable(&e))?;
        stream
            .set_read_timeout(Some(ANSWERS_WITHIN))
            .and_then(|()| stream.set_write_timeout(Some(ANSWERS_WITHIN)))
            .map_err(|e| unreachable(&e))?;
        let path = format!("{}{route}", self.prefix);
        let answer = http::send(&mut stream, method, &self.address, &path, Some(token), body)
            .map_err(|e| unreachable(&e))?;
        if answer.status >= 500 {
            return Err(HqError::Unreachable(format!(
                "HQ failed ({})",
                answer.status
            )));
        }
        Ok(answer)
    }

    /// A turn to start, with `free` places for one; `None` when there is none.
    ///
    /// # Errors
    /// [`HqError`] if HQ was not heard.
    pub fn next(&self, free: usize) -> Result<Option<Value>, HqError> {
        let answer = self.call("POST", &self.route("/next"), Some(&json!({ "free": free })))?;
        match answer.status {
            204 => Ok(None),
            200 => Ok(Some(answer.json())),
            status => Err(HqError::Answered(status)),
        }
    }

    /// Tells HQ how a run ended, quoting the lease it was handed out with: whether it
    /// succeeded, the worker's exit code (`None` for a container that was never seen to end)
    /// and why, in words HQ keeps on the run's record. HQ refuses (409) a report from an
    /// attempt that is no longer the run's current one.
    ///
    /// # Errors
    /// [`HqError`] if HQ was not heard, or did not take it.
    pub fn ended(&self, run: u64, lease: u64, ending: &Ending) -> Result<(), HqError> {
        let route = self.route(&format!("/{run}/ended"));
        let body = json!({
            "succeeded": ending.succeeded,
            "lease": lease,
            "code": ending.code,
            "why": ending.why,
        });
        let answer = self.call("POST", &route, Some(&body))?;
        if !(200..300).contains(&answer.status) {
            return Err(HqError::Answered(answer.status));
        }
        Ok(())
    }

    /// Where a check job reports (ADR 0115), up to its id: the worker in the job's container
    /// posts to `<this><id>/done` with the job's token, through the gateway when the runner
    /// goes through it.
    pub fn check_done_prefix(&self) -> String {
        format!("{}{}", self.url(), self.route("/check/"))
    }

    /// Reports a check job that never ran, or ended without reporting, with the job's own
    /// token (ADR 0115): HQ takes a check's report once, so one the worker made first is
    /// refused (409), which is no failure here.
    ///
    /// # Errors
    /// [`HqError`] if HQ was not heard.
    pub fn check_failed(&self, check: u64, token: &str, why: &str) -> Result<(), HqError> {
        let route = self.route(&format!("/check/{check}/done"));
        let body = json!({ "ok": false, "summary": why, "report": "" });
        let answer = self.call_as(token, "POST", &route, Some(&body))?;
        if (200..300).contains(&answer.status) || answer.status == 409 {
            return Ok(());
        }
        Err(HqError::Answered(answer.status))
    }

    /// Of the items whose sandboxes the runner keeps (ADR 0142, amended 2026-11-01), those HQ
    /// says to keep: the ones still open. The rest are done, dropped or blocked, and their
    /// sandboxes go. An HQ without the route says nothing, and the runner keeps them all until
    /// they have been idle too long.
    ///
    /// # Errors
    /// [`HqError`] if HQ was not heard, or did not answer with a list.
    pub fn sandboxes(&self, items: &[u64]) -> Result<Vec<u64>, HqError> {
        let body = json!({ "items": items });
        let answer = self.call("POST", &self.route("/sandboxes"), Some(&body))?;
        if answer.status != 200 {
            return Err(HqError::Answered(answer.status));
        }
        Ok(answer.json()["keep"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect())
    }

    /// The runs HQ asks to stop.
    ///
    /// # Errors
    /// [`HqError`] if HQ was not heard.
    pub fn stops(&self) -> Result<Vec<u64>, HqError> {
        let answer = self.call("GET", &self.route("/stops"), None)?;
        if answer.status != 200 {
            return Err(HqError::Answered(answer.status));
        }
        Ok(answer.json()["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hq_is_a_plain_http_address_on_the_houses_network() {
        let hq = Hq::new("http://hq:8790", "/runner-token/token").unwrap();
        assert_eq!(hq.url(), "http://hq:8790");
        let hq = Hq::new("http://172.31.99.10:8790/team/", "/t").unwrap();
        assert_eq!(hq.url(), "http://172.31.99.10:8790/team");
        assert_eq!(
            Hq::new("http://hq", "/t").unwrap().url(),
            "http://hq:80",
            "the port HTTP is on unless it says"
        );
        for refused in [
            "https://hq:8790",
            "hq:8790",
            "",
            "http://",
            "http://hq:8790\r\nX-Evil: 1",
            "http://hq:8790/a b",
        ] {
            assert!(Hq::new(refused, "/t").is_err(), "{refused:?}");
        }
    }

    #[test]
    fn through_the_team_gateway_the_runners_routes_are_the_gateways() {
        assert!(
            !Hq::new("http://hq:8790", "/t")
                .unwrap()
                .through_the_gateway()
        );
        let hq = Hq::new("http://172.31.99.10:8788/team-gateway/", "/t").unwrap();
        assert!(hq.through_the_gateway());
        assert_eq!(hq.origin(), "http://172.31.99.10:8788");
        assert_eq!(
            hq.check_done_prefix(),
            "http://172.31.99.10:8788/team-gateway/runner/check/"
        );
        assert_eq!(
            Hq::new("http://hq:8790", "/t").unwrap().check_done_prefix(),
            "http://hq:8790/v1/runner/check/"
        );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/team-gateway", listener.local_addr().unwrap());
        let file = std::env::temp_dir().join(format!("endora-runner-gw-{}", std::process::id()));
        std::fs::write(&file, "a-runner-token-of-at-least-thirty-two-characters").unwrap();
        let asked = std::thread::spawn(move || {
            use std::io::{Read as _, Write as _};
            let (mut socket, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8];
            while !head.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let body = r#"{"runs":[3]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(head).unwrap()
        });
        let hq = Hq::new(&url, &file).unwrap();
        assert_eq!(hq.stops(), Ok(vec![3]));
        assert!(
            asked
                .join()
                .unwrap()
                .starts_with("GET /team-gateway/runner/stops HTTP/1.1"),
            "the gateway passes /runner/ on to HQ's /v1/runner/"
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn without_its_token_the_runner_asks_nothing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let hq = Hq::new(&url, "/nonexistent/runner-token").unwrap();
        assert!(matches!(hq.next(2), Err(HqError::Unreachable(_))));
        assert!(matches!(hq.stops(), Err(HqError::Unreachable(_))));
        assert!(listener.accept().is_err(), "not even a connection was made");
    }

    /// Serves one canned 200 and gives back the request line it was asked.
    fn asked_once(path_prefix: &str, name: &str, ask: impl FnOnce(&Hq)) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}{path_prefix}", listener.local_addr().unwrap());
        let file = std::env::temp_dir().join(format!(
            "endora-runner-name-{}-{}",
            std::process::id(),
            name.len()
        ));
        std::fs::write(&file, "a-runner-token-of-at-least-thirty-two-characters").unwrap();
        let served = std::thread::spawn(move || {
            use std::io::{Read as _, Write as _};
            let (mut socket, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8];
            while !head.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let body = r#"{"runs":[]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(head).unwrap()
        });
        let hq = Hq::new(&url, &file).unwrap().named(name);
        ask(&hq);
        let _ = std::fs::remove_file(&file);
        served.join().unwrap().lines().next().unwrap().to_owned()
    }

    #[test]
    fn a_named_runner_says_its_name_on_every_call_direct_and_through_the_gateway() {
        assert_eq!(
            asked_once("", "runner-a", |hq| {
                hq.stops().unwrap();
            }),
            "GET /v1/runner/stops?runner=runner-a HTTP/1.1"
        );
        assert_eq!(
            asked_once("/team-gateway", "runner-a", |hq| {
                hq.stops().unwrap();
            }),
            "GET /team-gateway/runner/stops?runner=runner-a HTTP/1.1"
        );
        assert_eq!(
            asked_once("/team-gateway", "runner-a", |hq| {
                let _ = hq.next(1);
            }),
            "POST /team-gateway/runner/next?runner=runner-a HTTP/1.1"
        );
    }

    #[test]
    fn a_runner_with_no_name_asks_as_it_always_did() {
        assert_eq!(
            asked_once("", "", |hq| {
                hq.stops().unwrap();
            }),
            "GET /v1/runner/stops HTTP/1.1"
        );
    }

    #[test]
    fn a_runners_name_is_cut_to_what_a_name_may_hold() {
        assert_eq!(runner_name("Pi 5/lan;x\r\n").as_deref(), Some("Pi5lanx"));
        assert_eq!(runner_name(&"a".repeat(50)).unwrap().len(), 32);
        assert_eq!(runner_name("  /; "), None);
        assert_eq!(
            Hq::new("http://hq", "/t").unwrap().named("a b").name(),
            "ab"
        );
    }
}

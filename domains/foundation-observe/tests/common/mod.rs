//! A dummy service the tests own: a localhost TCP server that answers 200 while a `healthy`
//! file exists in its sandbox and 503 otherwise, with a restart script that makes it healthy.
#![allow(dead_code, missing_docs)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Lab {
    pub dir: PathBuf,
    pub sandbox: PathBuf,
    pub port: u16,
    stop: Arc<AtomicBool>,
}

impl Lab {
    pub fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("lab-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sandbox = dir.join("sandbox");
        std::fs::create_dir_all(&sandbox).unwrap();
        let script = |file: &str, body: &str| std::fs::write(sandbox.join(file), body).unwrap();
        script(
            "restart.sh",
            "touch healthy\necho restart >> restarts.log\n",
        );
        script(
            "slow-restart.sh",
            "touch started\nsleep 3\ntouch healthy\necho restart >> restarts.log\n",
        );
        script("disk.sh", "echo disk ok\n");
        script("data.db", "precious");
        script("healthy", "");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, root) = (stop.clone(), sandbox.clone());
        std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let Ok((mut conn, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                conn.set_nonblocking(false).unwrap();
                conn.set_read_timeout(Some(std::time::Duration::from_millis(500)))
                    .unwrap();
                let mut buf = [0u8; 512];
                let _ = conn.read(&mut buf);
                let up = root.join("healthy").exists();
                let reply = if up {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                } else {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndown"
                };
                let _ = conn.write_all(reply.as_bytes());
            }
        });
        Self {
            dir,
            sandbox,
            port,
            stop,
        }
    }

    pub fn url(&self) -> String {
        format!("127.0.0.1:{}/health", self.port)
    }

    pub fn set_healthy(&self, up: bool) {
        let f = self.sandbox.join("healthy");
        if up {
            std::fs::write(f, "").unwrap()
        } else {
            let _ = std::fs::remove_file(f);
        }
    }

    pub fn healthy(&self) -> bool {
        self.sandbox.join("healthy").exists()
    }

    pub fn restarts(&self) -> usize {
        std::fs::read_to_string(self.sandbox.join("restarts.log")).map_or(0, |t| t.lines().count())
    }

    pub fn data_intact(&self) -> bool {
        std::fs::read_to_string(self.sandbox.join("data.db")).is_ok_and(|t| t == "precious")
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A fixed owner token for tests (the CLI generates a random one).
pub const TOKEN: &str = concat!(
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef",
    "0123456789abcdef"
);

/// Enrols the test token if the file has no owner yet, then authenticates with it.
pub fn owner(
    core: &endora_foundation::application::Core,
) -> endora_foundation::application::OwnerProof {
    let _ = core.enroll_owner(TOKEN);
    core.authenticate_owner(TOKEN).expect("test owner token")
}

/// A capability actor with this name.
pub fn actor_named(name: &str) -> endora_foundation::domain::Actor {
    endora_foundation::domain::Actor::capability(name).expect("a plain test name")
}

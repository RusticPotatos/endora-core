//! `foundation-run`: the demo runner a test kills. It opens the one SQLite file, starts a work
//! item and makes a call, then (in a `*-hold` mode) waits to be killed mid-item.
//!
//! `--mode probe-hold`: start, probe once, print `READY`, wait.
//! `--mode restart-hold`: start, call `service.restart` (whose script may block), wait.
//! `--mode finish`: start, verify, ask policy to accept, exit.

use std::sync::Arc;

use endora_foundation::application::Core;
use endora_foundation::domain::{Actor, ItemId};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_observe::{Config, ObserveVerify};

fn arg(name: &str) -> String {
    let all: Vec<String> = std::env::args().collect();
    all.iter()
        .position(|a| a == name)
        .and_then(|i| all.get(i + 1).cloned())
        .unwrap_or_else(|| {
            eprintln!("foundation-run: missing {name}");
            std::process::exit(2);
        })
}

fn main() {
    let sandbox = arg("--sandbox");
    let mut core = Core::open(&arg("--db"), Arc::new(SystemClock)).unwrap_or_else(|e| {
        eprintln!("foundation-run: {e}");
        std::process::exit(1);
    });
    core.register(Arc::new(ObserveVerify::new(Config {
        sandbox: sandbox.into(),
        probes: vec![("disk".into(), "disk.sh".into())],
        restart_script: arg("--restart-script"),
    })))
    .unwrap_or_else(|e| {
        eprintln!("foundation-run: {e}");
        std::process::exit(1)
    });
    let runner = Actor::capability("runner").expect("plain name");
    let item = ItemId(arg("--item").parse().unwrap_or(0));
    let url = arg("--url");
    let fail = |e: endora_kernel::AppError| -> ! {
        eprintln!("foundation-run: {e}");
        std::process::exit(1)
    };
    core.start(&runner, item).unwrap_or_else(|e| fail(e));
    match arg("--mode").as_str() {
        "probe-hold" => {
            core.call(&runner, item, "probe.http", &url)
                .unwrap_or_else(|e| fail(e));
            println!("READY item={}", item.0);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        "restart-hold" => {
            core.call(&runner, item, "service.restart", "restart")
                .unwrap_or_else(|e| fail(e));
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        _ => {
            core.verify(item).unwrap_or_else(|e| fail(e));
            let done = core
                .settle_routine_by_policy(item)
                .unwrap_or_else(|e| fail(e));
            println!("{done:?}");
        }
    }
}

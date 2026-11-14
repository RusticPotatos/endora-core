//! `foundation-git-run`: the runner a test kills. It opens the one SQLite file, starts a work
//! item, prepares a repository and runs the (scripted) agent through Core, then exits. With a
//! blocking agent script it is killed mid-agent.

use std::sync::Arc;
use std::time::Duration;

use endora_foundation::application::Core;
use endora_foundation::domain::{Actor, ItemId};
use endora_foundation::infrastructure::SystemClock;
use endora_foundation_git::{Config, Confinement, GitChange, Options};

fn arg(name: &str) -> String {
    let all: Vec<String> = std::env::args().collect();
    all.iter()
        .position(|a| a == name)
        .and_then(|i| all.get(i + 1).cloned())
        .unwrap_or_else(|| {
            eprintln!("foundation-git-run: missing {name}");
            std::process::exit(2);
        })
}

fn main() {
    let die = |e: endora_kernel::AppError| -> ! {
        eprintln!("foundation-git-run: {e}");
        std::process::exit(1)
    };
    let mut core = Core::open(&arg("--db"), Arc::new(SystemClock)).unwrap_or_else(|e| die(e));
    // `--env-allow NAME` (repeatable) lets one parent variable through to the confined child.
    let all: Vec<String> = std::env::args().collect();
    let env_allow = all
        .windows(2)
        .filter(|w| w[0] == "--env-allow")
        .map(|w| w[1].clone())
        .collect();
    core.register(Arc::new(GitChange::with_options(
        Config {
            scratch_root: arg("--scratch").into(),
            template: arg("--template").into(),
            agent_command: vec!["sh".into(), arg("--agent-script")],
            test_command: vec!["sh".into(), "test.sh".into()],
            protected: vec!["test.sh".into()],
            agent_limit: Duration::from_secs(600),
            test_limit: Duration::from_secs(60),
            confinement: Confinement::Auto,
        },
        Options {
            env_allow,
            ..Options::default()
        },
    )))
    .unwrap_or_else(|e| die(e));
    let item = ItemId(arg("--item").parse().unwrap_or(0));
    let job = arg("--job");
    let runner = Actor::capability("runner").expect("plain name");
    core.start(&runner, item).unwrap_or_else(|e| die(e));
    for call in ["git.prepare", "git.change"] {
        let out = core
            .call(&runner, item, call, &job)
            .unwrap_or_else(|e| die(e));
        println!("{call}: {out:?}");
    }
}

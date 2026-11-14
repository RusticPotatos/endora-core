//! `foundation-git-status <db-file> [tail]`: plain text, read-only. Goals, items and the evidence
//! tail (all capabilities share the one file), headed by the items waiting for acceptance.

use endora_foundation::{application::core::status_text, domain::ItemState, infrastructure::Store};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: foundation-git-status <db-file> [tail]");
        std::process::exit(2);
    };
    let tail = std::env::args()
        .nth(2)
        .and_then(|t| t.parse().ok())
        .unwrap_or(15);
    let run = || -> Result<String, endora_kernel::AppError> {
        let store = Store::open(&path)?;
        let mut waiting = String::new();
        for i in store
            .items()?
            .into_iter()
            .filter(|i| i.state == ItemState::Running)
        {
            let k = store.criteria(i.id)?;
            if !k.is_empty() && k.iter().all(|c| c.met_by.is_some()) {
                waiting.push_str(&format!(
                    "  #{} {}: every criterion met, waiting for the owner\n",
                    i.id.0, i.title
                ));
            }
        }
        Ok(format!(
            "AWAITING ACCEPTANCE\n{waiting}\n{}",
            status_text(&store, tail)?
        ))
    };
    match run() {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("foundation-git-status: {e}");
            std::process::exit(1);
        }
    }
}

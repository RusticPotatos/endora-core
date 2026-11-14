//! `foundation-status <db-file>`: plain text, read-only: goals, items, the evidence tail.

use endora_foundation::{application::core::status_text, infrastructure::Store};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: foundation-status <db-file> [tail]");
        std::process::exit(2);
    };
    let tail = std::env::args()
        .nth(2)
        .and_then(|t| t.parse().ok())
        .unwrap_or(15);
    match Store::open(&path)
        .map_err(Into::into)
        .and_then(|s| status_text(&s, tail))
    {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("foundation-status: {e}");
            std::process::exit(1);
        }
    }
}

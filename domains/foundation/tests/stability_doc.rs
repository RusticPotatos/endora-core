//! The interface table in docs/foundation/STABILITY.md cannot drift: every row's status is
//! FROZEN or EXPERIMENTAL (nothing else), and the table is not empty.

use std::path::PathBuf;

fn doc() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/foundation/STABILITY.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// (interface, status cell) of every row of the four-column interface table.
fn rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| l.starts_with("| "))
        .map(|l| l.split('|').map(str::trim).collect::<Vec<_>>())
        .filter(|c| c.len() >= 6 && c[1] != "Interface" && !c[1].starts_with("---"))
        .map(|c| (c[1].to_owned(), c[2].to_owned()))
        .collect()
}

#[test]
fn every_interface_row_is_frozen_or_experimental_and_nothing_else() {
    let rows = rows(&doc());
    assert!(
        rows.len() >= 15,
        "the interface table was not found: {rows:?}"
    );
    for (name, status) in &rows {
        let word = status.split([' ', ',', ':', '(']).next().unwrap_or("");
        assert!(
            word == "FROZEN" || word == "EXPERIMENTAL",
            "row `{name}` has status `{status}`: only FROZEN or EXPERIMENTAL are allowed"
        );
    }
}

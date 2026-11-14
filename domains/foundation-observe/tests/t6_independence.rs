//! T6: the domain crate does not know capability A. These tests read the manifests and sources.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn t6_the_foundation_crate_names_no_capability_in_its_manifest_or_its_source() {
    let manifest = std::fs::read_to_string(root().join("foundation/Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("foundation-observe"),
        "the domain depends on a capability"
    );
    let mut stack = vec![root().join("foundation/src")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for word in [
                "foundation_observe",
                "probe.http",
                "service.restart",
                "ObserveVerify",
            ] {
                assert!(!text.contains(word), "{} names `{word}`", p.display());
            }
        }
    }
}

#[test]
fn t6_the_capability_depends_on_the_domain_and_not_the_other_way() {
    let manifest = std::fs::read_to_string(root().join("foundation-observe/Cargo.toml")).unwrap();
    assert!(manifest.contains("endora-foundation = "));
}

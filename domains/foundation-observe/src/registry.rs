//! `registry.tags`: a read-only look at the public tags of one image, to say whether a newer
//! stable tag of the same family exists than the one pinned. The owner names the registry host
//! in the config (`registry-host`); only an allow-listed host registers. The fetch is one https
//! GET with no credential, behind the [`TagSource`] port, so tests use a fake and never touch a
//! network. The fact is one closed word, `update` (`available` or `current`); the newest tag's
//! name is text for people and is never a fact. A fetch or parse error, or a page with no tag of
//! the pin's family, is a failed call with no facts: never a guess.
//!
//! The tag rules (written for this capability): an optional `v` prefix is ignored; the version
//! is the leading dotted numbers, compared as numbers with missing parts as zero; a trailing
//! `-lsNN` is a rebuild of the same version (it never makes a tag newer, only breaks a tie when
//! naming the newest); a tag with an `rc`, `beta`, `alpha`, `pre`, `preview`, `dev`, `nightly`,
//! `snapshot` or `canary` word in its suffix is a prerelease and is skipped; the rest of the
//! suffix (`-alpine`, `ubu2604`) is the tag's family, and only the pin's own family is compared.

use std::cmp::Ordering;

use endora_foundation::application::{CallFailure, CallOutput, Capability};
use endora_foundation::domain::{BudgetMode, CallSpec, Fact, Field};
use endora_kernel::{Reversibility, Tier};

/// The only registry hosts a `registry.tags` capability may be registered for.
pub const ALLOWED_HOSTS: &[&str] = &["hub.docker.com"];
/// The call name Core routes to this capability.
pub const CALL: &str = "registry.tags";
/// Most pages read for one image (100 tags each, newest first).
const MOST_PAGES: usize = 5;
/// Most bytes of one page read.
const MOST_PAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Words that make a tag a prerelease (a trailing number, as in `rc1`, is ignored).
const PRERELEASE_WORDS: &[&str] = &[
    "rc", "beta", "alpha", "pre", "preview", "dev", "nightly", "snapshot", "canary",
];

/// Where tag pages come from (the port): one https GET, returning the body.
pub trait TagSource: Send + Sync {
    /// The body of the page at `url`.
    ///
    /// # Errors
    /// A sentence when the page could not be fetched.
    fn get(&self, url: &str) -> Result<String, String>;
}

/// The real source: an https GET with a timeout, no redirects, no credential, a bounded body.
pub struct HubSource;

impl TagSource for HubSource {
    fn get(&self, url: &str) -> Result<String, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .max_redirects(0)
            .https_only(true)
            .build()
            .into();
        let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
        resp.body_mut()
            .with_config()
            .limit(MOST_PAGE_BYTES)
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}

/// What the tags say about the pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The newest stable tag of the pin's family, as the registry spells it.
    pub newest: String,
    /// Whether its version is greater than the pin's.
    pub update: bool,
}

#[derive(Debug)]
struct Parsed {
    core: Vec<u64>,
    family: String,
    prerelease: bool,
    rebuild: u64,
}

fn parse(tag: &str) -> Option<Parsed> {
    let t = match tag.strip_prefix(['v', 'V']) {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => tag,
    };
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, rest) = t.split_at(end);
    let num = num.trim_end_matches('.');
    if !num.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let core = num
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let rest = rest.to_ascii_lowercase();
    let (family, rebuild) = match rest.rfind("-ls") {
        Some(i)
            if !rest[i + 3..].is_empty() && rest[i + 3..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            (rest[..i].to_owned(), rest[i + 3..].parse().ok()?)
        }
        _ => (rest, 0),
    };
    let prerelease = family
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.trim_end_matches(|c: char| c.is_ascii_digit()))
        .any(|w| PRERELEASE_WORDS.contains(&w));
    Some(Parsed {
        core,
        family,
        prerelease,
        rebuild,
    })
}

fn cmp_core(a: &[u64], b: &[u64]) -> Ordering {
    (0..a.len().max(b.len()))
        .map(|i| {
            a.get(i)
                .copied()
                .unwrap_or(0)
                .cmp(&b.get(i).copied().unwrap_or(0))
        })
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// What `tags` say about `pin`.
///
/// # Errors
/// A sentence when no honest verdict can be given: the pin is not a stable version, or no stable
/// tag of the pin's family is among `tags`.
pub fn judge(pin: &str, tags: &[String]) -> Result<Verdict, String> {
    let pinned = parse(pin).ok_or_else(|| format!("the pin `{pin}` is not a version"))?;
    if pinned.prerelease {
        return Err(format!("the pin `{pin}` is a prerelease"));
    }
    let newest = tags
        .iter()
        .filter_map(|t| parse(t).map(|p| (t, p)))
        .filter(|(_, p)| !p.prerelease && p.family == pinned.family)
        .max_by(|(_, a), (_, b)| cmp_core(&a.core, &b.core).then(a.rebuild.cmp(&b.rebuild)))
        .ok_or_else(|| format!("no stable tag of the family of `{pin}` among the tags read"))?;
    Ok(Verdict {
        newest: newest.0.clone(),
        update: cmp_core(&newest.1.core, &pinned.core).is_gt(),
    })
}

fn name_ok(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        && !s.contains("..")
}

/// `name` or `namespace/name`, as the repository path: a bare name is in `library`.
fn repository(image: &str) -> Result<String, String> {
    let parts: Vec<&str> = image.split('/').collect();
    match parts.as_slice() {
        [n] if name_ok(n) => Ok(format!("library/{n}")),
        [ns, n] if name_ok(ns) && name_ok(n) => Ok(format!("{ns}/{n}")),
        _ => Err(format!(
            "`{image}` is not an image name (`name` or `namespace/name`)"
        )),
    }
}

fn pin_ok(pin: &str) -> bool {
    !pin.is_empty()
        && pin.len() <= 128
        && pin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
}

/// The capability.
pub struct RegistryTags {
    host: String,
    source: Box<dyn TagSource>,
}

impl RegistryTags {
    /// A capability reading tags from `host` through `source`.
    ///
    /// # Errors
    /// A sentence when `host` is not in [`ALLOWED_HOSTS`].
    pub fn new(host: &str, source: Box<dyn TagSource>) -> Result<Self, String> {
        if !ALLOWED_HOSTS.contains(&host) {
            return Err(format!(
                "registry host `{host}` is not allowed (allowed: {})",
                ALLOWED_HOSTS.join(", ")
            ));
        }
        Ok(Self {
            host: host.to_owned(),
            source,
        })
    }

    fn tag_names(&self, repo: &str) -> Result<Vec<String>, String> {
        let prefix = format!("https://{}/", self.host);
        let mut url =
            format!("{prefix}v2/repositories/{repo}/tags?page_size=100&ordering=last_updated");
        let mut names = Vec::new();
        for _ in 0..MOST_PAGES {
            let body = self.source.get(&url)?;
            let page: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| format!("the tags page is not JSON: {e}"))?;
            let results = page
                .get("results")
                .and_then(serde_json::Value::as_array)
                .ok_or("the tags page has no `results` list")?;
            for r in results {
                let name = r
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("a tag on the page has no name")?;
                names.push(name.to_owned());
            }
            match page.get("next") {
                None | Some(serde_json::Value::Null) => break,
                Some(serde_json::Value::String(n)) if n.starts_with(&prefix) => url = n.clone(),
                Some(_) => return Err(format!("the next page is not on {}", self.host)),
            }
        }
        Ok(names)
    }
}

impl Capability for RegistryTags {
    fn offers(&self) -> Vec<CallSpec> {
        vec![CallSpec {
            name: CALL.into(),
            band: Some(Reversibility::Observe),
            input: "image (name or namespace/name), then the pin (its current tag), one per line".into(),
            output: "fact: update (available | current); the newest stable tag of the pin's family is in the result text".into(),
            tier: Tier::Public,
            max_cost_micros: 0,
            max_seconds: 60,
            max_turns: None,
            inputs: vec![Field::text("image"), Field::text("pin")],
            facts: vec![Field::word("update", &["available", "current"])],
            budget: BudgetMode::Unenforced,
        }]
    }

    fn call(&self, _call: &str, input: &str) -> Result<CallOutput, CallFailure> {
        let (image, pin) = input
            .split_once('\n')
            .map(|(i, p)| (i.trim(), p.trim()))
            .ok_or_else(|| CallFailure::new("input is the image, a newline, then the pin"))?;
        let repo = repository(image).map_err(CallFailure::new)?;
        if !pin_ok(pin) {
            return Err(CallFailure::new(format!("`{pin}` is not a tag")));
        }
        let names = self.tag_names(&repo).map_err(CallFailure::new)?;
        let v = judge(pin, &names).map_err(CallFailure::new)?;
        let word = if v.update { "available" } else { "current" };
        Ok(CallOutput {
            cost_micros: 0,
            profile: None,
            facts: vec![Fact::word("update", word)],
            result: format!("{repo} pin {pin}: newest stable tag {} ({word})", v.newest),
            evidence: format!("read {} tags of {repo} on {}", names.len(), self.host),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use endora_foundation::domain::{BudgetMode, FactValue};
    use endora_kernel::Reversibility;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    /// (pin, tags on the page, expected newest, expected newer-than-pin)
    #[test]
    fn the_fact_table() {
        let table: &[(&str, &[&str], &str, bool, &str)] = &[
            (
                "1.4.0",
                &["1.5.0-rc1", "1.4.0", "1.4.1-rc.2"],
                "1.4.0",
                false,
                "an rc is skipped",
            ),
            (
                "1.4.0",
                &["2.0.0-beta.2", "1.4.0", "1.4.1"],
                "1.4.1",
                true,
                "a -beta.2 is skipped, the stable found",
            ),
            (
                "1.4.0",
                &["9.9.9-alpha", "1.4.0", "1.4.1-nightly", "1.4.1-pre3"],
                "1.4.0",
                false,
                "alpha, nightly, pre are skipped",
            ),
            (
                "1.4.0",
                &["2.0.0-alpine", "1.4.0", "1.4.2-alpine"],
                "1.4.0",
                false,
                "a -alpine tag is another family than a plain pin",
            ),
            (
                "1.4.0-alpine",
                &["2.0.0", "1.4.0-alpine", "1.5.0-alpine", "1.6.0-slim"],
                "1.5.0-alpine",
                true,
                "an -alpine pin takes only -alpine tags",
            ),
            (
                "4.0.20-ls10",
                &["4.0.20-ls12", "4.0.20-ls11", "4.0.20-ls10"],
                "4.0.20-ls12",
                false,
                "an -lsNN rebuild of the same version is current",
            ),
            (
                "4.0.20-ls10",
                &["4.0.21-ls1", "4.0.20-ls12"],
                "4.0.21-ls1",
                true,
                "a newer version in the lsNN family is found",
            ),
            (
                "v1.2.3",
                &["v1.2.3", "v1.2.4", "1.2.0"],
                "v1.2.4",
                true,
                "a v prefix is the same version",
            ),
            (
                "1.2.3",
                &["v1.3.0", "1.2.3"],
                "v1.3.0",
                true,
                "a v-prefixed tag against a plain pin",
            ),
            (
                "1.10.0",
                &["1.9.9", "1.10.0", "1.2.0"],
                "1.10.0",
                false,
                "numbers compare as numbers, not as text",
            ),
            (
                "2.0.0",
                &["1.9.0", "1.8.0"],
                "1.9.0",
                false,
                "an older newest tag is current",
            ),
            (
                "1.2.3",
                &["1.2.3"],
                "1.2.3",
                false,
                "the equal tag is current",
            ),
            (
                "1.2",
                &["1.2.0", "1.2.1"],
                "1.2.1",
                true,
                "missing trailing parts count as zero",
            ),
            (
                "1.2.3",
                &["latest", "stable", "1.2.3", "main"],
                "1.2.3",
                false,
                "non-version tags are skipped",
            ),
            (
                "12.3ubu2604-ls55",
                &["12.4ubu2604-ls57", "12.4-ls57", "12.3ubu2604-ls55"],
                "12.4ubu2604-ls57",
                true,
                "a distro-suffixed family is its own family",
            ),
        ];
        for (pin, names, newest, update, why) in table {
            let v = judge(pin, &tags(names)).unwrap_or_else(|e| panic!("{why}: {e}"));
            assert_eq!(v.newest, *newest, "{why}");
            assert_eq!(v.update, *update, "{why}");
        }
    }

    #[test]
    fn no_honest_verdict_is_a_failure_never_a_guess() {
        for (pin, names, why) in [
            ("latest", &["1.0.0"][..], "an unparseable pin"),
            ("1.0.0-rc1", &["1.0.0"][..], "a prerelease pin"),
            ("1.0.0", &[][..], "no tags at all"),
            (
                "1.0.0",
                &["1.1.0-rc1", "latest", "1.1.0-alpine"][..],
                "no stable tag of the pin's family",
            ),
            ("", &["1.0.0"][..], "an empty pin"),
        ] {
            assert!(judge(pin, &tags(names)).is_err(), "{why}");
        }
    }

    // ---- the capability, over a fake source ----

    struct Fake {
        pages: HashMap<String, Result<String, String>>,
        asked: Mutex<Vec<String>>,
    }
    impl TagSource for Fake {
        fn get(&self, url: &str) -> Result<String, String> {
            self.asked.lock().unwrap().push(url.to_owned());
            self.pages
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err(format!("no fixture for {url}")))
        }
    }
    impl TagSource for std::sync::Arc<Fake> {
        fn get(&self, url: &str) -> Result<String, String> {
            (**self).get(url)
        }
    }

    const FIRST: &str = "https://hub.docker.com/v2/repositories/example/app/tags?page_size=100&ordering=last_updated";

    fn page(names: &[&str], next: Option<&str>) -> String {
        json!({
            "count": names.len(),
            "next": next,
            "results": names.iter().map(|n| json!({"name": n})).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn cap(pages: Vec<(&str, Result<String, String>)>) -> (RegistryTags, std::sync::Arc<Fake>) {
        let fake = std::sync::Arc::new(Fake {
            pages: pages.into_iter().map(|(u, p)| (u.to_owned(), p)).collect(),
            asked: Mutex::new(Vec::new()),
        });
        let c = RegistryTags::new("hub.docker.com", Box::new(fake.clone())).unwrap();
        (c, fake)
    }

    fn update_fact(out: &CallOutput) -> String {
        let f = out.facts.iter().find(|f| f.name == "update").unwrap();
        assert!(matches!(f.value, FactValue::Word(_)));
        f.rendered()
    }

    #[test]
    fn a_newer_stable_tag_gives_available_and_the_text_names_it() {
        let (c, _) = cap(vec![(
            FIRST,
            Ok(page(&["2.0.0-rc1", "1.5.0", "1.4.0"], None)),
        )]);
        let out = c.call(CALL, "example/app\n1.4.0").unwrap();
        assert_eq!(update_fact(&out), "available");
        assert_eq!(out.cost_micros, 0);
        assert!(out.result.contains("1.5.0"), "{}", out.result);
    }

    #[test]
    fn an_equal_or_older_newest_gives_current() {
        let (c, _) = cap(vec![(FIRST, Ok(page(&["1.4.0", "1.3.0"], None)))]);
        let out = c.call(CALL, "example/app\n1.4.0").unwrap();
        assert_eq!(update_fact(&out), "current");
    }

    #[test]
    fn a_bare_image_name_is_in_the_library_namespace() {
        let url = "https://hub.docker.com/v2/repositories/library/redis/tags?page_size=100&ordering=last_updated";
        let (c, _) = cap(vec![(url, Ok(page(&["7.2.5"], None)))]);
        assert_eq!(
            update_fact(&c.call(CALL, "redis\n7.2.4").unwrap()),
            "available"
        );
    }

    #[test]
    fn the_next_page_is_followed_on_the_same_host_and_a_foreign_next_is_a_failure() {
        let second = "https://hub.docker.com/v2/repositories/example/app/tags?page=2";
        let (c, fake) = cap(vec![
            (FIRST, Ok(page(&["1.4.0"], Some(second)))),
            (second, Ok(page(&["1.6.0"], None))),
        ]);
        assert_eq!(
            update_fact(&c.call(CALL, "example/app\n1.4.0").unwrap()),
            "available"
        );
        assert_eq!(fake.asked.lock().unwrap().len(), 2);

        let foreign = "https://evil.example.com/v2/x";
        let (c, fake) = cap(vec![(FIRST, Ok(page(&["1.4.0"], Some(foreign))))]);
        assert!(c.call(CALL, "example/app\n1.4.0").is_err());
        assert_eq!(
            fake.asked.lock().unwrap().len(),
            1,
            "the foreign page was never asked for"
        );
    }

    #[test]
    fn pages_are_bounded() {
        let mut pages = Vec::new();
        let urls: Vec<String> = (0..20)
            .map(|i| format!("https://hub.docker.com/v2/repositories/example/app/tags?page={i}"))
            .collect();
        pages.push((FIRST, Ok(page(&["1.0.0"], Some(urls[0].as_str())))));
        for i in 0..19 {
            pages.push((
                urls[i].as_str(),
                Ok(page(&["1.0.0"], Some(urls[i + 1].as_str()))),
            ));
        }
        let (c, fake) = cap(pages);
        let _ = c.call(CALL, "example/app\n1.0.0");
        assert!(fake.asked.lock().unwrap().len() <= 5);
    }

    #[test]
    fn a_fetch_error_or_a_parse_error_is_a_failed_item_with_no_facts() {
        let bad: Vec<Result<String, String>> = vec![
            Err("connection refused".into()),
            Ok("<html>rate limited</html>".into()),
            Ok(json!({"results": "no"}).to_string()),
            Ok(json!({"next": null}).to_string()),
            Ok(json!({"results": [{"name": 5}]}).to_string()),
            Ok(page(&["latest"], None)),
        ];
        for b in bad {
            let (c, _) = cap(vec![(FIRST, b.clone())]);
            let e = c.call(CALL, "example/app\n1.4.0").unwrap_err();
            assert!(e.facts.is_empty(), "{b:?}");
        }
    }

    #[test]
    fn a_bad_image_or_input_is_refused_before_any_request() {
        for input in [
            "example/app",
            "\n1.0.0",
            "example/app\n",
            "a/b/c\n1.0.0",
            "example/app?x=1\n1.0.0",
            "../etc\n1.0.0",
            "Example/App\n1.0.0",
            "example/app@sha256:ab\n1.0.0",
            "example/app\n1.0 0",
        ] {
            let (c, fake) = cap(vec![]);
            assert!(c.call(CALL, input).is_err(), "{input:?}");
            assert!(fake.asked.lock().unwrap().is_empty(), "{input:?}");
        }
    }

    #[test]
    fn registration_refuses_a_host_outside_the_allow_list() {
        struct Never;
        impl TagSource for Never {
            fn get(&self, _: &str) -> Result<String, String> {
                unreachable!()
            }
        }
        for host in [
            "evil.example.com",
            "hub.docker.com.evil.example",
            "",
            "ghcr.io",
            "hub.docker.com/v2",
        ] {
            assert!(RegistryTags::new(host, Box::new(Never)).is_err(), "{host}");
        }
        assert!(RegistryTags::new("hub.docker.com", Box::new(Never)).is_ok());
    }

    #[test]
    fn it_declares_observe_band_zero_cost_unenforced_and_closed_facts() {
        let (c, _) = cap(vec![]);
        let offers = c.offers();
        assert_eq!(offers.len(), 1);
        let o = &offers[0];
        assert_eq!(o.name, CALL);
        assert_eq!(o.band, Some(Reversibility::Observe));
        assert_eq!(o.max_cost_micros, 0);
        assert_eq!(o.budget, BudgetMode::Unenforced);
        assert_eq!(o.facts.len(), 1);
        assert_eq!(o.facts[0].name, "update");
    }
}

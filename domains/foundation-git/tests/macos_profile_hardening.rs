//! macOS sandbox profile hardening (weekly review B findings WF-01 and WF-04).
//!
//! The profile starts from `(allow default)`, so every operation it does not deny is a route out.
//! These tests run small shell scripts under the real profiles (`mac_profile`,
//! `mac_profile_real_agent`) with `sandbox-exec`, inside a throwaway lab directory. Nothing here
//! needs the network, a model or a paid API, and nothing touches the owner's clipboard (the
//! pasteboard test uses a private, uniquely named pasteboard). Every test cleans up in all paths
//! (lab directory, preference domains and their plists, the test applet's LaunchServices record).
//!
//! The tests run only on macOS; elsewhere each one prints a SKIP line with the reason.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use endora_foundation_git::{Reads, check_profile_path, mac_profile, mac_profile_real_agent};

const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/\
LaunchServices.framework/Support/lsregister";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A job directory name that is only text (nothing is created there).
const JOB: &str = concat!("/private", "/tmp/job");

/// Both profiles, by name: the real-agent one derives from the scripted one.
fn profiles(job: &Path) -> [(&'static str, String); 2] {
    [
        ("scripted", mac_profile(job, &Reads::default())),
        (
            "real-agent",
            mac_profile_real_agent(job, &Reads::default(), &[]),
        ),
    ]
}

fn skip(test: &str) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    eprintln!(
        "SKIP {test}: the macOS sandbox profile can only be exercised on macOS (sandbox-exec)"
    );
    true
}

/// A throwaway lab: `<tmp>/sec1-<unique>/job` (the confined job directory) and `.../out` (outside
/// it, where a marker must never appear). Dropped in every path, including a failed assert.
struct Lab {
    root: PathBuf,
    job: PathBuf,
    out: PathBuf,
    tag: String,
    domains: Vec<String>,
    applets: Vec<PathBuf>,
}

impl Lab {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let tag = format!(
            "{}{}{nanos:x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("sec1-{name}-{tag}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("job")).unwrap();
        std::fs::create_dir_all(root.join("out")).unwrap();
        let root = root.canonicalize().unwrap();
        Self {
            job: root.join("job"),
            out: root.join("out"),
            root,
            tag,
            domains: Vec::new(),
            applets: Vec::new(),
        }
    }

    /// Run `script` under `profile` (cwd = the job directory, HOME = the job directory, as Core
    /// sets it). Returns (exit code, combined output). Killed after 40 seconds.
    fn confined(&self, profile: &str, script: &str) -> (i32, String) {
        let mut child = Command::new("sandbox-exec")
            .args(["-p", profile, "/bin/sh", "-c", script])
            .current_dir(&self.job)
            .env_clear()
            // The `/usr/bin/git` shim asks xcrun for the toolchain; the profile cannot read an
            // Xcode under /Applications, so point it at the command line tools under /Library.
            .envs([
                (
                    OsStr::new("PATH"),
                    OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin"),
                ),
                (OsStr::new("HOME"), self.job.as_os_str()),
                (
                    OsStr::new("DEVELOPER_DIR"),
                    OsStr::new("/Library/Developer/CommandLineTools"),
                ),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > Duration::from_secs(40) {
                let _ = child.kill();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let o = child.wait_with_output().unwrap();
        let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&o.stderr));
        (status.code().unwrap_or(-1), text)
    }

    /// Wait up to `secs` for `marker` to appear.
    fn appears(marker: &Path, secs: u64) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(secs) {
            if marker.exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        marker.exists()
    }

    /// Run a command unconfined (set-up and clean-up only).
    fn plain(args: &[&str]) -> bool {
        Command::new(args[0])
            .args(&args[1..])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        for a in &self.applets {
            // Unregister the test applet from LaunchServices before its files go.
            let _ = Command::new(LSREGISTER)
                .arg("-u")
                .arg(a)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let prefs = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Preferences"));
        for d in &self.domains {
            let _ = Self::plain(&["defaults", "delete", d]);
            if let Some(p) = &prefs {
                let _ = std::fs::remove_file(p.join(format!("{d}.plist")));
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = &self.tag;
    }
}

/// The profile text carries every deny line (checked on every platform: it is plain text).
#[test]
fn both_profiles_carry_the_launchservices_preference_and_pasteboard_denials() {
    let job = Path::new(JOB);
    for (name, p) in profiles(job) {
        for line in [
            "(deny lsopen)",
            "(deny appleevent-send)",
            "(deny mach-lookup",
            "\"com.apple.coreservices.launchservicesd\"",
            "(deny user-preference-read)",
            "(deny user-preference-write)",
            "\"com.apple.cfprefsd.daemon\"",
            "\"com.apple.cfprefsd.agent\"",
            "\"com.apple.pasteboard.1\"",
        ] {
            assert!(p.contains(line), "the {name} profile lacks {line}");
        }
    }
}

/// WF-01: a confined process builds an AppleScript applet in its job directory and asks
/// LaunchServices to open it. The applet would be started by launchd, outside the sandbox. The
/// marker is outside the job directory, so it can only appear if the applet ran unconfined.
#[test]
fn a_confined_process_cannot_have_launchservices_start_an_unconfined_applet() {
    if skip("launchservices applet") {
        return;
    }
    for pi in 0..2 {
        let mut lab = Lab::new("lsopen");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let marker = lab.out.join(format!("marker-{}.txt", lab.tag));
        let app = lab.job.join(format!("z{}.app", lab.tag));
        lab.applets.push(app.clone());
        let script = format!(
            "osacompile -o '{app}' -e 'do shell script \"echo ran > {marker}\"' || exit 90\n\
             open -g '{app}'\n",
            app = app.display(),
            marker = marker.display()
        );
        let (_code, text) = lab.confined(&profile, &script);
        // Control: the confined process really could build the applet (so a held result is the
        // profile, not a broken osacompile).
        assert!(
            app.exists(),
            "{name}: the applet was not built, the test proves nothing: {text}"
        );
        assert!(
            !Lab::appears(&marker, 6),
            "{name}: LaunchServices started an applet outside the sandbox (marker present)"
        );
    }
}

/// The profile must not break the ordinary tools a scripted agent uses.
#[test]
fn the_hardened_profiles_still_run_sh_git_and_osacompile() {
    if skip("ordinary tools") {
        return;
    }
    for pi in 0..2 {
        let lab = Lab::new("tools");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let git = if Path::new("/Library/Developer/CommandLineTools/usr/bin/git").exists() {
            "git init -q . || exit 92\n\
             echo hello > f.txt && git add f.txt \
             && git -c user.name=a -c user.email=a@b commit -q -m y || exit 93\n"
        } else {
            eprintln!("note: no command line tools, the git step is skipped");
            ""
        };
        // Homebrew's Node reads its OpenSSL config under /opt/homebrew/etc, which is denied except
        // for that one directory: it must still start.
        let node = if Path::new("/opt/homebrew/bin/node").exists() {
            "/opt/homebrew/bin/node -e 'process.exit(0)' || exit 96\n"
        } else {
            ""
        };
        let script = format!(
            "/bin/sh -c 'echo sh-ok > sh.txt' || exit 91\n{git}\
             osacompile -o t.app -e 'return 1' || exit 95\n{node}"
        );
        let (code, text) = lab.confined(&profile, &script);
        assert_eq!(code, 0, "{name}: an ordinary tool failed ({code}): {text}");
        assert!(lab.job.join("sh.txt").exists(), "{name}: sh wrote nothing");
    }
}

/// WF-04: a confined process must not write a preference domain (a persistent write outside the
/// job directory, through the preferences daemon) and must not read another app's preferences.
#[test]
fn a_confined_process_cannot_write_or_read_user_preference_domains() {
    if skip("preference domains") {
        return;
    }
    for pi in 0..2 {
        let mut lab = Lab::new("prefs");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let read_domain = format!("sec1.lab.read{}", lab.tag);
        let write_domain = format!("sec1.lab.write{}", lab.tag);
        let canary = format!("canary{}", lab.tag);
        lab.domains.push(read_domain.clone());
        lab.domains.push(write_domain.clone());
        assert!(
            Lab::plain(&["defaults", "write", &read_domain, "k", &canary]),
            "set-up: could not write the throwaway read domain"
        );
        let script = format!(
            "defaults write {write_domain} k 1 >/dev/null 2>&1\n\
             defaults read {read_domain} k 2>/dev/null > read.txt\n"
        );
        let _ = lab.confined(&profile, &script);
        let read = std::fs::read_to_string(lab.job.join("read.txt")).unwrap_or_default();
        assert!(
            !read.contains(&canary),
            "{name}: the confined process read another app's preference value"
        );
        let plist = PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Preferences")
            .join(format!("{write_domain}.plist"));
        assert!(
            !plist.exists(),
            "{name}: the confined process wrote a preference domain outside the job directory"
        );
    }
}

/// WF-04: the pasteboard service must be unreachable. The test uses a private, uniquely named
/// pasteboard (never the general one), so the owner's clipboard is neither read nor written.
#[test]
fn a_confined_process_cannot_reach_the_pasteboard_service() {
    if skip("pasteboard") {
        return;
    }
    for pi in 0..2 {
        let lab = Lab::new("pboard");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let word = format!("pb{}", lab.tag);
        let script = format!(
            "osascript -l JavaScript -e 'ObjC.import(\"AppKit\"); \
             var pb = $.NSPasteboard.pasteboardWithUniqueName; \
             pb.setStringForType(\"{word}\", $.NSPasteboardTypeString); \
             \"roundtrip=\" + ObjC.unwrap(pb.stringForType($.NSPasteboardTypeString))' \
             > pb.txt 2>&1\n"
        );
        let _ = lab.confined(&profile, &script);
        let out = std::fs::read_to_string(lab.job.join("pb.txt")).unwrap_or_default();
        assert!(
            !out.contains(&format!("roundtrip={word}")),
            "{name}: a confined process used the pasteboard service: {out}"
        );
    }
}

/// WF-A-16: the owner's database and token are denied by name in the scripted profile too, each
/// with its SQLite sidecars, after the system allow and before the job allow, wherever they lie;
/// the prefixes where service data and configuration live are denied whole; Homebrew is no longer
/// readable as a whole. Plain text: checked on every platform.
#[test]
fn both_profiles_deny_the_database_and_token_by_name_and_the_service_data_prefixes() {
    let job = Path::new(JOB);
    let reads = Reads {
        deny: vec![
            "/usr/local/var/endora/core.db".into(),
            "/Library/Application Support/endora/owner.token".into(),
        ],
        ..Reads::default()
    };
    let profiles = [
        ("scripted", mac_profile(job, &reads)),
        ("real-agent", mac_profile_real_agent(job, &reads, &[])),
    ];
    for (name, p) in profiles {
        let after_system = p.find("(allow file-read-data (literal \"/\")").unwrap();
        let job_allow = p
            .rfind(&format!("(allow file-read-data (subpath \"{JOB}\")"))
            .unwrap();
        for f in [
            "/usr/local/var/endora/core.db",
            "/usr/local/var/endora/core.db-journal",
            "/usr/local/var/endora/core.db-wal",
            "/usr/local/var/endora/core.db-shm",
            "/Library/Application Support/endora/owner.token",
        ] {
            let at = p
                .rfind(&format!("(subpath \"{f}\")"))
                .unwrap_or_else(|| panic!("{name}: no deny for {f}"));
            assert!(
                after_system < at && at < job_allow,
                "{name}: {f} is not denied between the system allow and the job allow"
            );
        }
        for d in [
            "/usr/local/var",
            "/usr/local/etc",
            "/opt/homebrew/var",
            "/opt/homebrew/etc",
            "/Library/Application Support",
        ] {
            assert!(
                p.contains(&format!("(subpath \"{d}\")")),
                "{name}: {d} is not denied"
            );
        }
        assert!(
            !p.contains("(subpath \"/opt/homebrew\")"),
            "{name}: all of /opt/homebrew is readable"
        );
    }
}

/// WF-A-25: a path can never end the profile's string literal and add rules of its own. Quote and
/// backslash are escaped; a newline (or any control character) is refused; parentheses sit inside
/// a literal and are harmless. One check per hostile character.
#[test]
fn hostile_characters_in_a_path_cannot_alter_the_profile() {
    for (what, bad) in [
        ("double quote", "/x\") (allow file-write* (subpath \"/"),
        ("backslash", "/x\\\") (allow file-write* (subpath \"/"),
        ("parenthesis", "/x) (allow file-write* (subpath \"/"),
    ] {
        let p = mac_profile(Path::new(bad), &Reads::default());
        // The profile text must not contain the injected rule outside a string literal: every
        // quote that came from the path is preceded by a backslash.
        assert!(
            !p.contains("(allow file-write* (subpath \"/\")"),
            "{what}: the path injected an allow rule: {p}"
        );
        let real = mac_profile_real_agent(Path::new(bad), &Reads::default(), &[bad.into()]);
        assert!(
            !real.contains("(allow file-write* (subpath \"/\")"),
            "{what}: real-agent"
        );
    }
    assert!(check_profile_path(Path::new("/ok/with space (1)")).is_ok());
    assert!(
        check_profile_path(Path::new("/bad\nnewline")).is_err(),
        "newline"
    );
    assert!(
        check_profile_path(Path::new("/bad\rreturn")).is_err(),
        "carriage return"
    );
    assert!(check_profile_path(Path::new("/bad\0nul")).is_err(), "nul");
}

/// WF-A-25 behaviour: a job directory whose name holds a quote, a backslash and parentheses is
/// still confined to itself (writes inside work, a write to a sibling named in the path is denied)
/// and the profile is not malformed.
#[test]
fn a_job_directory_named_with_hostile_characters_is_confined_to_itself() {
    if skip("hostile directory name") {
        return;
    }
    let mut lab = Lab::new("hostile");
    let sibling = lab.out.clone();
    let hostile = lab.root.join(format!(
        "job\") (allow file-write* (subpath \"{}\") (x\\y",
        sibling.display()
    ));
    std::fs::create_dir_all(&hostile).unwrap();
    lab.job = hostile.canonicalize().unwrap();
    let profile = mac_profile(&lab.job, &Reads::default());
    let marker = sibling.join("m.txt");
    let script = format!(
        "echo ok > inside.txt || exit 71\necho x > '{}' && exit 72\nexit 0\n",
        marker.display()
    );
    let (code, text) = lab.confined(&profile, &script);
    assert_eq!(code, 0, "exit {code}: {text}");
    assert!(
        lab.job.join("inside.txt").exists(),
        "no write inside the job directory"
    );
    assert!(
        !marker.exists(),
        "a write outside the job directory got through"
    );
}

/// The start-up note: which paths lie under a prefix the profile can read.
#[test]
fn the_startup_note_names_a_readable_prefix_and_stays_quiet_for_denied_ones() {
    use endora_foundation_git::mac_profile_can_read;
    assert!(mac_profile_can_read(Path::new("/opt/homebrew/share/endora/c.db")).is_some());
    assert!(mac_profile_can_read(Path::new("/usr/local/bin/x")).is_some());
    assert!(mac_profile_can_read(Path::new("/opt/homebrew/var/c.db")).is_none());
    assert!(mac_profile_can_read(Path::new("/usr/local/var/c.db")).is_none());
    assert!(mac_profile_can_read(Path::new(concat!("/Us", "ers/x/c.db"))).is_none());
    assert!(mac_profile_can_read(Path::new("/srv/c.db")).is_none());
}

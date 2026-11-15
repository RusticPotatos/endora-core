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

use endora_foundation_git::{
    Reads, canonical_deny_paths, check_deny_outside_writable, check_deny_path, check_profile_path,
    mac_profile, mac_profile_real_agent,
};

const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/\
LaunchServices.framework/Support/lsregister";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A job directory name that is only text (nothing is created there).
const JOB: &str = "/srv/sandbox-job";

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

/// A SKIP that is not about the platform: with `ENDORA_STRICT_TESTS=1` (`scripts/gate.sh` sets it on macOS) the
/// test fails instead of passing silently.
fn cannot_run(test: &str, why: &str) {
    eprintln!("SKIP {test}: {why}");
    assert!(
        std::env::var_os("ENDORA_STRICT_TESTS").is_none(),
        "ENDORA_STRICT_TESTS is set and {test} cannot run: {why}"
    );
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

    /// The same script with no sandbox (the control: it must succeed, so a held result is the
    /// profile and not a broken tool). Returns (exit code, combined output).
    fn unconfined(&self, script: &str) -> (i32, String) {
        let o = Command::new("/bin/sh")
            .args(["-c", script])
            .current_dir(&self.job)
            .output()
            .unwrap();
        let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&o.stderr));
        (o.status.code().unwrap_or(-1), text)
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
        // Control: unconfined, the same script writes the domain and reads the canary.
        let plist = PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Preferences")
            .join(format!("{write_domain}.plist"));
        lab.unconfined(&script);
        let control = std::fs::read_to_string(lab.job.join("read.txt")).unwrap_or_default();
        assert!(
            control.contains(&canary),
            "control: the unconfined read failed"
        );
        assert!(plist.exists(), "control: the unconfined write failed");
        let _ = Lab::plain(&["defaults", "delete", &write_domain]);
        let _ = std::fs::remove_file(&plist);
        let _ = std::fs::remove_file(lab.job.join("read.txt"));
        let _ = lab.confined(&profile, &script);
        let read = std::fs::read_to_string(lab.job.join("read.txt")).unwrap_or_default();
        assert!(
            !read.contains(&canary),
            "{name}: the confined process read another app's preference value"
        );
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
        // Control: unconfined, the same script round-trips on its private pasteboard.
        lab.unconfined(&script);
        let control = std::fs::read_to_string(lab.job.join("pb.txt")).unwrap_or_default();
        assert!(
            control.contains(&format!("roundtrip={word}")),
            "control: the unconfined pasteboard script failed: {control}"
        );
        let _ = std::fs::remove_file(lab.job.join("pb.txt"));
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
            "/usr/local/var/endora/core.db.holders",
            "/Library/Application Support/endora/owner.token",
        ] {
            let at = p
                .rfind(&format!("(subpath \"{f}\")"))
                .unwrap_or_else(|| panic!("{name}: no deny for {f}"));
            assert!(
                after_system < at && at > job_allow,
                "{name}: {f} is not denied after the system allow and after the final job allow (the last rule wins)"
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
    let home = std::env::var("HOME").unwrap_or_default();
    assert!(mac_profile_can_read(&Path::new(&home).join("c.db")).is_none());
    assert!(mac_profile_can_read(Path::new("/srv/c.db")).is_none());
}

// ---- round 2: review findings R1 (canonical paths), the order of the by-name deny, R2, R3 ----

/// R1: the sandbox matches canonical paths, so the by-name deny must be built from them. Every
/// spelling of a path (relative, `.`, `..`, `//`, a symlinked directory or file) gives the same
/// canonical path first, and keeps the other spellings after it.
#[test]
fn deny_paths_are_made_absolute_and_canonical_before_the_profile_sees_them() {
    let lab = Lab::new("canon");
    let real = lab.root.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let db = real.join("core.db");
    std::fs::write(&db, "dummy").unwrap();
    let want = db.canonicalize().unwrap();
    let first = |typed: PathBuf| canonical_deny_paths(&[typed]).remove(0);
    // dot, dot-dot and double slash
    let root = lab.root.display();
    assert_eq!(
        first(PathBuf::from(format!("{root}/./real/../real//core.db"))),
        want
    );
    assert_eq!(
        first(PathBuf::from(format!("{root}//real/./core.db"))),
        want
    );
    // a symlinked directory and a symlinked file
    let dir_link = lab.root.join("dirlink");
    std::os::unix::fs::symlink(&real, &dir_link).unwrap();
    assert_eq!(first(dir_link.join("core.db")), want);
    let file_link = lab.root.join("filelink.db");
    std::os::unix::fs::symlink(&db, &file_link).unwrap();
    assert_eq!(first(file_link.clone()), want);
    let spellings = canonical_deny_paths(&[file_link.clone()]);
    assert!(
        spellings.contains(&file_link),
        "the typed spelling is kept: {spellings:?}"
    );
    // a file that does not exist yet: its directory is canonicalised, the name is kept
    assert_eq!(
        first(dir_link.join("new.db")),
        real.canonicalize().unwrap().join("new.db")
    );
    // a relative path is made absolute against the current directory
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_eq!(
        first(PathBuf::from("relative-name.db")),
        cwd.join("relative-name.db")
    );
    // the profile builder refuses what it could not match
    assert!(check_deny_path(Path::new("relative-name.db")).is_err());
    assert!(check_deny_path(Path::new("/a/./b")).is_err());
    assert!(check_deny_path(Path::new("/a/../b")).is_err());
    assert!(check_deny_path(Path::new("/a//b")).is_err());
    assert!(check_deny_path(&want).is_ok());
}

/// R1 end to end: a file under a prefix the profile can read (Homebrew's `share`) stays unreadable
/// when the owner typed its path in any of the odd ways, because the deny is built from the
/// canonical path. The control shows that the typed path alone would not have matched.
#[test]
fn a_database_typed_by_a_non_canonical_path_is_still_denied_by_name() {
    if skip("non-canonical deny paths") {
        return;
    }
    let share = Path::new("/opt/homebrew/share");
    let probe = share.join(format!("sec1-probe-{}", std::process::id()));
    if std::fs::create_dir(&probe).is_err() {
        cannot_run(
            "non-canonical deny paths",
            "/opt/homebrew/share is not writable here",
        );
        return;
    }
    let _ = std::fs::remove_dir(&probe);
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    for pi in 0..2 {
        let lab = Lab::new("noncanon");
        let (name, _) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let dir = share.join(format!("sec1-r1-{}", lab.tag));
        let _clean = Cleanup(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("core.db");
        std::fs::write(&db, format!("dummy-{}", lab.tag)).unwrap();
        let dir_link = lab.root.join("dirlink");
        std::os::unix::fs::symlink(&dir, &dir_link).unwrap();
        let file_link = lab.root.join("filelink.db");
        std::os::unix::fs::symlink(&db, &file_link).unwrap();
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let up = "../".repeat(cwd.components().count() - 1);
        let relative = PathBuf::from(format!(
            "{up}opt/homebrew/share/sec1-r1-{}/core.db",
            lab.tag
        ));
        let odd = PathBuf::from(format!("{}/./x/..//core.db", dir.display()));
        let cat = format!(
            "cat '{}' > got.txt 2> got.err; echo $? > got.exit\n",
            db.display()
        );
        // Control: unconfined, the file is readable.
        assert_eq!(lab.unconfined(&cat).0, 0);
        let _ = std::fs::remove_file(lab.job.join("got.txt"));
        for (what, typed) in [
            ("relative", relative),
            ("dot, dot-dot and double slash", odd),
            ("symlinked directory", dir_link.join("core.db")),
            ("symlinked file", file_link),
        ] {
            let run = |deny: Vec<PathBuf>| {
                let reads = Reads {
                    deny,
                    ..Reads::default()
                };
                let profile = if pi == 0 {
                    mac_profile(&lab.job, &reads)
                } else {
                    mac_profile_real_agent(&lab.job, &reads, &[])
                };
                let _ = std::fs::remove_file(lab.job.join("got.exit"));
                lab.confined(&profile, &cat);
                std::fs::read_to_string(lab.job.join("got.exit")).unwrap_or_default()
            };
            let canonical = run(canonical_deny_paths(std::slice::from_ref(&typed)));
            assert_ne!(
                canonical.trim(),
                "0",
                "{name}/{what}: the database was readable"
            );
            // The typed spelling alone (what the code did before): only absolute spellings can
            // even be written into a profile, and the symlinked ones do not match.
            if what.starts_with("symlinked") {
                let alone = run(vec![typed]);
                assert_eq!(
                    alone.trim(),
                    "0",
                    "{name}/{what}: control: the typed path alone was expected to leak"
                );
            }
        }
    }
}

/// The by-name deny is the last rule: a database inside a named read directory stays unreadable,
/// and since that directory is read-only it cannot be renamed away either; the job can still
/// write and read its own files. (A database inside the job directory is refused at start, see the
/// next test: a deny cannot hold against a rename where the job may write.)
#[test]
fn the_by_name_deny_wins_inside_a_read_directory_and_the_job_keeps_its_files() {
    if skip("deny order") {
        return;
    }
    for pi in 0..2 {
        let lab = Lab::new("order");
        let (name, _) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let readdir = lab.root.join("readdir");
        std::fs::create_dir_all(&readdir).unwrap();
        let read_db = readdir.join("core.db");
        let read_other = readdir.join("other.txt");
        std::fs::write(&read_db, "dummy").unwrap();
        std::fs::write(&read_other, "dummy-other").unwrap();
        let reads = Reads {
            dirs: vec![readdir.canonicalize().unwrap()],
            deny: canonical_deny_paths(&[read_db.clone()]),
            ..Reads::default()
        };
        let profile = if pi == 0 {
            mac_profile(&lab.job, &reads)
        } else {
            mac_profile_real_agent(&lab.job, &reads, &[])
        };
        let script = format!(
            "cat '{r}' > /dev/null 2>&1; echo $? > read.exit\n\
             mv '{r}' '{d}/moved' > /dev/null 2>&1; echo $? > mv.exit\n\
             cat '{d}/moved' > /dev/null 2>&1; echo $? > moved.exit\n\
             cat '{o}' > other.txt 2>&1; echo $? > other.exit\n\
             echo mine > own.txt; echo $? > write.exit\n\
             cat own.txt > own.copy 2>&1; echo $? > own.exit\n",
            r = read_db.display(),
            d = readdir.display(),
            o = read_other.display()
        );
        lab.confined(&profile, &script);
        let n = |f: &str| {
            std::fs::read_to_string(lab.job.join(f))
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        assert_ne!(n("read.exit"), "0", "{name}: the database was readable");
        assert_ne!(n("mv.exit"), "0", "{name}: the read directory was writable");
        assert_ne!(
            n("moved.exit"),
            "0",
            "{name}: the database was readable after a rename"
        );
        assert_eq!(
            n("other.exit"),
            "0",
            "{name}: another file in the read directory must stay readable"
        );
        assert_eq!(
            n("write.exit"),
            "0",
            "{name}: the job could not write its own file"
        );
        assert_eq!(
            n("own.exit"),
            "0",
            "{name}: the job could not read its own file"
        );
    }
}

/// Review round 3, blocker 2: a denied path inside a directory the job may write cannot hold (the
/// job renames the file, or its parent, to a name that is not denied), so the layout is refused.
/// The Data-volume prefix does not hide it. A path outside the writable directories is fine.
#[test]
fn a_denied_path_inside_the_job_directory_is_refused_not_claimed() {
    let job = PathBuf::from(JOB);
    let inside = job.join("data/core.db");
    let data = |p: &Path| PathBuf::from(format!("/System/Volumes/Data{}", p.display()));
    for bad in [
        inside.clone(),
        job.join("core.db"),
        data(&inside),
        job.clone(),
    ] {
        let e = check_deny_outside_writable(&[bad.clone()], &[&job]).unwrap_err();
        assert!(
            e.contains("refusing to start") && e.contains("rename"),
            "{bad:?}: {e}"
        );
    }
    assert!(check_deny_outside_writable(&[PathBuf::from("/srv/owner/core.db")], &[&job]).is_ok());
    assert!(
        check_deny_outside_writable(&[PathBuf::from("/srv/sandbox-job2/core.db")], &[&job]).is_ok()
    );
}

/// Review round 3, blocker 1 (unit): the Data-volume spelling and the plain spelling of a path
/// both end up denied, with the plain one first when they are the same file; a path with a `..`
/// segment yields only spellings the profile accepts (finding 3).
#[test]
fn both_volume_spellings_are_denied_and_every_spelling_is_acceptable_to_the_profile() {
    let lab = Lab::new("firm");
    let db = lab.root.join("core.db");
    std::fs::write(&db, "dummy").unwrap();
    let plain = db.canonicalize().unwrap();
    let data = PathBuf::from(format!("/System/Volumes/Data{}", plain.display()));
    for typed in [data.clone(), plain.clone()] {
        let got = canonical_deny_paths(&[typed]);
        assert_eq!(got[0], plain, "the plain spelling comes first: {got:?}");
        assert!(
            got.contains(&data),
            "the Data-volume spelling is denied too: {got:?}"
        );
    }
    // a file that does not exist yet
    let new_plain = plain.with_file_name("new.db");
    let got = canonical_deny_paths(&[PathBuf::from(format!(
        "/System/Volumes/Data{}",
        new_plain.display()
    ))]);
    assert_eq!(got[0], new_plain, "{got:?}");
    // an absolute path with `..`, and the profile text carries both spellings and the sidecars
    let dotdot = PathBuf::from(format!("{}/x/../core.db", lab.root.display()));
    let got = canonical_deny_paths(&[dotdot]);
    assert!(got.iter().all(|p| check_deny_path(p).is_ok()), "{got:?}");
    let p = mac_profile(
        &lab.job,
        &Reads {
            deny: got,
            ..Reads::default()
        },
    );
    for f in [
        plain.display().to_string(),
        format!("{}-wal", data.display()),
        format!("{}-journal", plain.display()),
        format!("{}.holders", plain.display()),
        format!("{}.holders", data.display()),
    ] {
        assert!(p.contains(&format!("(subpath \"{f}\")")), "no deny for {f}");
    }
}

/// Review round 3, blocker 1 (real sandbox, both profiles): a file under a readable prefix typed
/// with the Data-volume prefix is unreadable through the plain spelling and through the prefixed
/// one. Control: unconfined it reads.
#[test]
fn a_database_typed_with_the_data_volume_prefix_is_denied_under_both_spellings() {
    if skip("data volume spelling") {
        return;
    }
    let share = Path::new("/opt/homebrew/share");
    let probe = share.join(format!("sec1-probe-firm-{}", std::process::id()));
    if std::fs::create_dir(&probe).is_err() {
        cannot_run(
            "data volume spelling",
            "/opt/homebrew/share is not writable here",
        );
        return;
    }
    let _ = std::fs::remove_dir(&probe);
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    for pi in 0..2 {
        let lab = Lab::new("firmrw");
        let (name, _) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let dir = share.join(format!("sec1-firm-{}", lab.tag));
        let _clean = Cleanup(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("core.db");
        std::fs::write(&db, "dummy").unwrap();
        let typed = PathBuf::from(format!("/System/Volumes/Data{}", db.display()));
        let cat = |p: &Path| {
            format!(
                "cat '{}' > /dev/null 2>&1; echo $? > got.exit\n",
                p.display()
            )
        };
        assert_eq!(lab.unconfined(&cat(&db)).0, 0, "control");
        let reads = Reads {
            deny: canonical_deny_paths(&[typed.clone()]),
            ..Reads::default()
        };
        let profile = if pi == 0 {
            mac_profile(&lab.job, &reads)
        } else {
            mac_profile_real_agent(&lab.job, &reads, &[])
        };
        for (what, p) in [("plain", db.clone()), ("data volume", typed)] {
            let _ = std::fs::remove_file(lab.job.join("got.exit"));
            lab.confined(&profile, &cat(&p));
            let code = std::fs::read_to_string(lab.job.join("got.exit")).unwrap_or_default();
            assert_ne!(code.trim(), "0", "{name}: the {what} spelling was readable");
        }
    }
}

/// R3: the Shortcuts service is unreachable (shortcuts run outside the sandbox, as the owner).
/// Lists only; no shortcut is ever run. Needs at least one shortcut, so the control can see a
/// difference; otherwise it says SKIP.
#[test]
fn a_confined_process_cannot_list_the_owners_shortcuts() {
    if skip("shortcuts") {
        return;
    }
    for pi in 0..2 {
        let lab = Lab::new("shortcuts");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let (code, listed) = lab.unconfined("shortcuts list");
        if code != 0 || listed.trim().is_empty() {
            cannot_run(
                "shortcuts",
                "`shortcuts list` gives nothing unconfined, nothing to hide",
            );
            return;
        }
        let (_, confined) = lab.confined(&profile, "shortcuts list 2>/dev/null");
        assert!(
            confined.trim().is_empty(),
            "{name}: a confined process listed the owner's shortcuts"
        );
    }
}

/// R2 speed bump: `launchctl` itself cannot be run. This is NOT a boundary (see TRUST-BOUNDARY.md:
/// launchd job environments stay readable by other routes); the test only pins the deny line.
#[test]
fn launchctl_cannot_be_run_from_the_profile_but_it_runs_unconfined() {
    if skip("launchctl") {
        return;
    }
    for pi in 0..2 {
        let lab = Lab::new("launchctl");
        let (name, profile) = profiles(&lab.job).into_iter().nth(pi).unwrap();
        let script = "/bin/launchctl print-disabled gui/$(id -u) > /dev/null 2>&1";
        assert_eq!(
            lab.unconfined(script).0,
            0,
            "control: launchctl failed unconfined"
        );
        assert_ne!(lab.confined(&profile, script).0, 0, "{name}: launchctl ran");
    }
}

/// Review of the integrated branch, N1: the lease's holder files (`<db>.holders/`) are denied by
/// name like the database's other sidecars. With the database under a readable prefix, a confined
/// agent can neither list the directory nor open a holder file (both spellings, both profiles),
/// while the Core process (unconfined) still keeps the lease: a second process leaves the live
/// item alone, and takes it over once the holder is gone. Control: unconfined it lists and opens.
#[test]
fn a_confined_agent_cannot_list_or_open_the_holders_directory_and_the_lease_still_works() {
    use endora_foundation::application::{Core, NewGoal};
    use endora_foundation::domain::{Actor, Criterion};
    use endora_foundation::infrastructure::ManualClock;
    use std::sync::Arc;
    const TOKEN: &str = concat!(
        "0123456789abcdef",
        "0123456789abcdef",
        "0123456789abcdef",
        "0123456789abcdef"
    );
    if skip("holders directory") {
        return;
    }
    let share = Path::new("/opt/homebrew/share");
    if std::fs::create_dir(share.join(format!("rec2-probe-{}", std::process::id()))).is_err() {
        cannot_run(
            "holders directory",
            "/opt/homebrew/share is not writable here",
        );
        return;
    }
    let _ = std::fs::remove_dir(share.join(format!("rec2-probe-{}", std::process::id())));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    for pi in 0..2 {
        let lab = Lab::new("holders");
        let dir = share.join(format!("rec2-holders-{}", lab.tag));
        let _clean = Cleanup(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("core.db");
        let open = || Core::open(db.to_str().unwrap(), Arc::new(ManualClock::new(1))).unwrap();
        let a = open();
        a.enroll_owner(TOKEN).unwrap();
        let owner = a.authenticate_owner(TOKEN).unwrap();
        let g = a
            .add_goal(
                &owner,
                NewGoal {
                    title: "t".into(),
                    why: "w".into(),
                    criteria: vec![Criterion {
                        text: "c".into(),
                        check: None,
                    }],
                    scope: vec![],
                    tier: endora_kernel::Tier::Internal,
                    spend_cap_micros: 0,
                },
            )
            .unwrap();
        let item = a.add_item(g, "held").unwrap();
        a.start(&Actor::capability("runner").unwrap(), item)
            .unwrap();
        let holders = dir.join("core.db.holders");
        let holder_file = std::fs::read_dir(&holders)
            .unwrap()
            .flatten()
            .next()
            .expect("the live item's holder file")
            .path();
        let data = |p: &Path| PathBuf::from(format!("/System/Volumes/Data{}", p.display()));
        let script = |d: &Path, f: &Path| {
            format!(
                "ls '{d}' > got.ls 2>&1; echo $? > got.lsexit; cat '{f}' > /dev/null 2>&1; echo $? > got.catexit\n",
                d = d.display(),
                f = f.display()
            )
        };
        // Control: unconfined, both work (so a denial below is the profile, not a broken tool).
        let (_, _) = lab.unconfined(&script(&holders, &holder_file));
        let exit = |n: &str| std::fs::read_to_string(lab.job.join(n)).unwrap_or_default();
        assert_eq!(exit("got.lsexit").trim(), "0", "control: ls");
        assert!(
            std::fs::read_to_string(lab.job.join("got.ls"))
                .unwrap()
                .contains(holder_file.file_name().unwrap().to_str().unwrap()),
            "control: ls shows the holder file"
        );
        assert_eq!(exit("got.catexit").trim(), "0", "control: open");
        let reads = Reads {
            deny: canonical_deny_paths(&[db.clone()]),
            ..Reads::default()
        };
        let (name, profile) = if pi == 0 {
            ("scripted", mac_profile(&lab.job, &reads))
        } else {
            ("real-agent", mac_profile_real_agent(&lab.job, &reads, &[]))
        };
        for (what, d, f) in [
            ("plain", holders.clone(), holder_file.clone()),
            ("data volume", data(&holders), data(&holder_file)),
        ] {
            for n in ["got.ls", "got.lsexit", "got.catexit"] {
                let _ = std::fs::remove_file(lab.job.join(n));
            }
            lab.confined(&profile, &script(&d, &f));
            assert_ne!(
                exit("got.lsexit").trim(),
                "0",
                "{name}: the {what} holders directory was listable"
            );
            assert!(
                !std::fs::read_to_string(lab.job.join("got.ls"))
                    .unwrap_or_default()
                    .contains(holder_file.file_name().unwrap().to_str().unwrap()),
                "{name}: the {what} listing showed the holder file"
            );
            assert_ne!(
                exit("got.catexit").trim(),
                "0",
                "{name}: the {what} holder file was openable"
            );
        }
        // The lease is untouched: while `a` lives a second process leaves the item alone; once
        // `a` is gone it takes the item over.
        let b = open();
        assert!(
            b.recover().unwrap().is_empty(),
            "{name}: the live holder's item was recovered"
        );
        drop(a);
        assert_eq!(
            b.recover().unwrap().len(),
            1,
            "{name}: the dead holder's item was not recovered"
        );
    }
}

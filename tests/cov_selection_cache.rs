//! Integration coverage for the selection / cache / dry-run code paths in
//! `src/lib.rs`. Every test spawns the real `wb` binary (so llvm-cov
//! instruments it) and isolates `~/.wb` by pointing `HOME` at a fresh
//! `tempfile::tempdir()`. Workbooks are written into tempdirs.

use std::path::PathBuf;
use std::process::Command;

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// A workbook written into its own tempdir, with an isolated HOME. Keeping the
/// `TempDir`s alive for the test body keeps the files on disk.
struct Fixture {
    home: tempfile::TempDir,
    dir: tempfile::TempDir,
    path: PathBuf,
}

fn fixture(name: &str, content: &str) -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    Fixture { home, dir, path }
}

impl Fixture {
    /// `wb run <path> <extra args...>` with the isolated HOME.
    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new(wb_binary());
        cmd.arg("run")
            .arg(self.path.to_str().unwrap())
            .args(args)
            .env("HOME", self.home.path())
            .current_dir(self.dir.path());
        cmd.output().expect("failed to spawn wb")
    }
}

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}
fn combined(o: &std::process::Output) -> String {
    format!("{}{}", stdout(o), stderr(o))
}

/// Three blocks: login / migrate(.smoke) / smoke(.smoke). Distinct echo
/// markers so we can assert which blocks actually ran from stdout.
const THREE_BLOCKS: &str = "\
```bash {#login}
echo LOGINMARK
```

```bash {#migrate .smoke}
echo MIGRATEMARK
```

```bash {#smoke .smoke}
echo SMOKEMARK
```
";

// ---------------------------------------------------------------------------
// --only
// ---------------------------------------------------------------------------

#[test]
fn only_runs_just_the_selected_block() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "migrate"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(
        !out.contains("LOGINMARK"),
        "login should be skipped:\n{out}"
    );
    assert!(
        !out.contains("SMOKEMARK"),
        "smoke should be skipped:\n{out}"
    );
}

#[test]
fn only_emits_selection_skip_for_other_blocks() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "migrate"]);
    let err = stderr(&o);
    // selection skips are printed to stderr with their reason.
    assert!(err.contains("skipped"), "stderr:\n{err}");
    assert!(
        err.contains("outside --only/--from/--until range"),
        "stderr:\n{err}"
    );
    // selective runs disable checkpointing and say so.
    assert!(err.contains("selective run"), "stderr:\n{err}");
}

#[test]
fn only_unknown_step_id_is_usage_error_before_running() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "nope"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    let err = stderr(&o);
    assert!(err.contains("not found in workbook"), "stderr:\n{err}");
    // No block ran.
    assert!(!combined(&o).contains("MIGRATEMARK"));
}

// ---------------------------------------------------------------------------
// --from / --until / bounded range
// ---------------------------------------------------------------------------

#[test]
fn from_starts_mid_workbook() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--from", "migrate"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains("LOGINMARK"), "out:\n{out}");
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(out.contains("SMOKEMARK"), "out:\n{out}");
}

#[test]
fn until_stops_after_named_block() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--until", "migrate"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("LOGINMARK"), "out:\n{out}");
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(!out.contains("SMOKEMARK"), "out:\n{out}");
}

#[test]
fn from_until_bounded_range() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--from", "migrate", "--until", "migrate"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains("LOGINMARK"), "out:\n{out}");
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(!out.contains("SMOKEMARK"), "out:\n{out}");
}

#[test]
fn from_until_empty_range_is_usage_error() {
    // --from smoke (pos 3) .. --until login (pos 1) => empty range.
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--from", "smoke", "--until", "login"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    assert!(
        stderr(&o).contains("selection range is empty"),
        "stderr:\n{}",
        stderr(&o)
    );
}

// ---------------------------------------------------------------------------
// --tag
// ---------------------------------------------------------------------------

#[test]
fn tag_runs_only_matching_class() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--tag", "smoke"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains("LOGINMARK"), "out:\n{out}");
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(out.contains("SMOKEMARK"), "out:\n{out}");
}

#[test]
fn tag_repeated_is_union_of_classes() {
    let wb = "\
```bash {#a .red}
echo REDMARK
```

```bash {#b .blue}
echo BLUEMARK
```

```bash {#c .green}
echo GREENMARK
```
";
    let f = fixture("wb.md", wb);
    let o = f.run(&["--tag", "red", "--tag", "blue"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("REDMARK"), "out:\n{out}");
    assert!(out.contains("BLUEMARK"), "out:\n{out}");
    assert!(
        !out.contains("GREENMARK"),
        "green should be skipped:\n{out}"
    );
}

#[test]
fn tag_matching_no_block_is_usage_error() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--tag", "nomatch"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    assert!(
        stderr(&o).contains("matches no block"),
        "stderr:\n{}",
        stderr(&o)
    );
    assert!(!combined(&o).contains("MIGRATEMARK"));
}

// ---------------------------------------------------------------------------
// conflicts
// ---------------------------------------------------------------------------

#[test]
fn only_conflicts_with_from_at_parse() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "login", "--from", "migrate"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    // clap's conflict message; nothing executed.
    assert!(
        stderr(&o).contains("cannot be used with"),
        "stderr:\n{}",
        stderr(&o)
    );
    assert!(!combined(&o).contains("LOGINMARK"));
}

#[test]
fn only_conflicts_with_tag_at_parse() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "login", "--tag", "smoke"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    assert!(
        stderr(&o).contains("cannot be used with"),
        "stderr:\n{}",
        stderr(&o)
    );
}

#[test]
fn only_conflicts_with_changed_at_parse() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "login", "--changed"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
}

#[test]
fn selection_conflicts_with_checkpoint() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "migrate", "--checkpoint", "ck1"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    assert!(
        stderr(&o).contains("cannot be combined with --checkpoint"),
        "stderr:\n{}",
        stderr(&o)
    );
    assert!(!combined(&o).contains("MIGRATEMARK"));
}

#[test]
fn tag_selection_conflicts_with_checkpoint() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--tag", "smoke", "--checkpoint", "ck2"]);
    assert_eq!(o.status.code(), Some(2), "combined:\n{}", combined(&o));
    assert!(
        stderr(&o).contains("cannot be combined with --checkpoint"),
        "stderr:\n{}",
        stderr(&o)
    );
}

// ---------------------------------------------------------------------------
// --default-block-timeout interplay with selection
// ---------------------------------------------------------------------------

#[test]
fn default_block_timeout_composes_with_selection() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--only", "migrate", "--default-block-timeout", "30s"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("MIGRATEMARK"), "out:\n{out}");
    assert!(!out.contains("LOGINMARK"), "out:\n{out}");
    assert!(!out.contains("SMOKEMARK"), "out:\n{out}");
}

// ---------------------------------------------------------------------------
// --dry-run
// ---------------------------------------------------------------------------

const PROFILE_WB: &str = "\
---
params:
  region:
    type: enum
    one_of: [us-east-1, eu-west-1]
    default: us-east-1
profiles:
  prod:
    region: eu-west-1
---
```bash {#touchit}
touch SIDEEFFECT.txt
echo region=$region
```
";

#[test]
fn dry_run_prints_plan_and_has_no_side_effects() {
    let f = fixture("wb.md", PROFILE_WB);
    let o = f.run(&["--dry-run"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("dry run:"), "out:\n{out}");
    assert!(out.contains("plan:"), "out:\n{out}");
    assert!(out.contains("would run"), "out:\n{out}");
    // The block was not executed: the side-effect file must not exist. (We
    // can't key off the echoed `region=...` marker since the plan header also
    // prints the resolved param value.)
    assert!(
        !f.dir.path().join("SIDEEFFECT.txt").exists(),
        "dry run created a side-effect file"
    );
}

#[test]
fn dry_run_applies_profile() {
    let f = fixture("wb.md", PROFILE_WB);
    let o = f.run(&["--dry-run", "--profile", "prod"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("params: region=eu-west-1"), "out:\n{out}");
    assert!(!f.dir.path().join("SIDEEFFECT.txt").exists());
}

#[test]
fn dry_run_applies_param_override() {
    let f = fixture("wb.md", PROFILE_WB);
    let o = f.run(&["--dry-run", "--param", "region=eu-west-1"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    assert!(
        stdout(&o).contains("params: region=eu-west-1"),
        "out:\n{}",
        stdout(&o)
    );
}

#[test]
fn dry_run_marks_selection_skips_in_plan() {
    let f = fixture("wb.md", THREE_BLOCKS);
    let o = f.run(&["--dry-run", "--only", "migrate"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("skip (selection)"), "out:\n{out}");
    assert!(out.contains("run "), "out:\n{out}");
    assert!(out.contains("1 would run"), "out:\n{out}");
}

#[test]
fn dry_run_marks_no_run_blocks() {
    let wb = "\
```bash {#a}
echo AAA
```

```bash {#b no-run}
echo BBB
```
";
    let f = fixture("wb.md", wb);
    let o = f.run(&["--dry-run"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    assert!(stdout(&o).contains("skip (no-run)"), "out:\n{}", stdout(&o));
}

// ---------------------------------------------------------------------------
// --cache
// ---------------------------------------------------------------------------

const CACHE_WB: &str = "\
```bash {#a}
echo AAAMARK
```

```bash {#b no-cache}
echo BBBMARK
```
";

#[test]
fn cache_skips_unchanged_block_on_second_run() {
    let f = fixture("wb.md", CACHE_WB);

    let first = f.run(&["--cache", "pipe"]);
    assert_eq!(first.status.code(), Some(0), "stderr:\n{}", stderr(&first));
    // First run executes everything.
    assert!(
        stdout(&first).contains("AAAMARK"),
        "out:\n{}",
        stdout(&first)
    );

    let second = f.run(&["--cache", "pipe"]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&second)
    );
    // Block a (cacheable) is skipped on the second run.
    assert!(
        !stdout(&second).contains("AAAMARK"),
        "cached block re-ran:\n{}",
        stdout(&second)
    );
    let err = stderr(&second);
    assert!(err.contains("skipped"), "stderr:\n{err}");
    assert!(
        err.contains("unchanged source + params since a cached success"),
        "stderr:\n{err}"
    );
    // Block b carries {no-cache} and always runs.
    assert!(
        stdout(&second).contains("BBBMARK"),
        "no-cache block was skipped:\n{}",
        stdout(&second)
    );
}

#[test]
fn cache_reruns_block_after_edit() {
    let f = fixture("wb.md", CACHE_WB);

    let first = f.run(&["--cache", "pipe2"]);
    assert_eq!(first.status.code(), Some(0), "stderr:\n{}", stderr(&first));

    // Edit block a's body — its cache key changes, so it must re-run.
    let edited = "\
```bash {#a}
echo AAAEDITED
```

```bash {#b no-cache}
echo BBBMARK
```
";
    std::fs::write(&f.path, edited).unwrap();
    let second = f.run(&["--cache", "pipe2"]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("AAAEDITED"),
        "edited block did not re-run:\n{}",
        stdout(&second)
    );
}

#[test]
fn no_cache_flag_forces_full_run() {
    let f = fixture("wb.md", CACHE_WB);

    let first = f.run(&["--cache", "pipe3"]);
    assert_eq!(first.status.code(), Some(0), "stderr:\n{}", stderr(&first));

    // --no-cache disables the cache for the whole run, so block a runs again.
    let second = f.run(&["--cache", "pipe3", "--no-cache"]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("AAAMARK"),
        "no-cache should force a full run:\n{}",
        stdout(&second)
    );
    assert!(
        !stderr(&second).contains("cached success"),
        "no cache skip expected:\n{}",
        stderr(&second)
    );
}

// ---------------------------------------------------------------------------
// --changed (git-gated)
// ---------------------------------------------------------------------------

/// True if `git` is on PATH. The --changed tests no-op (pass) without it.
fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a git command inside `dir` with a fixed identity and isolated HOME.
fn git(dir: &std::path::Path, home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_AUTHOR_NAME", "wbtest")
        .env("GIT_AUTHOR_EMAIL", "wbtest@example.com")
        .env("GIT_COMMITTER_NAME", "wbtest")
        .env("GIT_COMMITTER_EMAIL", "wbtest@example.com")
        .output()
        .expect("git spawn")
}

const CHANGED_WB: &str = "\
```bash {#one}
echo ONEMARK
```

```bash {#two}
echo TWOMARK
```
";

#[test]
fn changed_runs_only_edited_block() {
    if !git_available() {
        eprintln!("git unavailable — skipping changed_runs_only_edited_block");
        return;
    }
    let f = fixture("ch.md", CHANGED_WB);
    let home = f.home.path();
    let dir = f.dir.path();

    assert!(
        git(dir, home, &["-c", "init.defaultBranch=main", "init", "-q"])
            .status
            .success()
    );
    git(dir, home, &["add", "ch.md"]);
    assert!(git(dir, home, &["commit", "-qm", "init"]).status.success());

    // Edit only block two.
    let edited = "\
```bash {#one}
echo ONEMARK
```

```bash {#two}
echo TWO_EDITED
```
";
    std::fs::write(&f.path, edited).unwrap();

    let o = f.run(&["--changed"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("TWO_EDITED"),
        "edited block should run:\n{out}"
    );
    assert!(
        !out.contains("ONEMARK"),
        "unchanged block should be skipped:\n{out}"
    );
}

#[test]
fn changed_with_explicit_base() {
    if !git_available() {
        eprintln!("git unavailable — skipping changed_with_explicit_base");
        return;
    }
    let f = fixture("ch.md", CHANGED_WB);
    let home = f.home.path();
    let dir = f.dir.path();

    git(dir, home, &["-c", "init.defaultBranch=main", "init", "-q"]);
    git(dir, home, &["add", "ch.md"]);
    git(dir, home, &["commit", "-qm", "init"]);

    let edited = "\
```bash {#one}
echo ONE_EDITED
```

```bash {#two}
echo TWOMARK
```
";
    std::fs::write(&f.path, edited).unwrap();

    let o = f.run(&["--changed", "--changed-base", "HEAD"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("ONE_EDITED"),
        "edited block should run:\n{out}"
    );
    assert!(
        !out.contains("TWOMARK"),
        "unchanged block should be skipped:\n{out}"
    );
}

#[test]
fn changed_untracked_file_runs_everything() {
    if !git_available() {
        eprintln!("git unavailable — skipping changed_untracked_file_runs_everything");
        return;
    }
    let f = fixture("ch.md", CHANGED_WB);
    let home = f.home.path();
    let dir = f.dir.path();
    // Init a repo but never add/commit the workbook — it's untracked, so
    // --changed treats every block as changed.
    git(dir, home, &["-c", "init.defaultBranch=main", "init", "-q"]);

    let o = f.run(&["--changed"]);
    assert_eq!(o.status.code(), Some(0), "stderr:\n{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("ONEMARK"), "out:\n{out}");
    assert!(out.contains("TWOMARK"), "out:\n{out}");
    assert!(stderr(&o).contains("untracked"), "stderr:\n{}", stderr(&o));
}

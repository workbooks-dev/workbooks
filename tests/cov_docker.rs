//! Integration coverage for the *real* Docker sandbox path.
//!
//! These tests spawn the real `wb` binary (so cargo-llvm-cov instruments it)
//! and drive the genuine `src/sandbox.rs` machinery: `build_image` (custom +
//! generated paths), `image_exists` (miss then hit), `run_in_sandbox`,
//! `list_images`, and `prune_images`, plus the `wb run --sandbox` /
//! `wb containers` / `wb inspect` dispatch in `src/lib.rs`.
//!
//! Every test is gated on `docker info` succeeding: if Docker is unavailable
//! the test early-returns and passes as a no-op, so the suite stays green on
//! hosts without Docker. Each spawn sets an isolated `HOME` (a fresh tempdir)
//! so `~/.wb` state never leaks.
//!
//! Docker work is kept tiny: the custom-sandbox tests use a `FROM alpine`
//! image whose only layer installs a 3-line `/usr/local/bin/wb` shell shim, so
//! `docker run … wb run <file>` echoes a sentinel and `cat`s the mounted
//! workbook (proving the build + the container run + the bind mount). The
//! `--sandbox` flag test reuses the deterministic generated python image.
//!
//! Run single-threaded so the shared content-addressed images don't race:
//!   cargo test --test cov_docker -- --test-threads=1
//!
//! NOTE: the final `test_zzz_*` prune test removes *all* `wb-sandbox:*` images
//! (that is what `wb containers prune` does). These are deterministic build
//! caches that wb regenerates on the next sandbox run; no non-wb images are
//! touched.

use std::path::PathBuf;
use std::process::Command;

/// Locate the `wb` binary next to this test executable.
fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// Result of one `wb` invocation.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    /// stdout + stderr concatenated (sandbox output streams to both).
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Spawn the real `wb` binary with an isolated HOME and the given cwd.
fn run_wb(home: &std::path::Path, cwd: &std::path::Path, args: &[&str]) -> Run {
    let out = Command::new(wb_binary())
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        // Keep the inner wb from re-sandboxing if a test ever runs inside one.
        .env_remove("WB_SANDBOX_INNER")
        .output()
        .expect("spawn wb");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// True when a Docker daemon is reachable. The whole file no-ops when false.
fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Sentinel echoed by the alpine shim's `wb` — its presence proves the image
/// built and the container actually ran our command.
const SHIM_SENTINEL: &str = "WB_SANDBOX_SHIM_OK";
/// Body of the custom workbook's bash block — the shim `cat`s the mounted
/// file, so seeing this proves the workbook bind-mount worked.
const BODY_SENTINEL: &str = "WORKBOOK_BODY_MARKER";

/// Write a `sandbox: custom` workbook + its tiny alpine Dockerfile into `dir`.
/// The Dockerfile installs a `/usr/local/bin/wb` shim that echoes
/// `SHIM_SENTINEL` and cats its file argument. Returns the workbook filename.
fn write_custom_workbook(dir: &std::path::Path) -> &'static str {
    // The shim stands in for the in-container `wb`: `wb run <file>` -> $1=run,
    // $2=<file>. It echoes a sentinel then cats the mounted workbook.
    let dockerfile = "FROM alpine:latest\n\
        RUN printf '%s\\n' '#!/bin/sh' 'echo WB_SANDBOX_SHIM_OK' 'cat \"$2\" 2>/dev/null || true' \
        > /usr/local/bin/wb && chmod +x /usr/local/bin/wb\n";
    std::fs::write(dir.join("Dockerfile.sbx"), dockerfile).unwrap();

    let wb = format!(
        "---\n\
         title: Custom Sandbox\n\
         requires:\n\
         \x20 sandbox: custom\n\
         \x20 dockerfile: ./Dockerfile.sbx\n\
         ---\n\n\
         # Custom Sandbox\n\n\
         ```bash\n\
         echo \"{}\"\n\
         ```\n",
        BODY_SENTINEL
    );
    std::fs::write(dir.join("custom-sandbox.md"), wb).unwrap();
    "custom-sandbox.md"
}

/// Write a plain (no `requires:`) bash workbook used by the `--sandbox` flag
/// tests, which synthesize a generated image. Returns the workbook filename.
fn write_plain_workbook(dir: &std::path::Path, sentinel: &str) -> String {
    let wb = format!(
        "---\n\
         title: Plain\n\
         runtime: bash\n\
         ---\n\n\
         ```bash\n\
         echo \"{}\"\n\
         ```\n",
        sentinel
    );
    let name = "plain.md";
    std::fs::write(dir.join(name), wb).unwrap();
    name.to_string()
}

// ── 1. custom build + run: build_image(custom), image_exists, run_in_sandbox ──
#[test]
fn test_custom_sandbox_builds_and_runs() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());

    let r = run_wb(home.path(), dir.path(), &["run", wb]);
    let out = r.all();
    assert_eq!(r.code, 0, "expected exit 0; stderr=\n{}", r.stderr);
    assert!(
        out.contains(SHIM_SENTINEL),
        "sentinel proving in-container run missing; output=\n{}",
        out
    );
    // The shim cat'd the bind-mounted workbook, so its body marker is present.
    assert!(
        out.contains(BODY_SENTINEL),
        "bind-mounted workbook body missing; output=\n{}",
        out
    );
}

// ── 2. cached re-run: image_exists hit, no rebuild ──
#[test]
fn test_custom_sandbox_cached_no_rebuild() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());

    // First run: builds the image (or hits a prior cache — either is fine).
    let first = run_wb(home.path(), dir.path(), &["run", wb]);
    assert_eq!(first.code, 0, "first run failed; stderr=\n{}", first.stderr);

    // Second run: image now definitely exists -> build_image short-circuits and
    // never prints the "building sandbox image" notice.
    let second = run_wb(home.path(), dir.path(), &["run", wb]);
    assert_eq!(
        second.code, 0,
        "second run failed; stderr=\n{}",
        second.stderr
    );
    assert!(
        second.all().contains(SHIM_SENTINEL),
        "cached re-run lost the sentinel; output=\n{}",
        second.all()
    );
    assert!(
        !second.stderr.contains("building sandbox image"),
        "expected cached image (no rebuild); stderr=\n{}",
        second.stderr
    );
}

// ── 3. `--sandbox` forces a generated container for a plain workbook ──
#[test]
fn test_sandbox_flag_forces_generated_image() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sentinel = "SANDBOX_FLAG_RAN";
    let wb = write_plain_workbook(dir.path(), sentinel);

    // bash runtime -> synthesized `sandbox: python` (debian + wb installer).
    // The deterministic tag is usually already cached on a dev host; if not,
    // Docker's layer cache makes the rebuild fast.
    let r = run_wb(home.path(), dir.path(), &["run", "--sandbox", &wb]);
    assert_eq!(
        r.code, 0,
        "--sandbox run failed (is the generated image buildable offline?); stderr=\n{}",
        r.stderr
    );
    assert!(
        r.all().contains(sentinel),
        "block output missing from sandboxed run; output=\n{}",
        r.all()
    );
}

// ── 4. `--sandbox-no-network` exercises the `--network none` branch ──
// Uses the tiny custom alpine image (no network needed) so the no-network
// branch of run_in_sandbox is covered cheaply.
#[test]
fn test_sandbox_no_network_branch() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());

    let r = run_wb(
        home.path(),
        dir.path(),
        &["run", "--sandbox-no-network", wb],
    );
    assert_eq!(r.code, 0, "no-network run failed; stderr=\n{}", r.stderr);
    assert!(
        r.all().contains(SHIM_SENTINEL),
        "sentinel missing under --network none; output=\n{}",
        r.all()
    );
}

// ── 5. `wb containers list` (text + json): list_images ──
#[test]
fn test_containers_list_shows_built_image() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());
    // Ensure at least one wb-sandbox image exists for listing.
    let built = run_wb(home.path(), dir.path(), &["run", wb]);
    assert_eq!(
        built.code, 0,
        "setup build failed; stderr=\n{}",
        built.stderr
    );

    let text = run_wb(home.path(), dir.path(), &["containers", "list"]);
    assert_eq!(
        text.code, 0,
        "containers list exit; stderr=\n{}",
        text.stderr
    );
    assert!(
        text.all().contains("wb-sandbox:"),
        "listing should show a wb-sandbox image; output=\n{}",
        text.all()
    );

    let json = run_wb(
        home.path(),
        dir.path(),
        &["containers", "list", "--format", "json"],
    );
    assert_eq!(json.code, 0, "containers list --format json exit");
    assert!(
        json.stdout.contains("\"images\""),
        "json listing should carry an images array; stdout=\n{}",
        json.stdout
    );
}

// ── 6. `wb containers build <dir>` pre-builds a folder's images ──
#[test]
fn test_containers_build_dir() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    write_custom_workbook(dir.path());

    let dir_str = dir.path().to_string_lossy().into_owned();
    let r = run_wb(home.path(), dir.path(), &["containers", "build", &dir_str]);
    assert_eq!(r.code, 0, "containers build failed; stderr=\n{}", r.stderr);
    // Either freshly built or already cached; both are success outcomes.
    let out = r.all();
    assert!(
        out.contains("built") || out.contains("cached") || out.contains("->") || out.contains("✓"),
        "build summary missing; output=\n{}",
        out
    );

    // JSON form reports a structured result.
    let json = run_wb(
        home.path(),
        dir.path(),
        &["containers", "build", &dir_str, "--format", "json"],
    );
    assert_eq!(json.code, 0, "containers build --format json exit");
    assert!(
        json.stdout.contains("\"results\""),
        "json build should carry a results array; stdout=\n{}",
        json.stdout
    );
}

// ── 7. `wb inspect` shows resolved sandbox config + image tag (no Docker run) ──
#[test]
fn test_inspect_shows_sandbox_config() {
    // inspect computes image_tag without invoking Docker, but gate anyway for
    // consistency: the test is a no-op when Docker is down.
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());

    let r = run_wb(home.path(), dir.path(), &["inspect", wb]);
    assert_eq!(r.code, 0, "inspect exit; stderr=\n{}", r.stderr);
    let out = r.all();
    assert!(
        out.contains("sandbox: custom"),
        "inspect should report the sandbox type; output=\n{}",
        out
    );
    assert!(
        out.contains("wb-sandbox:"),
        "inspect should report the resolved image tag; output=\n{}",
        out
    );
}

// ── 8. error path: `sandbox: custom` -> missing Dockerfile => exit 5 ──
#[test]
fn test_custom_sandbox_missing_dockerfile_errors() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    // A custom workbook whose dockerfile does not exist on disk.
    let wb = "---\n\
        title: Broken Sandbox\n\
        requires:\n\
        \x20 sandbox: custom\n\
        \x20 dockerfile: ./does-not-exist.Dockerfile\n\
        ---\n\n\
        ```bash\n\
        echo nope\n\
        ```\n";
    std::fs::write(dir.path().join("broken.md"), wb).unwrap();

    let r = run_wb(home.path(), dir.path(), &["run", "broken.md"]);
    // EXIT_SANDBOX_UNAVAILABLE == 5 (build_image errors before any block runs).
    assert_eq!(
        r.code, 5,
        "expected EXIT_SANDBOX_UNAVAILABLE (5); got {} stderr=\n{}",
        r.code, r.stderr
    );
    assert!(
        r.stderr.contains("sandbox") || r.stderr.contains("dockerfile"),
        "error should mention the sandbox/dockerfile; stderr=\n{}",
        r.stderr
    );
}

// ── 9 (LAST). `wb containers prune` removes wb-sandbox images: prune_images ──
// Named `zzz` so libtest's alphabetical ordering runs it after the build/run
// tests. Self-contained: it first ensures an image exists, then prunes.
#[test]
fn test_zzz_containers_prune_removes_images() {
    if !docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = write_custom_workbook(dir.path());
    // Guarantee at least one wb-sandbox image is present to remove.
    let built = run_wb(home.path(), dir.path(), &["run", wb]);
    assert_eq!(
        built.code, 0,
        "setup build failed; stderr=\n{}",
        built.stderr
    );

    let prune = run_wb(
        home.path(),
        dir.path(),
        &["containers", "prune", "--format", "json"],
    );
    assert_eq!(prune.code, 0, "prune exit; stderr=\n{}", prune.stderr);
    assert!(
        prune.stdout.contains("\"removed\""),
        "prune json should report removed count; stdout=\n{}",
        prune.stdout
    );
    // We built at least one image above, so prune removed >= 1.
    assert!(
        !prune.stdout.contains("\"removed\": 0") && !prune.stdout.contains("\"removed\":0"),
        "expected to remove at least one image; stdout=\n{}",
        prune.stdout
    );

    // After a full prune, listing reports no wb-sandbox images.
    let list = run_wb(home.path(), dir.path(), &["containers", "list"]);
    assert_eq!(list.code, 0, "post-prune list exit");
    assert!(
        !list.all().contains("wb-sandbox:"),
        "no wb-sandbox images should remain after prune; output=\n{}",
        list.all()
    );
}

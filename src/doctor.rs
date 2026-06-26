// `wb doctor` — environment and runtime health checks.
//
// Shallow checks run by default: no network, no Docker image pulls, no sidecar.
// Deep checks (--deep): Docker build smoke test, sidecar handshake, Redis ping.
// Doctor is intentionally decoupled from validate — they share the Diagnostic
// type but have no other cross-imports.

use crate::diagnostic::Diagnostic;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: Option<String>,
    // Reserved for structured diagnostic integration in a future wave.
    #[allow(dead_code)]
    pub diagnostic: Option<Diagnostic>,
}

impl CheckResult {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            detail: Some(detail.into()),
            diagnostic: None,
        }
    }

    fn warn(name: &'static str, msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self {
            name,
            status: CheckStatus::Warn,
            detail: Some(msg.clone()),
            diagnostic: Some(Diagnostic::warning("wb-doctor", "/", msg)),
        }
    }

    fn fail(name: &'static str, msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self {
            name,
            status: CheckStatus::Fail,
            detail: Some(msg.clone()),
            diagnostic: Some(Diagnostic::error("wb-doctor", "/", msg)),
        }
    }

    fn skipped(name: &'static str, reason: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skipped,
            detail: Some(reason.into()),
            diagnostic: None,
        }
    }
}

pub struct DoctorOptions {
    pub deep: bool,
    pub json: bool,
}

/// Run all checks and return (results, exit_code).
pub fn run(opts: &DoctorOptions) -> (Vec<CheckResult>, i32) {
    let mut results = vec![
        check_wb_version(),
        check_binary("bash", "bash"),
        check_binary("python3", "python3"),
        check_binary("node", "node"),
        check_binary_warn("ruby", "ruby"),
        check_docker(),
        check_home_dir(),
        check_browser_runtime(),
    ];

    // Deep checks
    if opts.deep {
        results.push(check_docker_build_smoke(&results));
        results.push(check_sidecar_handshake(&results));
        results.push(check_redis_ping(&results));
    }

    let has_fail = results.iter().any(|r| r.status == CheckStatus::Fail);
    let code = if has_fail {
        crate::exit_codes::EXIT_WORKBOOK_INVALID
    } else {
        crate::exit_codes::EXIT_SUCCESS
    };
    (results, code)
}

// ─── Shallow checks ──────────────────────────────────────────────────────────

fn check_wb_version() -> CheckResult {
    let version = env!("CARGO_PKG_VERSION");
    let path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    CheckResult::pass("wb_version", format!("wb v{version} ({path})"))
}

/// Run `<binary> --version` (or `<binary> -v`) and return the first line.
fn resolve_binary_version(binary: &str) -> Option<String> {
    let out = Command::new(binary)
        .arg("--version")
        .output()
        .or_else(|_| Command::new(binary).arg("-v").output())
        .ok()?;
    if out.status.success() || !out.stdout.is_empty() {
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().next().unwrap_or("").trim().to_string();
        Some(line)
    } else {
        None
    }
}

fn check_binary(name: &'static str, binary: &str) -> CheckResult {
    match resolve_binary_version(binary) {
        Some(ver) => CheckResult::pass(name, ver),
        None => CheckResult::fail(name, format!("{binary} not found on PATH")),
    }
}

fn check_binary_warn(name: &'static str, binary: &str) -> CheckResult {
    match resolve_binary_version(binary) {
        Some(ver) => CheckResult::pass(name, ver),
        None => CheckResult::warn(name, format!("{binary} not found on PATH")),
    }
}

fn check_docker() -> CheckResult {
    // Try `docker version --format '{{.Server.Version}}'` — a single call,
    // no image pulls. We distinguish "docker missing" from "daemon down".
    let which = Command::new("docker").arg("--version").output();
    match which {
        Err(_) => return CheckResult::fail("docker_present", "docker not found on PATH"),
        Ok(o) if !o.status.success() => {
            return CheckResult::fail("docker_present", "docker --version failed")
        }
        Ok(_) => {}
    }

    // docker is on PATH; now check if the daemon is up.
    let daemon = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output();
    match daemon {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout).trim().to_string();
            CheckResult::pass("docker_present", format!("docker server {ver}"))
        }
        _ => CheckResult::warn(
            "docker_present",
            "docker is installed but the daemon appears to be down",
        ),
    }
}

fn check_home_dir() -> CheckResult {
    let home = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => return CheckResult::fail("home_dir_writable", "$HOME not set"),
    };
    let wb_dir = home.join(".wb");
    if let Err(e) = std::fs::create_dir_all(&wb_dir) {
        return CheckResult::fail("home_dir_writable", format!("cannot create ~/.wb: {e}"));
    }
    // Check writable by creating a temp file.
    let probe = wb_dir.join(".doctor-probe");
    if let Err(e) = std::fs::write(&probe, b"probe") {
        return CheckResult::fail("home_dir_writable", format!("~/.wb is not writable: {e}"));
    }
    let _ = std::fs::remove_file(&probe);
    CheckResult::pass(
        "home_dir_writable",
        format!("{} writable", wb_dir.display()),
    )
}

fn check_browser_runtime() -> CheckResult {
    // Check for wb-browser-runtime on PATH or in node_modules/.bin/.
    let path_check = Command::new("which")
        .arg("wb-browser-runtime")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let nm_check = std::path::Path::new("node_modules/.bin/wb-browser-runtime").exists();

    if path_check || nm_check {
        CheckResult::pass("wb_browser_runtime_present", "wb-browser-runtime found")
    } else {
        CheckResult::warn(
            "wb_browser_runtime_present",
            "wb-browser-runtime not installed; install via npm if you use browser blocks",
        )
    }
}

// ─── Deep checks ─────────────────────────────────────────────────────────────

fn check_docker_build_smoke(results: &[CheckResult]) -> CheckResult {
    let docker_ok = results
        .iter()
        .any(|r| r.name == "docker_present" && r.status == CheckStatus::Pass);
    if !docker_ok {
        return CheckResult::skipped(
            "docker_build_smoke",
            "skipped (docker daemon not available)",
        );
    }

    // Build a minimal image to verify Docker daemon networking works.
    let dockerfile = "FROM alpine:latest\nRUN true\n";
    let dir = std::env::temp_dir().join("wb-doctor-smoke");
    if std::fs::create_dir_all(&dir).is_err() {
        return CheckResult::fail(
            "docker_build_smoke",
            "failed to create temp directory for smoke build",
        );
    }
    if std::fs::write(dir.join("Dockerfile"), dockerfile).is_err() {
        return CheckResult::fail("docker_build_smoke", "failed to write Dockerfile");
    }

    let out = Command::new("docker")
        .args([
            "build",
            "--quiet",
            "--no-cache",
            "-t",
            "wb-doctor-smoke:latest",
            dir.to_str().unwrap_or("."),
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let _ = Command::new("docker")
                .args(["rmi", "-f", "wb-doctor-smoke:latest"])
                .output();
            CheckResult::pass("docker_build_smoke", "Docker build smoke test passed")
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            CheckResult::fail(
                "docker_build_smoke",
                format!("Docker build failed: {stderr}"),
            )
        }
        Err(e) => CheckResult::fail("docker_build_smoke", format!("docker build error: {e}")),
    }
}

fn check_sidecar_handshake(results: &[CheckResult]) -> CheckResult {
    let runtime_present = results
        .iter()
        .any(|r| r.name == "wb_browser_runtime_present" && r.status == CheckStatus::Pass);
    if !runtime_present {
        return CheckResult::skipped(
            "sidecar_handshake",
            "skipped (wb-browser-runtime not installed)",
        );
    }
    // Spawn the sidecar with an empty verb list and expect it to exit 0.
    // This is a no-op handshake to verify the process starts cleanly.
    let out = Command::new("wb-browser-runtime").arg("--help").output();
    match out {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => {
            CheckResult::pass("sidecar_handshake", "wb-browser-runtime responded")
        }
        Ok(_) => CheckResult::warn("sidecar_handshake", "wb-browser-runtime exited non-zero"),
        Err(e) => CheckResult::fail("sidecar_handshake", format!("spawn failed: {e}")),
    }
}

fn check_redis_ping(results: &[CheckResult]) -> CheckResult {
    let _ = results;
    // Only probe if WB_SIGNAL_URL or WB_CALLBACK_URL looks like redis://.
    let url = std::env::var("WB_SIGNAL_URL")
        .or_else(|_| std::env::var("WB_CALLBACK_URL"))
        .unwrap_or_default();

    if !url.starts_with("redis://") && !url.starts_with("rediss://") {
        return CheckResult::skipped(
            "redis_ping",
            "skipped (WB_SIGNAL_URL / WB_CALLBACK_URL not set to a redis:// URL)",
        );
    }

    // Use redis-cli if available.
    let out = Command::new("redis-cli")
        .args(["-u", &url, "PING"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            CheckResult::pass("redis_ping", format!("Redis PING at {url} → PONG"))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            CheckResult::warn("redis_ping", format!("Redis PING failed: {stderr}"))
        }
        Err(_) => CheckResult::skipped(
            "redis_ping",
            "redis-cli not on PATH; skipping Redis connectivity check",
        ),
    }
}

// ─── Output rendering ─────────────────────────────────────────────────────────

pub fn render_text(results: &[CheckResult]) -> String {
    let mut out = String::new();
    for r in results {
        let glyph = match r.status {
            CheckStatus::Pass => "✓",
            CheckStatus::Warn => "⚠",
            CheckStatus::Fail => "✗",
            CheckStatus::Skipped => "·",
        };
        let detail = r.detail.as_deref().unwrap_or("");
        out.push_str(&format!("{glyph} {detail}\n"));
    }
    // Summary line
    let pass = results
        .iter()
        .filter(|r| r.status == CheckStatus::Pass)
        .count();
    let warn = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warn)
        .count();
    let fail = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    out.push_str(&format!("Pass: {pass}  Warn: {warn}  Fail: {fail}\n"));
    out
}

pub fn render_json(results: &[CheckResult]) -> String {
    let checks: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let status = match r.status {
                CheckStatus::Pass => "pass",
                CheckStatus::Warn => "warn",
                CheckStatus::Fail => "fail",
                CheckStatus::Skipped => "skipped",
            };
            serde_json::json!({
                "name": r.name,
                "status": status,
                "detail": r.detail,
            })
        })
        .collect();
    let pass = results
        .iter()
        .filter(|r| r.status == CheckStatus::Pass)
        .count();
    let warn = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warn)
        .count();
    let fail = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    let obj = serde_json::json!({
        "checks": checks,
        "summary": { "pass": pass, "warn": warn, "fail": fail }
    });
    serde_json::to_string_pretty(&obj).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_resolution_runs() {
        // bash is always present on test machines; should not panic.
        let result = resolve_binary_version("bash");
        assert!(result.is_some(), "bash version should be resolvable");
    }

    #[test]
    fn deep_mode_skips_when_runtime_missing() {
        let fake = vec![CheckResult::warn(
            "wb_browser_runtime_present",
            "not installed",
        )];
        let sidecar = check_sidecar_handshake(&fake);
        assert_eq!(sidecar.status, CheckStatus::Skipped);
    }

    #[test]
    fn format_text_renders_warns_with_warn_glyph() {
        let results = vec![
            CheckResult::pass("wb_version", "wb v0.1.0"),
            CheckResult::warn("ruby", "not on PATH"),
        ];
        let text = render_text(&results);
        assert!(text.contains('✓'), "missing pass glyph");
        assert!(text.contains('⚠'), "missing warn glyph");
        assert!(text.contains("Warn: 1"), "missing warn count");
    }

    // ─── CheckResult constructors ────────────────────────────────────────────

    #[test]
    fn pass_constructor_sets_status_detail_and_no_diagnostic() {
        let r = CheckResult::pass("n", "all good");
        assert_eq!(r.name, "n");
        assert_eq!(r.status, CheckStatus::Pass);
        assert_eq!(r.detail.as_deref(), Some("all good"));
        assert!(r.diagnostic.is_none());
    }

    #[test]
    fn warn_constructor_carries_diagnostic_with_same_message() {
        let r = CheckResult::warn("n", "be careful");
        assert_eq!(r.status, CheckStatus::Warn);
        assert_eq!(r.detail.as_deref(), Some("be careful"));
        let d = r.diagnostic.expect("warn should carry a diagnostic");
        assert_eq!(d.message, "be careful");
    }

    #[test]
    fn fail_constructor_carries_error_diagnostic() {
        let r = CheckResult::fail("n", "broke");
        assert_eq!(r.status, CheckStatus::Fail);
        assert_eq!(r.detail.as_deref(), Some("broke"));
        let d = r.diagnostic.expect("fail should carry a diagnostic");
        assert_eq!(d.message, "broke");
    }

    #[test]
    fn skipped_constructor_has_reason_and_no_diagnostic() {
        let r = CheckResult::skipped("n", "not relevant");
        assert_eq!(r.status, CheckStatus::Skipped);
        assert_eq!(r.detail.as_deref(), Some("not relevant"));
        assert!(r.diagnostic.is_none());
    }

    // ─── run() aggregation + exit code ───────────────────────────────────────

    #[test]
    fn run_shallow_returns_success_or_fail_code_consistent_with_results() {
        let opts = DoctorOptions {
            deep: false,
            json: false,
        };
        let (results, code) = run(&opts);
        assert!(!results.is_empty(), "shallow run should produce checks");
        // Shallow run does not push deep checks.
        assert!(
            !results.iter().any(|r| r.name == "docker_build_smoke"),
            "shallow run must not include deep checks"
        );
        // Exit code must agree with whether any check failed.
        let has_fail = results.iter().any(|r| r.status == CheckStatus::Fail);
        let expected = if has_fail {
            crate::exit_codes::EXIT_WORKBOOK_INVALID
        } else {
            crate::exit_codes::EXIT_SUCCESS
        };
        assert_eq!(code, expected);
        // wb_version is always a pass.
        assert!(results
            .iter()
            .any(|r| r.name == "wb_version" && r.status == CheckStatus::Pass));
    }

    #[test]
    fn run_deep_appends_deep_checks() {
        let opts = DoctorOptions {
            deep: true,
            json: true,
        };
        let (results, _code) = run(&opts);
        for name in ["docker_build_smoke", "sidecar_handshake", "redis_ping"] {
            assert!(
                results.iter().any(|r| r.name == name),
                "deep run should include {name}"
            );
        }
    }

    // ─── individual checks (invariants only — environment-dependent) ─────────

    #[test]
    fn check_wb_version_always_passes() {
        let r = check_wb_version();
        assert_eq!(r.name, "wb_version");
        assert_eq!(r.status, CheckStatus::Pass);
        assert!(r.detail.unwrap().contains("wb v"));
    }

    #[test]
    fn check_binary_present_passes() {
        // bash is present on test machines.
        let r = check_binary("bash", "bash");
        assert_eq!(r.status, CheckStatus::Pass);
    }

    #[test]
    fn check_binary_missing_fails() {
        let r = check_binary("nope", "wb-nonexistent-binary-xyz");
        assert_eq!(r.status, CheckStatus::Fail);
        assert!(r.detail.unwrap().contains("not found on PATH"));
    }

    #[test]
    fn check_binary_warn_missing_warns() {
        let r = check_binary_warn("nope", "wb-nonexistent-binary-xyz");
        assert_eq!(r.status, CheckStatus::Warn);
    }

    #[test]
    fn check_binary_warn_present_passes() {
        let r = check_binary_warn("bash", "bash");
        assert_eq!(r.status, CheckStatus::Pass);
    }

    #[test]
    fn resolve_binary_version_missing_is_none() {
        assert!(resolve_binary_version("wb-nonexistent-binary-xyz").is_none());
    }

    #[test]
    fn check_docker_returns_a_docker_present_result() {
        // Environment-dependent: may pass/warn/fail. Assert invariants only.
        let r = check_docker();
        assert_eq!(r.name, "docker_present");
        assert!(matches!(
            r.status,
            CheckStatus::Pass | CheckStatus::Warn | CheckStatus::Fail
        ));
        assert!(r.detail.is_some());
    }

    #[test]
    fn check_home_dir_runs_and_names_correctly() {
        let r = check_home_dir();
        assert_eq!(r.name, "home_dir_writable");
        // On a normal test machine HOME is set and ~/.wb is writable.
        assert!(matches!(r.status, CheckStatus::Pass | CheckStatus::Fail));
        assert!(r.detail.is_some());
    }

    #[test]
    fn check_browser_runtime_runs_and_names_correctly() {
        let r = check_browser_runtime();
        assert_eq!(r.name, "wb_browser_runtime_present");
        assert!(matches!(r.status, CheckStatus::Pass | CheckStatus::Warn));
        assert!(r.detail.is_some());
    }

    // ─── deep-check gating ───────────────────────────────────────────────────

    #[test]
    fn docker_build_smoke_skips_when_docker_not_passing() {
        let prior = vec![CheckResult::fail("docker_present", "not found")];
        let r = check_docker_build_smoke(&prior);
        assert_eq!(r.name, "docker_build_smoke");
        assert_eq!(r.status, CheckStatus::Skipped);
        assert!(r.detail.unwrap().contains("docker daemon not available"));
    }

    #[test]
    fn sidecar_handshake_skips_when_runtime_missing() {
        let prior = vec![CheckResult::warn(
            "wb_browser_runtime_present",
            "not installed",
        )];
        let r = check_sidecar_handshake(&prior);
        assert_eq!(r.status, CheckStatus::Skipped);
        assert!(r
            .detail
            .unwrap()
            .contains("wb-browser-runtime not installed"));
    }

    #[test]
    fn sidecar_handshake_runs_when_runtime_present() {
        // Force the gate open with a synthetic passing prior result. The probe
        // then shells out to wb-browser-runtime, which is environment-dependent;
        // assert only on invariants (name + a non-skipped-by-gate result).
        let prior = vec![CheckResult::pass("wb_browser_runtime_present", "found")];
        let r = check_sidecar_handshake(&prior);
        assert_eq!(r.name, "sidecar_handshake");
        // Could be pass/warn (responded/non-zero) or fail (spawn failed) — never
        // the gate's "skipped (runtime not installed)" path.
        assert!(matches!(
            r.status,
            CheckStatus::Pass | CheckStatus::Warn | CheckStatus::Fail
        ));
    }

    // Both redis_ping branches mutate the same process-global env vars, so they
    // live in one test to avoid racing with each other under the parallel test
    // runner.
    #[test]
    fn redis_ping_gating_on_url() {
        let saved_signal = std::env::var("WB_SIGNAL_URL").ok();
        let saved_cb = std::env::var("WB_CALLBACK_URL").ok();

        // (1) No redis:// URL → gate skips.
        std::env::remove_var("WB_SIGNAL_URL");
        std::env::remove_var("WB_CALLBACK_URL");
        let r = check_redis_ping(&[]);
        assert_eq!(r.name, "redis_ping");
        assert_eq!(r.status, CheckStatus::Skipped);
        assert!(r.detail.unwrap().contains("not set to a redis:// URL"));

        // (2) redis:// URL set → gate opens, probe is attempted.
        std::env::set_var("WB_SIGNAL_URL", "redis://127.0.0.1:6399/0");
        let r = check_redis_ping(&[]);
        assert_eq!(r.name, "redis_ping");
        // redis-cli may be absent (Skipped) or present but unable to connect
        // (Warn) or, improbably, connect (Pass). Never the "no redis URL" skip.
        assert!(matches!(
            r.status,
            CheckStatus::Pass | CheckStatus::Warn | CheckStatus::Skipped
        ));
        assert!(!r.detail.unwrap().contains("not set to a redis:// URL"));

        // Restore.
        match saved_signal {
            Some(v) => std::env::set_var("WB_SIGNAL_URL", v),
            None => std::env::remove_var("WB_SIGNAL_URL"),
        }
        match saved_cb {
            Some(v) => std::env::set_var("WB_CALLBACK_URL", v),
            None => std::env::remove_var("WB_CALLBACK_URL"),
        }
    }

    // ─── rendering ───────────────────────────────────────────────────────────

    #[test]
    fn render_text_includes_all_glyphs_and_counts() {
        let results = vec![
            CheckResult::pass("a", "ok"),
            CheckResult::warn("b", "careful"),
            CheckResult::fail("c", "broke"),
            CheckResult::skipped("d", "n/a"),
        ];
        let text = render_text(&results);
        assert!(text.contains('✓'));
        assert!(text.contains('⚠'));
        assert!(text.contains('✗'));
        assert!(text.contains('·'));
        assert!(text.contains("Pass: 1  Warn: 1  Fail: 1"));
    }

    #[test]
    fn render_text_handles_missing_detail() {
        let results = vec![CheckResult {
            name: "x",
            status: CheckStatus::Pass,
            detail: None,
            diagnostic: None,
        }];
        let text = render_text(&results);
        // Empty detail still renders the glyph + a newline + summary.
        assert!(text.contains('✓'));
        assert!(text.contains("Pass: 1  Warn: 0  Fail: 0"));
    }

    #[test]
    fn render_text_empty_results_just_summary() {
        let text = render_text(&[]);
        assert_eq!(text, "Pass: 0  Warn: 0  Fail: 0\n");
    }

    #[test]
    fn render_json_is_valid_and_has_expected_shape() {
        let results = vec![
            CheckResult::pass("a", "ok"),
            CheckResult::warn("b", "careful"),
            CheckResult::fail("c", "broke"),
            CheckResult::skipped("d", "n/a"),
        ];
        let json = render_json(&results);
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let checks = v["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 4);
        assert_eq!(checks[0]["name"], "a");
        assert_eq!(checks[0]["status"], "pass");
        assert_eq!(checks[1]["status"], "warn");
        assert_eq!(checks[2]["status"], "fail");
        assert_eq!(checks[3]["status"], "skipped");
        assert_eq!(v["summary"]["pass"], 1);
        assert_eq!(v["summary"]["warn"], 1);
        assert_eq!(v["summary"]["fail"], 1);
    }

    #[test]
    fn render_json_empty_results() {
        let json = render_json(&[]);
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(v["checks"].as_array().unwrap().len(), 0);
        assert_eq!(v["summary"]["pass"], 0);
    }

    #[test]
    fn check_status_equality() {
        assert_eq!(CheckStatus::Pass, CheckStatus::Pass);
        assert_ne!(CheckStatus::Pass, CheckStatus::Warn);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_version_nonzero_empty_output_is_none() {
        // A binary that runs but exits non-zero with empty stdout for both
        // `--version` and `-v` takes the `else { None }` arm (line 125), which
        // is distinct from the binary-not-found `?` early return.
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("wb-doctor-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("always-fail");
        std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let got = resolve_binary_version(script.to_str().unwrap());
        assert!(got.is_none(), "expected None, got {got:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

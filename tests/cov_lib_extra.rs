//! Integration coverage for the remaining reachable `src/lib.rs` /
//! `src/executor.rs` paths not exercised by the other `cov_*` suites:
//!
//!  * the native `http` runtime (real local server, 2xx + 404 + invalid),
//!  * the native `sql` runtime (sqlite `sqlite:` URL form, query error, no-conn),
//!  * secret providers wired through the CLI (`--secrets` + `--secrets-cmd` +
//!    `--project`: dotenv / command / cmd / env / unknown / missing-cmd),
//!  * `--repair` against a *responsive* endpoint (skip / abort / rerun) plus
//!    `--repair-max` budgeting and parse errors,
//!  * `wb capture -i` interactive REPL recording (piped stdin) and the plain
//!    non-interactive, non-`--assert` stdin path,
//!  * misc bare-invocation back-compat flags (`--inspect` / `-i`, `--verbose`,
//!    no-file usage), `transform` with `{{variables}}`, and a folder run with
//!    an aggregated `-o` report.
//!
//! Every test spawns the real `wb` binary (so cargo-llvm-cov instruments it)
//! with an isolated `HOME` pointed at a fresh `tempfile::tempdir()`, so `~/.wb`
//! never leaks between tests or onto the developer's machine. Any spawned
//! helper server is bounded by a Drop-kill guard plus a readiness poll, so a
//! test can never hang. Tool-dependent tests gate on tool presence and no-op
//! (pass) when the tool is absent. No dependence on Docker / Redis / Doppler /
//! the real network.

use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// A fresh, isolated HOME (so `~/.wb` is sandboxed).
fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// A `wb` command with an isolated HOME and a quiet, deterministic stderr.
/// `WB_SQL_URL` / `DATABASE_URL` are scrubbed so the `sql` no-connection path
/// is reachable regardless of the developer's shell.
fn wb(home: &std::path::Path) -> Command {
    let mut c = Command::new(wb_binary());
    c.env("HOME", home)
        .env("WB_LOG_LEVEL", "error")
        .env_remove("WB_CONFIG_PATH")
        .env_remove("WB_SQL_URL")
        .env_remove("DATABASE_URL")
        .env_remove("WB_CALLBACK_URL")
        .env_remove("WB_CALLBACK_SECRET")
        .env_remove("WB_REQUIRE_TRUST");
    c
}

fn code(o: &std::process::Output) -> i32 {
    o.status.code().unwrap_or(-1)
}
fn out(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn both(o: &std::process::Output) -> String {
    format!("{}{}", out(o), err(o))
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Pick an ephemeral TCP port by binding :0, then releasing it. The window
/// between release and the helper re-binding is tiny and local-only.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A spawned helper process bounded to the test: killed + reaped on Drop so a
/// test can never leak a server or hang.
struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Block until `port` accepts a TCP connection (server is up) or `timeout`
/// elapses. Returns whether the server became reachable.
fn wait_until_listening(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

// ---------------------------------------------------------------------------
// HTTP runtime (gated on python3 + curl)
// ---------------------------------------------------------------------------

/// Serve `dir` over `python3 -m http.server <port>`; returns a kill-guard.
fn serve_dir(dir: &std::path::Path, port: u16) -> Option<ServerGuard> {
    let child = Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
        ])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let guard = ServerGuard(child);
    if !wait_until_listening(port, Duration::from_secs(8)) {
        return None;
    }
    Some(guard)
}

#[test]
fn http_get_2xx_passes_expect_fence() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping http_get_2xx_passes_expect_fence");
        return;
    }
    let h = home();
    let srv_dir = home();
    std::fs::write(srv_dir.path().join("payload.txt"), "HTTP_BODY_TOKEN\n").unwrap();
    let port = free_port();
    let Some(_guard) = serve_dir(srv_dir.path(), port) else {
        eprintln!("http server didn't come up — skipping");
        return;
    };

    // An http block GET, with an env-substituted header to also exercise the
    // runtime's `$VAR` substitution path. `wb test` evaluates the expect fence.
    let wbf = h.path().join("get.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\nenv:\n  TOKVAL: abc\n---\n```http\nGET http://127.0.0.1:{port}/payload.txt\nX-Tok: $TOKVAL\n```\n```expect\nexit 0\nstdout contains \"HTTP_BODY_TOKEN\"\n```\n"
        ),
    )
    .unwrap();

    let o = wb(h.path())
        .args(["test", wbf.to_str().unwrap(), "-q"])
        .output()
        .unwrap();
    assert_eq!(
        code(&o),
        0,
        "http GET should pass expect fence:\n{}",
        both(&o)
    );
}

#[test]
fn http_404_is_http_status_failure() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping http_404_is_http_status_failure");
        return;
    }
    let h = home();
    let srv_dir = home();
    // Empty dir → any path 404s.
    let port = free_port();
    let Some(_guard) = serve_dir(srv_dir.path(), port) else {
        eprintln!("http server didn't come up — skipping");
        return;
    };

    let wbf = h.path().join("miss.md");
    std::fs::write(
        &wbf,
        format!("---\nruntime: bash\n---\n```http\nGET http://127.0.0.1:{port}/nope-not-here.txt\n```\n"),
    )
    .unwrap();

    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "a 404 http block should fail the run:\n{}",
        both(&o)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&o.stdout).expect("--json stdout should parse");
    assert_eq!(
        v["results"][0]["error_type"], "http_status",
        "404 should carry error_type=http_status:\n{v}"
    );
}

#[test]
fn http_invalid_request_body_errors_without_network() {
    // An http block whose body has no request line is rejected before curl is
    // ever launched (error_type=http_invalid). No server needed.
    let h = home();
    let wbf = h.path().join("bad.md");
    std::fs::write(
        &wbf,
        "---\nruntime: bash\n---\n```http\n# only a comment, no request line\n```\n",
    )
    .unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "an invalid http block should fail:\n{}",
        both(&o)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&o.stdout).expect("--json stdout should parse");
    assert_eq!(v["results"][0]["error_type"], "http_invalid", "got:\n{v}");
}

// ---------------------------------------------------------------------------
// SQL runtime (gated on sqlite3)
// ---------------------------------------------------------------------------

#[test]
fn sql_sqlite_url_scheme_runs_query() {
    if !have("sqlite3") {
        eprintln!("sqlite3 unavailable — skipping sql_sqlite_url_scheme_runs_query");
        return;
    }
    let h = home();
    let dir = home();
    let db = dir.path().join("s.db");
    // The `sqlite:<path>` URL form (distinct from the bare-path form covered in
    // cli_smoke.rs) exercises the scheme-strip branch in execute_sql.
    let wbf = dir.path().join("q.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\nenv:\n  WB_SQL_URL: sqlite:{}\n---\n```sql\nCREATE TABLE t(x);\n```\n```sql\nINSERT INTO t VALUES ('row-token-xyz');\n```\n```sql\nSELECT x FROM t;\n```\n",
            db.display()
        ),
    )
    .unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "sqlite: URL run should succeed:\n{}", both(&o));
    assert!(
        out(&o).contains("row-token-xyz"),
        "SELECT row missing:\n{}",
        out(&o)
    );
}

#[test]
fn sql_query_error_is_nonzero() {
    if !have("sqlite3") {
        eprintln!("sqlite3 unavailable — skipping sql_query_error_is_nonzero");
        return;
    }
    let h = home();
    let dir = home();
    let db = dir.path().join("s.db");
    let wbf = dir.path().join("err.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\nenv:\n  WB_SQL_URL: sqlite:{}\n---\n```sql\nSELECT * FROM no_such_table_here;\n```\n",
            db.display()
        ),
    )
    .unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "a bad query should fail the run:\n{}",
        both(&o)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&o.stdout).expect("--json stdout should parse");
    assert_eq!(v["results"][0]["error_type"], "sql_error", "got:\n{v}");
}

#[test]
fn sql_without_connection_errors() {
    if !have("sqlite3") {
        eprintln!("sqlite3 unavailable — skipping sql_without_connection_errors");
        return;
    }
    let h = home();
    let dir = home();
    let wbf = dir.path().join("noconn.md");
    // No WB_SQL_URL / DATABASE_URL anywhere (scrubbed by wb()).
    std::fs::write(&wbf, "---\nruntime: bash\n---\n```sql\nSELECT 1;\n```\n").unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "no connection should fail the run:\n{}",
        both(&o)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&o.stdout).expect("--json stdout should parse");
    assert_eq!(
        v["results"][0]["error_type"], "sql_no_connection",
        "got:\n{v}"
    );
}

// ---------------------------------------------------------------------------
// Secret providers wired through the CLI
// ---------------------------------------------------------------------------

const SECRET_ECHO_WB: &str =
    "---\nruntime: bash\n---\n```bash\necho \"SECRETVAL=$SECRETVAL\"\n```\n";

#[test]
fn secrets_command_provider_injects() {
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--secrets",
            "command",
            "--secrets-cmd",
            "echo SECRETVAL=from-command",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        out(&o).contains("SECRETVAL=from-command"),
        "got:\n{}",
        out(&o)
    );
}

#[test]
fn secrets_cmd_alias_injects() {
    // The `cmd` alias dispatches to the same shell-command provider as `command`.
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--secrets",
            "cmd",
            "--secrets-cmd",
            "printf 'SECRETVAL=via-printf\\n'",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        out(&o).contains("SECRETVAL=via-printf"),
        "got:\n{}",
        out(&o)
    );
}

#[test]
fn secrets_dotenv_explicit_path_injects() {
    let h = home();
    let dir = home();
    let envf = dir.path().join("vault.env");
    std::fs::write(&envf, "SECRETVAL=from-dotenv\n").unwrap();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--secrets",
            "dotenv",
            "--secrets-cmd",
            envf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        out(&o).contains("SECRETVAL=from-dotenv"),
        "got:\n{}",
        out(&o)
    );
}

#[test]
fn secrets_dotenv_default_dotfile_in_cwd() {
    // With no `--secrets-cmd`, the dotenv provider reads `.env` from the process
    // CWD. Run with current_dir set to the dir holding the `.env`.
    let h = home();
    let dir = home();
    std::fs::write(dir.path().join(".env"), "SECRETVAL=from-default-dotenv\n").unwrap();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .current_dir(dir.path())
        .args(["run", wbf.to_str().unwrap(), "--secrets", "dotenv"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        out(&o).contains("SECRETVAL=from-default-dotenv"),
        "got:\n{}",
        out(&o)
    );
}

#[test]
fn secrets_env_provider_runs_and_inherits_exported_var() {
    // The `env` provider with no `keys:` resolves nothing, but a normally
    // exported var still reaches the block via process-env inheritance. Covers
    // the resolve_env (empty) path without erroring the run.
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .env("SECRETVAL", "from-parent-env")
        .args(["run", wbf.to_str().unwrap(), "--secrets", "env"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        out(&o).contains("SECRETVAL=from-parent-env"),
        "got:\n{}",
        out(&o)
    );
}

#[test]
fn secrets_project_flag_passes_through_to_command_provider() {
    // `--project` is consumed by build_secrets_config (and ignored by the
    // command provider). Assert it plumbs through without breaking the run.
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--secrets",
            "command",
            "--secrets-cmd",
            "echo SECRETVAL=ok",
            "--project",
            "my-project",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(out(&o).contains("SECRETVAL=ok"), "got:\n{}", out(&o));
}

#[test]
fn secrets_unknown_provider_fails() {
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--secrets", "bogus-provider"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "unknown provider should fail the run:\n{}",
        both(&o)
    );
    assert!(
        both(&o).contains("Unknown secret provider"),
        "should name the bad provider:\n{}",
        both(&o)
    );
}

#[test]
fn secrets_command_without_cmd_fails() {
    // The command provider requires a `command` (`--secrets-cmd`); omitting it
    // is a secrets error.
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, SECRET_ECHO_WB).unwrap();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--secrets", "command"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "command provider w/o command should fail:\n{}",
        both(&o)
    );
}

// ---------------------------------------------------------------------------
// --repair against a responsive endpoint (gated on python3 + curl)
// ---------------------------------------------------------------------------

/// Spawn a tiny local HTTP server that answers every POST with
/// `{"action": "<action>"}` (200, application/json). Returns the guard + port.
fn spawn_repair_server(dir: &std::path::Path, action: &str) -> Option<(ServerGuard, u16)> {
    let port = free_port();
    let script = dir.join("repair_server.py");
    std::fs::write(
        &script,
        r#"import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
ACTION = sys.argv[1]
PORT = int(sys.argv[2])
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get('Content-Length', 0) or 0)
        if n:
            self.rfile.read(n)
        body = ('{"action": "%s"}' % ACTION).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass
HTTPServer(('127.0.0.1', PORT), H).serve_forever()
"#,
    )
    .ok()?;
    let child = Command::new("python3")
        .arg(&script)
        .arg(action)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let guard = ServerGuard(child);
    if !wait_until_listening(port, Duration::from_secs(8)) {
        return None;
    }
    Some((guard, port))
}

const REPAIR_TWO_BLOCK_WB: &str =
    "---\nruntime: bash\n---\n```bash\n( exit 1 )\n```\n```bash\necho SECOND_BLOCK_RAN\n```\n";

#[test]
fn repair_skip_continues_past_failure() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping repair_skip_continues_past_failure");
        return;
    }
    let h = home();
    let dir = home();
    let Some((_g, port)) = spawn_repair_server(dir.path(), "skip") else {
        eprintln!("repair server didn't come up — skipping");
        return;
    };
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, REPAIR_TWO_BLOCK_WB).unwrap();
    let url = format!("http://127.0.0.1:{port}/");
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--bail", "--repair", &url])
        .output()
        .unwrap();
    // skip suppresses the bail, so the second block still runs.
    assert!(
        out(&o).contains("SECOND_BLOCK_RAN"),
        "repair skip should continue past the failed block:\n{}",
        both(&o)
    );
}

#[test]
fn repair_abort_halts_the_run() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping repair_abort_halts_the_run");
        return;
    }
    let h = home();
    let dir = home();
    let Some((_g, port)) = spawn_repair_server(dir.path(), "abort") else {
        eprintln!("repair server didn't come up — skipping");
        return;
    };
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, REPAIR_TWO_BLOCK_WB).unwrap();
    let url = format!("http://127.0.0.1:{port}/");
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--bail", "--repair", &url])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "abort under --bail should fail the run:\n{}",
        both(&o)
    );
    assert!(
        !out(&o).contains("SECOND_BLOCK_RAN"),
        "repair abort should bail before the second block:\n{}",
        both(&o)
    );
}

#[test]
fn repair_rerun_recovers_and_continues() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping repair_rerun_recovers_and_continues");
        return;
    }
    let h = home();
    let dir = home();
    let Some((_g, port)) = spawn_repair_server(dir.path(), "rerun") else {
        eprintln!("repair server didn't come up — skipping");
        return;
    };
    // Block 1 fails the first time, then succeeds once a marker file exists, so
    // a single `rerun` makes it pass deterministically. Block 2 then runs.
    let marker = dir.path().join("attempted.flag");
    let wbf = dir.path().join("w.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\n---\n```bash\nif [ -f '{m}' ]; then echo BLOCK1_RECOVERED; else touch '{m}'; exit 1; fi\n```\n```bash\necho SECOND_BLOCK_RAN\n```\n",
            m = marker.display()
        ),
    )
    .unwrap();
    let url = format!("http://127.0.0.1:{port}/");
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--bail", "--repair", &url])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "rerun should recover the run:\n{}", both(&o));
    assert!(
        out(&o).contains("BLOCK1_RECOVERED"),
        "rerun should re-execute block 1:\n{}",
        out(&o)
    );
    assert!(
        out(&o).contains("SECOND_BLOCK_RAN"),
        "run should continue after recovery:\n{}",
        out(&o)
    );
}

#[test]
fn repair_max_zero_exhausts_budget_and_aborts() {
    if !(have("python3") && have("curl")) {
        eprintln!("python3/curl unavailable — skipping repair_max_zero_exhausts_budget_and_aborts");
        return;
    }
    let h = home();
    let dir = home();
    let Some((_g, port)) = spawn_repair_server(dir.path(), "rerun") else {
        eprintln!("repair server didn't come up — skipping");
        return;
    };
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, REPAIR_TWO_BLOCK_WB).unwrap();
    let url = format!("http://127.0.0.1:{port}/");
    // --repair-max 0 → the rerun action has no budget, so the loop breaks and
    // the failure stands (bails before block 2).
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--repair",
            &url,
            "--repair-max",
            "0",
        ])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "exhausted budget should fail:\n{}", both(&o));
    assert!(
        !out(&o).contains("SECOND_BLOCK_RAN"),
        "no rerun budget → bail before block 2:\n{}",
        both(&o)
    );
}

#[test]
fn repair_max_non_integer_is_usage_error() {
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(&wbf, REPAIR_TWO_BLOCK_WB).unwrap();
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--repair",
            "http://127.0.0.1:9/",
            "--repair-max",
            "not-an-int",
        ])
        .output()
        .unwrap();
    assert_eq!(
        code(&o),
        2,
        "a non-integer --repair-max is a clap usage error:\n{}",
        both(&o)
    );
}

// ---------------------------------------------------------------------------
// capture: interactive REPL + plain non-assert stdin
// ---------------------------------------------------------------------------

/// Drive a capture invocation by piping `stdin_payload`, returning the Output.
fn run_capture(home: &std::path::Path, args: &[&str], stdin_payload: &str) -> std::process::Output {
    let mut child = wb(home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wb capture");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin_payload.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait capture")
}

#[test]
fn capture_interactive_pipe_emits_runnable_workbook() {
    let h = home();
    let dir = home();
    let outp = dir.path().join("repl.md");
    // `-i` reads one command per line from stdin (EOF ends the session), runs
    // each live, and records it. With --assert each gets an expect fence.
    let o = run_capture(
        h.path(),
        &[
            "capture",
            "-i",
            "--assert",
            "--title",
            "REPL",
            "-o",
            outp.to_str().unwrap(),
        ],
        "echo repl-one\necho repl-two\n",
    );
    assert_eq!(
        code(&o),
        0,
        "interactive capture should succeed:\n{}",
        both(&o)
    );
    let md = std::fs::read_to_string(&outp).unwrap();
    assert!(md.contains("```bash"), "should emit a bash block:\n{md}");
    assert!(
        md.contains("echo repl-one"),
        "first command recorded:\n{md}"
    );
    assert!(
        md.contains("echo repl-two"),
        "second command recorded:\n{md}"
    );
    assert!(
        md.contains("```expect"),
        "--assert should add expect fences:\n{md}"
    );

    // The recorded workbook itself re-runs green.
    let t = wb(h.path())
        .args(["test", outp.to_str().unwrap(), "-q"])
        .output()
        .unwrap();
    assert_eq!(code(&t), 0, "recorded workbook should pass:\n{}", both(&t));
}

#[test]
fn capture_noninteractive_no_assert_to_stdout() {
    let h = home();
    // No -o → workbook goes to stdout; no --assert → no expect fences.
    let o = run_capture(h.path(), &["capture"], "echo plain-capture\n");
    assert_eq!(code(&o), 0, "plain capture should succeed:\n{}", both(&o));
    let md = out(&o);
    assert!(
        md.contains("```bash"),
        "stdout should carry a bash block:\n{md}"
    );
    assert!(md.contains("echo plain-capture"), "command recorded:\n{md}");
    assert!(
        !md.contains("```expect"),
        "no --assert → no expect fences:\n{md}"
    );
}

#[test]
fn capture_empty_stdin_is_usage_error() {
    let h = home();
    // No commands on stdin → usage error (exit 2).
    let o = run_capture(h.path(), &["capture"], "");
    assert_eq!(
        code(&o),
        2,
        "empty capture input should be a usage error:\n{}",
        both(&o)
    );
}

// ---------------------------------------------------------------------------
// Misc bare-invocation back-compat flags + transform + folder -o
// ---------------------------------------------------------------------------

#[test]
fn bare_inspect_flag_shows_structure_without_running() {
    let h = home();
    let dir = home();
    // A side-effect block: if it ran, the marker file would exist. inspect must
    // not run it. (We can't key off the echoed text — inspect prints a command
    // preview that includes it.)
    let marker = dir.path().join("ran.flag");
    let wbf = dir.path().join("w.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\n---\n```bash\ntouch '{}'\n```\n",
            marker.display()
        ),
    )
    .unwrap();
    // The hidden back-compat `--inspect` flag routes to cmd_inspect.
    let o = wb(h.path())
        .arg(wbf.to_str().unwrap())
        .arg("--inspect")
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        !marker.exists(),
        "inspect must not execute the block (marker created)"
    );
    assert!(
        both(&o).contains("bash"),
        "inspect should describe the block:\n{}",
        both(&o)
    );
}

#[test]
fn bare_inspect_short_i_matches_long_form() {
    let h = home();
    let dir = home();
    let marker = dir.path().join("ran.flag");
    let wbf = dir.path().join("w.md");
    std::fs::write(
        &wbf,
        format!(
            "---\nruntime: bash\n---\n```bash\ntouch '{}'\n```\n",
            marker.display()
        ),
    )
    .unwrap();
    let o = wb(h.path())
        .arg(wbf.to_str().unwrap())
        .arg("-i")
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(!marker.exists(), "-i (inspect) must not execute the block");
}

#[test]
fn bare_verbose_flag_still_runs() {
    let h = home();
    let dir = home();
    let wbf = dir.path().join("w.md");
    std::fs::write(
        &wbf,
        "---\nruntime: bash\n---\n```bash\necho VERBOSE_OK\n```\n",
    )
    .unwrap();
    // The hidden `--verbose` flag is accepted and the run proceeds normally.
    let o = wb(h.path())
        .arg(wbf.to_str().unwrap())
        .arg("--verbose")
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr:\n{}", err(&o));
    assert!(
        both(&o).contains("VERBOSE_OK"),
        "verbose run should still execute:\n{}",
        both(&o)
    );
}

#[test]
fn bare_no_file_prints_usage_and_exits_two() {
    let h = home();
    let o = wb(h.path()).output().unwrap();
    assert_eq!(code(&o), 2, "no file → usage exit 2:\n{}", both(&o));
    assert!(
        err(&o).contains("usage: wb"),
        "should print short usage:\n{}",
        err(&o)
    );
}

#[test]
fn transform_reports_referenced_variables() {
    let h = home();
    let dir = home();
    let wbf = dir.path().join("tpl.md");
    // A `{{region}}` reference not declared under vars: → reported as a var,
    // and flagged undefined on stderr. Exits 0 (advisory).
    std::fs::write(
        &wbf,
        "---\nruntime: bash\n---\n```bash\necho deploy to {{region}}\n```\n",
    )
    .unwrap();
    let o = wb(h.path())
        .args(["transform", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "transform should exit 0:\n{}", both(&o));
    assert!(
        out(&o).contains("vars:"),
        "should print a vars: block:\n{}",
        out(&o)
    );
    assert!(
        out(&o).contains("region"),
        "should list the referenced var:\n{}",
        out(&o)
    );
    assert!(
        err(&o).contains("undefined"),
        "should flag the undefined var:\n{}",
        err(&o)
    );
}

#[test]
fn folder_run_with_output_file_aggregates_both() {
    let h = home();
    let dir = home();
    std::fs::write(
        dir.path().join("a.md"),
        "---\nruntime: bash\n---\n```bash\necho FOLDER_A_TOKEN\n```\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.md"),
        "---\nruntime: bash\n---\n```bash\necho FOLDER_B_TOKEN\n```\n",
    )
    .unwrap();
    let report = h.path().join("report.md");
    let o = wb(h.path())
        .arg(dir.path())
        .args(["-q", "-o"])
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "folder run should exit 0:\n{}", both(&o));
    let written = std::fs::read_to_string(&report).expect("aggregated report written");
    // A folder run writes an aggregated per-workbook summary table (not raw
    // block stdout): both files plus a run summary line.
    assert!(
        written.contains("a.md"),
        "report should list a.md:\n{written}"
    );
    assert!(
        written.contains("b.md"),
        "report should list b.md:\n{written}"
    );
    assert!(
        written.contains("Ran 2 workbooks"),
        "report should carry the run summary:\n{written}"
    );
}

//! Integration coverage for wb's *real* Redis paths, exercised end-to-end by
//! spawning the actual `wb` binary against a locally-spawned `redis-server`.
//!
//! Targets:
//!   - src/callback.rs  `CallbackConfig::send_redis`  (XADD to a stream)
//!   - src/signal.rs    `read_signal` / `find_ready_signal` (GET + DEL)
//!
//! Discovered Redis layout (from the source):
//!   - Callbacks: `XADD <stream_key> * event <event> data <json-payload>`,
//!     where `stream_key` defaults to `wb:events` (override via --callback-key
//!     / WB_CALLBACK_KEY). One stream entry per emitted event.
//!   - Signals: `read_signal` does `GET <WB_SIGNAL_KEY>:<checkpoint_id>` and,
//!     when present, `DEL`s the same key. The value is a JSON object whose
//!     keys are bound into the resumed workbook (or a JSON scalar bound to the
//!     single bind name). `config_from_env` requires BOTH WB_SIGNAL_URL and
//!     WB_SIGNAL_KEY to be set.
//!   - `find_ready_signal` scans `pending::list_all()` (reads WB_CHECKPOINT_DIR)
//!     and calls `read_signal` for each pending id — driven by `wb resume`
//!     with no explicit id.
//!
//! Note: `signal::archive_signal` (SET ... EX) has no CLI caller (it is
//! `#[allow(dead_code)]`) and lives in a private module, so it cannot be
//! reached by spawning the binary. It is left to the existing in-crate unit
//! tests in src/signal.rs.
//!
//! The whole file is gated: if `redis-server` is not installed or fails to
//! come up, every test early-returns (passes) so the suite never hard-fails on
//! a machine without Redis.

use std::io::Read;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// Result of a time-bounded child process run.
struct CmdOut {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

/// Run a command with a hard wall-clock cap. Captures stdout/stderr; on
/// timeout the child is killed and `timed_out` is set. Used so a hung `wb`
/// or `redis-cli` invocation fails the test instead of stalling the suite.
fn run_bounded(cmd: &mut Command, secs: u64) -> CmdOut {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn child");

    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait().expect("try_wait") {
            Some(s) => break Some(s),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    if timed_out {
        let _ = child.wait();
    }

    CmdOut {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
    }
}

/// Pick a free TCP port by binding to :0 and immediately releasing it.
fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

/// A locally-spawned `redis-server` on a private port. Killed on drop.
struct RedisServer {
    child: Child,
    port: u16,
    _dir: tempfile::TempDir,
}

impl RedisServer {
    /// Start a fresh `redis-server`. Returns `None` if redis isn't installed
    /// or fails to answer PING within ~3s, so callers can pass-skip.
    fn start() -> Option<RedisServer> {
        // Gate: redis-server must be on PATH.
        let installed = Command::new("redis-server")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !installed {
            return None;
        }

        let port = free_port();
        let dir = tempfile::tempdir().ok()?;
        let child = Command::new("redis-server")
            .args([
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
                "--daemonize",
                "no",
            ])
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let server = RedisServer {
            child,
            port,
            _dir: dir,
        };

        // Poll until PING returns PONG (or give up after ~3s).
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if Instant::now() >= deadline {
                // server drops here → child killed.
                return None;
            }
            let out = Command::new("redis-cli")
                .args(["-p", &port.to_string(), "ping"])
                .output();
            if let Ok(o) = out {
                if String::from_utf8_lossy(&o.stdout).contains("PONG") {
                    return Some(server);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn url(&self) -> String {
        format!("redis://127.0.0.1:{}", self.port)
    }
}

impl Drop for RedisServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Run `redis-cli -p <port> <args...>`, time-bounded.
fn redis_cli(port: u16, args: &[&str]) -> CmdOut {
    let mut c = Command::new("redis-cli");
    c.arg("-p").arg(port.to_string()).args(args);
    run_bounded(&mut c, 5)
}

/// A two-block bash workbook (two step.complete + one run.complete = 3 events).
fn write_two_block_workbook(dir: &std::path::Path) -> std::path::PathBuf {
    let wb = dir.join("cb.md");
    std::fs::write(&wb, "```bash\necho one\n```\n\n```bash\necho two\n```\n").unwrap();
    wb
}

/// A wait/pause workbook that binds `approved` then echoes it.
fn write_wait_workbook(dir: &std::path::Path) -> std::path::PathBuf {
    let wb = dir.join("wait.md");
    std::fs::write(
        &wb,
        r#"```wait {#approval}
kind: manual
bind: approved
timeout: 5m
on_timeout: abort
```

```bash {#after}
echo "approved=$approved"
```
"#,
    )
    .unwrap();
    wb
}

// ---------------------------------------------------------------------------
// CALLBACK over Redis — drives CallbackConfig::send_redis (XADD).
// ---------------------------------------------------------------------------

#[test]
fn redis_callback_publishes_step_and_run_events() {
    let Some(server) = RedisServer::start() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let wb = write_two_block_workbook(dir.path());

    let mut c = Command::new(wb_binary());
    c.args(["run", wb.to_str().unwrap(), "--callback", &server.url()])
        .env("HOME", &home)
        .env("WB_CHECKPOINT_DIR", dir.path().join("ckpt"));
    let out = run_bounded(&mut c, 30);

    assert!(!out.timed_out, "wb run hung; stderr:\n{}", out.stderr);
    assert_eq!(out.code, Some(0), "wb run failed; stderr:\n{}", out.stderr);

    // Default stream key is `wb:events`. Two blocks + run.complete = 3 entries.
    let xlen = redis_cli(server.port, &["XLEN", "wb:events"]);
    let n: i64 = xlen.stdout.trim().parse().unwrap_or(-1);
    assert!(
        n >= 3,
        "expected >=3 stream entries, got {} (raw: {:?})",
        n,
        xlen.stdout
    );

    // The XADD payloads carry the event names in the `event` field; the raw
    // XRANGE dump contains them as substrings.
    let xr = redis_cli(server.port, &["XRANGE", "wb:events", "-", "+"]);
    assert!(
        xr.stdout.contains("step.complete"),
        "stream missing step.complete:\n{}",
        xr.stdout
    );
    assert!(
        xr.stdout.contains("run.complete"),
        "stream missing run.complete:\n{}",
        xr.stdout
    );
}

#[test]
fn redis_callback_via_env_var_and_custom_stream_key() {
    let Some(server) = RedisServer::start() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let wb = dir.path().join("one.md");
    std::fs::write(&wb, "```bash\necho hello\n```\n").unwrap();

    // No --callback flag: URL comes from WB_CALLBACK_URL. Custom stream key via
    // WB_CALLBACK_KEY. A callback secret on a redis URL is meaningless (HMAC is
    // HTTP-only) and must surface the upfront "Redis stream" warning.
    let mut c = Command::new(wb_binary());
    c.args(["run", wb.to_str().unwrap()])
        .env("HOME", &home)
        .env("WB_CHECKPOINT_DIR", dir.path().join("ckpt"))
        .env("WB_CALLBACK_URL", server.url())
        .env("WB_CALLBACK_KEY", "custom:stream")
        .env("WB_CALLBACK_SECRET", "hmac-key");
    let out = run_bounded(&mut c, 30);

    assert!(!out.timed_out, "wb run hung; stderr:\n{}", out.stderr);
    assert_eq!(out.code, Some(0), "wb run failed; stderr:\n{}", out.stderr);
    assert!(
        out.stderr.contains("Redis"),
        "expected redis+secret warning on stderr:\n{}",
        out.stderr
    );

    // Custom key got the events (1 block + run.complete = 2).
    let xlen = redis_cli(server.port, &["XLEN", "custom:stream"]);
    let n: i64 = xlen.stdout.trim().parse().unwrap_or(-1);
    assert!(
        n >= 2,
        "expected >=2 entries on custom:stream, got {} (raw: {:?})",
        n,
        xlen.stdout
    );
    let xr = redis_cli(server.port, &["XRANGE", "custom:stream", "-", "+"]);
    assert!(
        xr.stdout.contains("run.complete"),
        "custom stream missing run.complete:\n{}",
        xr.stdout
    );

    // The default key must be untouched (events went to the custom key).
    let default_len = redis_cli(server.port, &["XLEN", "wb:events"]);
    assert_eq!(
        default_len.stdout.trim(),
        "0",
        "default wb:events should be empty when a custom key is set"
    );
}

#[test]
fn bad_redis_callback_url_does_not_hang_and_run_completes() {
    let Some(_server) = RedisServer::start() else {
        // Even this negative test needs the gate so it skips on no-redis hosts.
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let wb = dir.path().join("one.md");
    std::fs::write(&wb, "```bash\necho hi\n```\n").unwrap();

    // Port 1 has nothing listening → immediate connection refused. send_redis
    // must log a warning and let the run finish successfully (exit 0), never
    // hang.
    let mut c = Command::new(wb_binary());
    c.args([
        "run",
        wb.to_str().unwrap(),
        "--callback",
        "redis://127.0.0.1:1",
    ])
    .env("HOME", &home)
    .env("WB_CHECKPOINT_DIR", dir.path().join("ckpt"));
    let out = run_bounded(&mut c, 30);

    assert!(!out.timed_out, "wb run hung on unreachable redis");
    assert_eq!(
        out.code,
        Some(0),
        "run should still complete despite bad redis callback; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.to_lowercase().contains("redis"),
        "expected a redis callback warning on stderr:\n{}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// SIGNAL over Redis — drives signal::read_signal and find_ready_signal.
// ---------------------------------------------------------------------------

/// Run the wait workbook to a pause (exit 42) under the given checkpoint id.
fn pause_wait_workbook(
    wb: &std::path::Path,
    checkpoint_dir: &std::path::Path,
    home: &std::path::Path,
    checkpoint_id: &str,
) {
    let mut c = Command::new(wb_binary());
    c.args(["run", wb.to_str().unwrap(), "--checkpoint", checkpoint_id])
        .env("HOME", home)
        .env("WB_CHECKPOINT_DIR", checkpoint_dir);
    let out = run_bounded(&mut c, 30);
    assert!(!out.timed_out, "wait run hung; stderr:\n{}", out.stderr);
    assert_eq!(
        out.code,
        Some(42),
        "wait run should pause (exit 42); stderr:\n{}",
        out.stderr
    );
}

#[test]
fn redis_signal_read_on_resume_with_explicit_id() {
    let Some(server) = RedisServer::start() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let ckpt = dir.path().join("ckpt");
    std::fs::create_dir_all(&home).unwrap();
    let wb = write_wait_workbook(dir.path());
    let id = "redis-sig-explicit";

    pause_wait_workbook(&wb, &ckpt, &home, id);

    // An external resolver writes the awaited payload to the signal key the way
    // read_signal expects: GET <WB_SIGNAL_KEY>:<id> → a JSON object.
    let signal_key = format!("wbtest:signal:{id}");
    let set = redis_cli(server.port, &["SET", &signal_key, r#"{"approved":"yes"}"#]);
    assert!(set.stdout.contains("OK"), "SET failed: {:?}", set.stdout);

    // Resume with an explicit id and no --value/--signal → wb falls through to
    // read_signal(config, id).
    let mut c = Command::new(wb_binary());
    c.args(["resume", id])
        .env("HOME", &home)
        .env("WB_CHECKPOINT_DIR", &ckpt)
        .env("WB_SIGNAL_URL", server.url())
        .env("WB_SIGNAL_KEY", "wbtest:signal");
    let out = run_bounded(&mut c, 30);

    assert!(!out.timed_out, "resume hung; stderr:\n{}", out.stderr);
    assert_eq!(out.code, Some(0), "resume failed; stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("approved=yes"),
        "resumed step should see the signal-bound var; stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr.contains("signal read from"),
        "expected 'signal read from' notice; stderr:\n{}",
        out.stderr
    );

    // read_signal DELetes the key after reading it.
    let exists = redis_cli(server.port, &["EXISTS", &signal_key]);
    assert_eq!(
        exists.stdout.trim(),
        "0",
        "signal key should be deleted after read; raw: {:?}",
        exists.stdout
    );
}

#[test]
fn redis_signal_find_ready_on_resume_without_id() {
    let Some(server) = RedisServer::start() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let ckpt = dir.path().join("ckpt");
    std::fs::create_dir_all(&home).unwrap();
    let wb = write_wait_workbook(dir.path());
    let id = "redis-sig-autodetect";

    pause_wait_workbook(&wb, &ckpt, &home, id);

    // Deliver the signal, then resume WITHOUT an id → find_ready_signal scans
    // pending descriptors and read_signal picks it up.
    let signal_key = format!("wbtest:signal:{id}");
    let set = redis_cli(server.port, &["SET", &signal_key, r#"{"approved":"yes"}"#]);
    assert!(set.stdout.contains("OK"), "SET failed: {:?}", set.stdout);

    let mut c = Command::new(wb_binary());
    c.args(["resume"]) // no id → auto-detect
        .env("HOME", &home)
        .env("WB_CHECKPOINT_DIR", &ckpt)
        .env("WB_SIGNAL_URL", server.url())
        .env("WB_SIGNAL_KEY", "wbtest:signal");
    let out = run_bounded(&mut c, 30);

    assert!(!out.timed_out, "resume hung; stderr:\n{}", out.stderr);
    assert_eq!(out.code, Some(0), "resume failed; stderr:\n{}", out.stderr);
    assert!(
        out.stderr.contains("auto-detected pending"),
        "expected auto-detect notice; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("approved=yes"),
        "auto-detected resume should bind the signal var; stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );

    // find_ready_signal → read_signal also DELetes the key.
    let exists = redis_cli(server.port, &["EXISTS", &signal_key]);
    assert_eq!(
        exists.stdout.trim(),
        "0",
        "signal key should be deleted after find_ready_signal read; raw: {:?}",
        exists.stdout
    );
}

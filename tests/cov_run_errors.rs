//! Integration coverage for the error / edge branches of the core run
//! orchestration in `src/lib.rs` (`run_single` / `run_single_collect`) plus the
//! artifact-manifest internals — the paths the other `cov_*` suites leave
//! uncovered.
//!
//! Every test spawns the real `wb` binary so cargo-llvm-cov instruments the
//! production code. Isolation: each spawn gets a fresh `tempfile::tempdir()` as
//! `$HOME`, so all of `~/.wb` (checkpoints, runs, config) is scoped to the test
//! and never touches the developer's machine. A few env vars that a dev shell
//! might set (`WB_CHECKPOINT_DIR`, `WB_CONFIG_PATH`, `WB_CALLBACK_*`) are
//! scrubbed so the tests are deterministic regardless of the host environment.
//!
//! What's covered here, and why it's not a duplicate of the existing suites:
//!   * `retries` — fence `{retries=N}` that eventually succeeds, and one that
//!     exhausts its budget (no other suite drives the retry loop).
//!   * `continue_on_error` — fence-attr form AND the legacy `continue_on_error:
//!     [N]` frontmatter form, both under `--bail` (must CONTINUE, not bail).
//!   * `timeouts` precedence + knob-source diagnostic — `timeouts._default`,
//!     `--default-block-timeout`, and fence `{timeout=}` beating the frontmatter
//!     default (cov_run_flags only covers a bare per-block fence timeout).
//!   * callbacks during a *real* run to a loopback receiver: `step.complete` +
//!     `run.complete` with an `X-WB-Signature` (HMAC) header, and
//!     `checkpoint.failed` under `--bail --checkpoint` (cov_lifecycle only
//!     covers callback-URL *validation*; orchestration_contract never sets an
//!     HMAC secret or asserts `checkpoint.failed`).
//!   * `--events` JSONL with a conditional skip + a timeout (partial flags).
//!   * structured-output export + `{when=$WB_OUT_x=val}` equality gating, and
//!     the malformed-`output:` parse-failure branch.
//!   * artifact manifest internals via `wb artifacts list --run <id> --format
//!     json` (label / description / sha256 / step provenance).
//!   * `--allow-runtime` DENIAL (cov_run_flags only covers the permit case).
//!   * `--redact` masking in `--json`, and a `secret: true` param masked in `-o`.
//!   * include splice numbering + the include-frame `step.started`/
//!     `step.finished` events (`emit_finish_for_stack`), incl. the failure
//!     outcome under `--bail`.
//!   * `prepare_checkpoint` fresh-restart edges not in cov_lifecycle: a params
//!     change and a workbook-path change under a reused checkpoint id.
//!
//! No dependence on Docker / Redis / Doppler / the real network. The callback
//! receiver is a loopback `TcpListener` whose accept loop self-terminates after
//! the expected number of requests or a deadline, so nothing leaks or hangs.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

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

/// A fresh, isolated HOME (so `~/.wb` is sandboxed for this test).
fn home() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// Build a `wb` command with an isolated HOME and a deterministic, quiet
/// environment (dev-shell vars that could redirect state or inject callbacks
/// are scrubbed).
fn wb(home: &Path) -> Command {
    let mut c = Command::new(wb_binary());
    c.env("HOME", home)
        .env_remove("WB_CHECKPOINT_DIR")
        .env_remove("WB_CONFIG_PATH")
        .env_remove("WB_CALLBACK_URL")
        .env_remove("WB_CALLBACK_SECRET")
        .env_remove("WB_CALLBACK_KEY")
        .env_remove("WB_REQUIRE_TRUST");
    c
}

/// Write `body` to `<dir>/<name>` and return the absolute path.
fn write_wb(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
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

/// Parse the `--json` document from a `-q --json` run's stdout.
fn json_stdout(o: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("--json stdout should parse ({e}):\n{}", both(o)))
}

// ---------------------------------------------------------------------------
// Loopback callback receiver (real HTTP over TCP, headers + JSON body)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CapturedRequest {
    headers: String,
    body: serde_json::Value,
}

impl CapturedRequest {
    fn event(&self) -> String {
        self.body["event"].as_str().unwrap_or("").to_string()
    }
}

/// Bind a loopback listener and accept exactly `expected` POSTs (or give up
/// after a deadline). Returns the base URL plus a channel of parsed requests.
/// The accept loop runs on a detached thread that self-terminates, so there is
/// nothing to kill and no way to leak a server.
fn start_callback_sink(expected: usize) -> (String, mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind callback listener");
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut accepted = 0;
        while accepted < expected && Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    accepted += 1;
                    if let Some(req) = read_request(stream) {
                        let _ = tx.send(req);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    (format!("http://127.0.0.1:{port}/hook"), rx)
}

fn read_request(mut stream: TcpStream) -> Option<CapturedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 2048];
    let mut header_end = None;
    let mut content_length = None;

    loop {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if header_end.is_none() {
            header_end = find_header_end(&bytes);
            if let Some(end) = header_end {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_string();
                content_length = parse_content_length(&headers);
            }
        }
        if let (Some(end), Some(len)) = (header_end, content_length) {
            if bytes.len() >= end + 4 + len {
                break;
            }
        }
    }

    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.shutdown(Shutdown::Both);

    let end = header_end?;
    let headers = String::from_utf8_lossy(&bytes[..end]).to_string();
    let body_start = end + 4;
    let body_len = content_length.unwrap_or_else(|| bytes.len().saturating_sub(body_start));
    let body_end = body_start.saturating_add(body_len).min(bytes.len());
    let body = serde_json::from_slice(&bytes[body_start..body_end]).ok()?;
    Some(CapturedRequest { headers, body })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

/// Collect up to `n` requests, stopping early if the sink stops delivering.
fn drain_requests(rx: &mpsc::Receiver<CapturedRequest>, n: usize) -> Vec<CapturedRequest> {
    let mut v = Vec::new();
    for _ in 0..n {
        match rx.recv_timeout(Duration::from_secs(6)) {
            Ok(req) => v.push(req),
            Err(_) => break,
        }
    }
    v
}

// ===========================================================================
// retries
// ===========================================================================

/// A block that fails its first attempt then succeeds (a counter file in the
/// `-C` workdir gates it) passes once the retry budget kicks in. Drives the
/// success branch of `execute_block_with_policy`'s retry loop.
#[test]
fn retries_eventually_succeeds() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "retry.md",
        "---\nruntime: bash\n---\n```bash {retries=2}\n\
         n=$(cat c 2>/dev/null||echo 0); n=$((n+1)); echo $n>c; [ $n -ge 2 ]\n```\n",
    );
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-C",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        code(&o),
        0,
        "a block that passes on attempt 2 should succeed:\n{}",
        both(&o)
    );
    assert!(
        err(&o).contains("retry 1/2"),
        "a retry should have been logged:\n{}",
        err(&o)
    );
}

/// A block that always fails exhausts its retry budget and the run fails. Drives
/// the `attempt >= total_attempts` exit branch of the retry loop.
#[test]
fn retries_exhausted_fails_the_run() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "rx.md",
        "---\nruntime: bash\n---\n```bash {retries=2}\nfalse\n```\n",
    );
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "exhausted retries should fail the run:\n{}",
        both(&o)
    );
    let v = json_stdout(&o);
    assert_eq!(v["status"], "fail", "run status should be fail:\n{v}");
    // 2 additional attempts → two "retry N/2" lines on stderr.
    let retries = err(&o).matches("retry").count();
    assert_eq!(retries, 2, "expected 2 retry log lines:\n{}", err(&o));
}

// ===========================================================================
// continue_on_error  (fence attr + legacy frontmatter map), under --bail
// ===========================================================================

/// `{continue_on_error}` on a failing block suppresses the `--bail` halt: the
/// next block still runs, but the run's exit reflects the failure.
#[test]
fn continue_on_error_fence_attr_continues_under_bail() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "coe.md",
        "---\nruntime: bash\n---\n```bash {continue_on_error}\nfalse\n```\n\
         ```bash\necho SECOND_RAN\n```\n",
    );
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap()])
        .arg("--bail")
        .output()
        .unwrap();
    assert!(
        out(&o).contains("SECOND_RAN"),
        "continue_on_error should let the run pass the failed block under --bail:\n{}",
        both(&o)
    );
    assert_ne!(
        code(&o),
        0,
        "the recorded block failure should still fail the run:\n{}",
        both(&o)
    );
}

/// The legacy `continue_on_error: [N]` frontmatter map has the same effect as
/// the fence attr (covers the frontmatter-keyed policy resolution path).
#[test]
fn continue_on_error_legacy_frontmatter_continues_under_bail() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "coe2.md",
        "---\nruntime: bash\ncontinue_on_error: [1]\n---\n```bash\nfalse\n```\n\
         ```bash\necho SECOND_RAN\n```\n",
    );
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap()])
        .arg("--bail")
        .output()
        .unwrap();
    assert!(
        out(&o).contains("SECOND_RAN"),
        "legacy continue_on_error: [1] should continue under --bail:\n{}",
        both(&o)
    );
    assert_ne!(
        code(&o),
        0,
        "block failure should still fail the run:\n{}",
        both(&o)
    );
}

// ===========================================================================
// timeout precedence + knob-source diagnostic
// ===========================================================================

/// `timeouts._default` caps a block with no per-block override; the diagnostic
/// names the frontmatter default as the knob to tune, and the JSON marks the
/// output partial.
#[test]
fn frontmatter_default_timeout_fires_with_knob_message() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "td.md",
        "---\nruntime: bash\ntimeouts:\n  _default: 1s\n---\n```bash\nsleep 30\necho NOPE\n```\n",
    );
    let start = Instant::now();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert!(
        start.elapsed().as_secs() < 20,
        "1s cap should cut the sleep short"
    );
    assert_ne!(
        code(&o),
        0,
        "a timed-out block fails the run:\n{}",
        both(&o)
    );
    assert!(
        err(&o).contains("limit set by frontmatter `timeouts._default`"),
        "diagnostic should name the frontmatter default knob:\n{}",
        err(&o)
    );
    let v = json_stdout(&o);
    assert_eq!(v["results"][0]["error_type"], "timeout", "got:\n{v}");
    assert_eq!(
        v["results"][0]["stdout_partial"],
        serde_json::Value::Bool(true),
        "got:\n{v}"
    );
    assert!(!out(&o).contains("NOPE"), "post-sleep echo must not run");
}

/// `--default-block-timeout` caps every block; the diagnostic names the CLI
/// flag as the source.
#[test]
fn cli_default_block_timeout_fires_with_knob_message() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "cli.md",
        "---\nruntime: bash\n---\n```bash\nsleep 30\n```\n",
    );
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--default-block-timeout",
            "1s",
        ])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "CLI default cap should fire:\n{}", both(&o));
    assert!(
        err(&o).contains("limit set by --default-block-timeout"),
        "diagnostic should name the CLI flag:\n{}",
        err(&o)
    );
}

/// Precedence: a fence `{timeout=1s}` wins over a looser `timeouts._default`,
/// and the diagnostic names the fence attr as the source.
#[test]
fn fence_timeout_beats_frontmatter_default() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "prec.md",
        // Fence cap (1s) is far below the runbook-wide default (30s); the block
        // sleeps 10s so there is no tie between the default and natural
        // completion — only the fence cap firing can finish this quickly.
        "---\nruntime: bash\ntimeouts:\n  _default: 30s\n---\n```bash {timeout=1s}\nsleep 10\n```\n",
    );
    let start = Instant::now();
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q"])
        .output()
        .unwrap();
    assert!(
        start.elapsed().as_secs() < 8,
        "the fence's 1s cap (not the 30s default) should fire:\n{}",
        both(&o)
    );
    assert_ne!(code(&o), 0, "timed-out block fails the run:\n{}", both(&o));
    assert!(
        err(&o).contains("limit set by fence attr"),
        "the fence attr should be reported as the limiting knob:\n{}",
        err(&o)
    );
}

// ===========================================================================
// callbacks during a real run (loopback receiver + HMAC signature)
// ===========================================================================

/// A passing run with `--callback ... --callback-secret k` POSTs a
/// `step.complete` per block then a terminal `run.complete`, each carrying an
/// `X-WB-Signature: sha256=` HMAC header.
#[test]
fn callback_emits_step_and_run_complete_with_signature() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "cb.md",
        "---\nruntime: bash\n---\n```bash\necho one\n```\n```bash\necho two\n```\n",
    );
    // 2 blocks → 2 step.complete + 1 run.complete.
    let (url, rx) = start_callback_sink(3);
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--callback",
            &url,
            "--callback-secret",
            "k",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "run should pass:\n{}", both(&o));

    let reqs = drain_requests(&rx, 3);
    let events: Vec<String> = reqs.iter().map(CapturedRequest::event).collect();
    assert_eq!(
        events,
        vec!["step.complete", "step.complete", "run.complete"],
        "callback event order"
    );
    for r in &reqs {
        assert!(
            r.headers.contains("X-WB-Signature: sha256="),
            "every signed callback should carry an HMAC header:\n{}",
            r.headers
        );
    }
    let run = reqs.last().unwrap();
    assert_eq!(run.body["status"], "pass");
    assert_eq!(run.body["blocks"]["total"], 2);
}

/// `--bail --checkpoint c1` on a workbook with a failing block fires a
/// `checkpoint.failed` callback (in addition to the per-block + run events).
#[test]
fn callback_checkpoint_failed_on_bail() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "fail.md",
        "---\nruntime: bash\n---\n```bash\necho ok\n```\n```bash\nfalse\n```\n",
    );
    // step.complete(ok) + step.complete(fail) + checkpoint.failed + run.complete = 4.
    let (url, rx) = start_callback_sink(4);
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--bail",
            "--checkpoint",
            "c1",
            "--callback",
            &url,
            "--callback-secret",
            "k",
        ])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "bail on failure should be non-zero:\n{}",
        both(&o)
    );

    let reqs = drain_requests(&rx, 4);
    let events: Vec<String> = reqs.iter().map(CapturedRequest::event).collect();
    assert!(
        events.iter().any(|e| e == "checkpoint.failed"),
        "a checkpoint.failed callback should fire under --bail:\n{events:?}"
    );
    let failed = reqs
        .iter()
        .find(|r| r.event() == "checkpoint.failed")
        .unwrap();
    assert_eq!(failed.body["checkpoint_id"], "c1");
    assert!(
        failed.headers.contains("X-WB-Signature: sha256="),
        "checkpoint.failed should be signed too:\n{}",
        failed.headers
    );
}

// ===========================================================================
// --events JSONL: conditional skip + timeout partial
// ===========================================================================

/// The `--events` JSONL stream records a `step.skipped` (kind `skip_if`) for a
/// conditionally skipped block, a `step.complete` carrying `stdout_partial` for
/// a timed-out block, and a terminal `run.complete`.
#[test]
fn events_jsonl_records_skip_and_timeout_partial() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "ev.md",
        "---\nruntime: bash\n---\n\
         ```bash {skip_if=$SKIP}\necho skipme\n```\n\
         ```bash {timeout=1s}\nsleep 30\n```\n",
    );
    let events = dir.path().join("run.jsonl");
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--events",
            events.to_str().unwrap(),
        ])
        .env("SKIP", "1")
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "the timeout should fail the run:\n{}",
        both(&o)
    );

    let text = std::fs::read_to_string(&events).expect("events file written");
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("event not JSON ({e}): {l}")))
        .collect();

    // Each line is an envelope `{event, run_id, data:{...}}`.
    let skip = records
        .iter()
        .find(|v| v["event"] == "step.skipped")
        .expect("a step.skipped event for the conditional skip");
    assert_eq!(
        skip["data"]["skip"]["kind"], "skip_if",
        "skip kind:\n{skip}"
    );

    let timeout = records
        .iter()
        .find(|v| v["data"]["block"]["error_type"] == "timeout")
        .expect("a step.complete event for the timed-out block");
    assert_eq!(timeout["event"], "step.complete");
    assert_eq!(
        timeout["data"]["block"]["stdout_partial"],
        serde_json::Value::Bool(true),
        "timed-out block marks partial output:\n{timeout}"
    );

    assert_eq!(
        records.last().map(|v| v["event"].clone()),
        Some(serde_json::Value::String("run.complete".to_string())),
        "stream should end with run.complete"
    );
}

// ===========================================================================
// structured outputs: export + equality gating + parse failure
// ===========================================================================

/// A block printing `output: foo=bar` exports `$WB_OUT_foo`, so a later
/// `{when=$WB_OUT_foo=bar}` runs while `{when=$WB_OUT_foo=nope}` is skipped.
/// Drives `capture_outputs_for_result` + the `WB_OUT_` export into the
/// conditional evaluator.
#[test]
fn output_export_gates_equality_conditionals() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "gate.md",
        "---\nruntime: bash\n---\n\
         ```bash\necho \"output: foo=bar\"\n```\n\
         ```bash {when=$WB_OUT_foo=bar}\necho MATCH_RAN\n```\n\
         ```bash {when=$WB_OUT_foo=nope}\necho NOMATCH_RAN\n```\n",
    );
    // Run non-quiet: the matching block streams its echo + output, while the
    // skipped block emits nothing at all (no echo, no output).
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "run should pass:\n{}", both(&o));
    assert!(
        out(&o).contains("MATCH_RAN"),
        "matching equality gate should run:\n{}",
        out(&o)
    );
    assert!(
        !out(&o).contains("NOMATCH_RAN"),
        "non-matching equality gate should be skipped:\n{}",
        out(&o)
    );
}

/// A malformed `output:` line (no `=`) sends the block down the
/// `capture_outputs_for_result` error branch: it's marked
/// `error_type=output_parse_failed` and the run fails.
#[test]
fn malformed_output_marks_parse_failure() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "bad.md",
        "---\nruntime: bash\n---\n```bash\necho \"output: this has no equals sign\"\n```\n",
    );
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--json"])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "a parse failure should fail the run:\n{}",
        both(&o)
    );
    let v = json_stdout(&o);
    assert_eq!(
        v["results"][0]["error_type"], "output_parse_failed",
        "got:\n{v}"
    );
}

// ===========================================================================
// artifact manifest internals
// ===========================================================================

/// A bash block writes a file into `$WB_ARTIFACTS_DIR` plus a `<name>.meta.json`
/// sidecar; with a stable `WB_RECORDING_RUN_ID` + HOME the run records a
/// manifest, and `wb artifacts list --run r1 --format json` surfaces the label,
/// description, sha256 checksum, and producing step id. Covers the manifest
/// record/load path (`manifest_for`) and sidecar metadata.
#[test]
fn artifact_manifest_records_metadata_and_step_provenance() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "art.md",
        "---\nruntime: bash\n---\n```bash {#make-report}\n\
         printf 'id,name\\n1,Ada\\n' > \"$WB_ARTIFACTS_DIR/report.csv\"\n\
         printf '{\"label\":\"Sales report\",\"description\":\"Quarterly CSV\"}' > \"$WB_ARTIFACTS_DIR/report.csv.meta.json\"\n```\n",
    );
    let run = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q"])
        .env("WB_RECORDING_RUN_ID", "r1")
        .output()
        .unwrap();
    assert_eq!(
        code(&run),
        0,
        "artifact-writing run should pass:\n{}",
        both(&run)
    );

    let listed = wb(h.path())
        .args(["artifacts", "list", "--run", "r1", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        code(&listed),
        0,
        "artifacts list should succeed:\n{}",
        both(&listed)
    );
    let v = json_stdout(&listed);
    assert_eq!(v["run_id"], "r1");
    let entry = &v["artifacts"][0];
    assert_eq!(entry["filename"], "report.csv");
    assert_eq!(entry["content_type"], "text/csv");
    assert_eq!(entry["label"], "Sales report");
    assert_eq!(entry["description"], "Quarterly CSV");
    assert_eq!(
        entry["step_id"], "make-report",
        "step provenance should be recorded:\n{entry}"
    );
    let sha = entry["sha256"].as_str().unwrap_or_default();
    assert_eq!(sha.len(), 64, "sha256 should be 64 hex chars:\n{entry}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "sha256 should be hex:\n{entry}"
    );
}

// ===========================================================================
// --allow-runtime DENIAL (and permit)
// ===========================================================================

/// A workbook using a language not on the `--allow-runtime` allowlist is refused
/// before any block runs (exit 2), with a message naming the offending runtime.
/// `ruby` need not be installed: the gate works off the parsed language tag.
#[test]
fn allow_runtime_denies_unlisted_language() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "ruby.md",
        "---\nruntime: bash\n---\n```bash\necho bash-ok\n```\n```ruby\nputs 1\n```\n",
    );
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--allow-runtime",
            "bash",
        ])
        .output()
        .unwrap();
    assert_eq!(
        code(&o),
        2,
        "a disallowed runtime is a usage error:\n{}",
        both(&o)
    );
    assert!(
        err(&o).contains("not in the --allow-runtime allowlist"),
        "should explain the allowlist denial:\n{}",
        err(&o)
    );
    assert!(
        !out(&o).contains("bash-ok"),
        "the gate must refuse before any block runs:\n{}",
        out(&o)
    );
}

/// A repeated `--allow-runtime` allowlist that covers every language used lets
/// the run proceed (distinct from cov_run_flags' single-language permit).
#[test]
fn allow_runtime_permits_listed_languages() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "ok.md",
        "---\nruntime: bash\n---\n```bash\necho BASH_OK\n```\n```python\nprint(\"PY_OK\")\n```\n",
    );
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--allow-runtime",
            "bash",
            "--allow-runtime",
            "python",
        ])
        .output()
        .unwrap();
    assert_eq!(
        code(&o),
        0,
        "an allowlist covering all languages should run:\n{}",
        both(&o)
    );
    assert!(
        both(&o).contains("BASH_OK") && both(&o).contains("PY_OK"),
        "both blocks run:\n{}",
        both(&o)
    );
}

// ===========================================================================
// redaction
// ===========================================================================

/// `--redact NAME` masks the value in the machine-readable `--json` document.
#[test]
fn redact_flag_masks_value_in_json() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "r.md",
        "---\nruntime: bash\nenv:\n  APIKEY: zzsecretkeyzz\n---\n```bash\necho \"key=$APIKEY\"\n```\n",
    );
    let o = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--redact",
            "APIKEY",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "run should pass:\n{}", both(&o));
    assert!(
        !out(&o).contains("zzsecretkeyzz"),
        "redacted value must not appear in --json output:\n{}",
        out(&o)
    );
}

/// A `secret: true` param has its value masked in rendered (`-o`) output.
#[test]
fn secret_param_redacted_in_rendered_output() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "sec.md",
        "---\nruntime: bash\nparams:\n  token:\n    type: string\n    secret: true\n    default: TOPSECRET123\n---\n```bash\necho \"tok=$token\"\n```\n",
    );
    let out_path = dir.path().join("out.md");
    let o = wb(h.path())
        .args(["run", wbf.to_str().unwrap(), "-q", "--md", "-o"])
        .arg(&out_path)
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "run should pass:\n{}", both(&o));
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        !written.contains("TOPSECRET123"),
        "secret param value must be redacted:\n{written}"
    );
    assert!(
        written.contains("tok="),
        "the surrounding context should remain:\n{written}"
    );
}

// ===========================================================================
// include splice numbering + frame finish events
// ===========================================================================

/// An `include:` splices the child's blocks into the parent's list: progress
/// numbers them as one stream ([1/3]…[3/3]) and the events carry an
/// include-frame `step.started` + `step.finished(outcome=ok)`.
#[test]
fn include_splices_blocks_and_emits_frame_finish() {
    let h = home();
    let dir = home();
    write_wb(
        dir.path(),
        "child.md",
        "---\nruntime: bash\n---\n```bash\necho CHILD_A\n```\n```bash\necho CHILD_B\n```\n",
    );
    let parent = write_wb(
        dir.path(),
        "parent.md",
        "---\nruntime: bash\n---\n```include\npath: ./child.md\n```\n```bash\necho PARENT_ONLY\n```\n",
    );
    let events = dir.path().join("e.jsonl");
    let o = wb(h.path())
        .args([
            "run",
            parent.to_str().unwrap(),
            "--events",
            events.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "include run should pass:\n{}", both(&o));
    let s = both(&o);
    // 1 child block + 1 child block + 1 parent block = 3 total in one stream.
    assert!(
        s.contains("[1/3]") && s.contains("[2/3]") && s.contains("[3/3]"),
        "spliced numbering:\n{s}"
    );

    let text = std::fs::read_to_string(&events).unwrap();
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let started = records
        .iter()
        .find(|v| v["event"] == "step.started")
        .expect("include step.started");
    assert_eq!(
        started["data"]["step_kind"], "include",
        "frame should be an include:\n{started}"
    );
    let finished = records
        .iter()
        .find(|v| v["event"] == "step.finished")
        .expect("include step.finished");
    assert_eq!(
        finished["data"]["outcome"], "ok",
        "frame should close ok:\n{finished}"
    );
    // The two child blocks ran under a non-empty include chain; the parent block didn't.
    let child_chained = records.iter().any(|v| {
        v["event"] == "step.complete"
            && v["data"]["include_chain"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
    });
    assert!(child_chained, "child blocks should carry an include_chain");
}

/// When a bailed failure happens inside an include, `emit_finish_for_stack`
/// closes the frame with `step.finished(outcome=failed)` so the timeline shows
/// the parent include failing too.
#[test]
fn include_failure_finishes_frame_with_failed_outcome() {
    let h = home();
    let dir = home();
    write_wb(
        dir.path(),
        "cfail.md",
        "---\nruntime: bash\n---\n```bash\nfalse\n```\n",
    );
    let parent = write_wb(
        dir.path(),
        "pfail.md",
        "---\nruntime: bash\n---\n```include\npath: ./cfail.md\n```\n```bash\necho PARENT_ONLY\n```\n",
    );
    let events = dir.path().join("e.jsonl");
    let o = wb(h.path())
        .args([
            "run",
            parent.to_str().unwrap(),
            "--bail",
            "-q",
            "--events",
            events.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_ne!(
        code(&o),
        0,
        "bailed include failure should fail the run:\n{}",
        both(&o)
    );
    assert!(
        !out(&o).contains("PARENT_ONLY"),
        "parent block must not run after bail:\n{}",
        out(&o)
    );

    let text = std::fs::read_to_string(&events).unwrap();
    let finished = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["event"] == "step.finished")
        .expect("include frame should emit step.finished on bail");
    assert_eq!(
        finished["data"]["outcome"], "failed",
        "frame closes failed:\n{finished}"
    );
    assert_eq!(finished["data"]["step_kind"], "include");
}

// ===========================================================================
// prepare_checkpoint fresh-restart edges (complementing cov_lifecycle)
// ===========================================================================

/// Re-running a non-complete checkpoint with a different `--param` value starts
/// fresh (the resolved env differs, so resuming would mix parameter sets).
#[test]
fn param_change_restarts_checkpoint_fresh() {
    let h = home();
    let dir = home();
    let wbf = write_wb(
        dir.path(),
        "p.md",
        "---\nruntime: bash\nparams:\n  region:\n    type: string\n    default: us\n---\n\
         ```bash\necho \"region=$region\"\n```\n```bash\nfalse\n```\n",
    );
    // First run fails at block 2 under --bail → an in-progress checkpoint.
    let r1 = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-q",
            "--bail",
            "--checkpoint",
            "pc1",
            "--param",
            "region=us",
        ])
        .output()
        .unwrap();
    assert_ne!(code(&r1), 0, "first run should fail:\n{}", both(&r1));

    // Re-run with a different param → start-fresh notice instead of resume.
    let r2 = wb(h.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "pc1",
            "--param",
            "region=eu",
        ])
        .output()
        .unwrap();
    assert!(
        err(&r2).contains("parameters changed"),
        "a param change should restart the checkpoint fresh:\n{}",
        err(&r2)
    );
}

/// Reusing a checkpoint id against a *different workbook path* starts fresh
/// (the saved `workbook` no longer matches), re-running block 1 live.
#[test]
fn workbook_path_change_restarts_checkpoint_fresh() {
    let h = home();
    let dir = home();
    let a = write_wb(
        dir.path(),
        "a.md",
        "---\nruntime: bash\n---\n```bash\necho A1\n```\n```bash\nfalse\n```\n",
    );
    let b = write_wb(
        dir.path(),
        "b.md",
        "---\nruntime: bash\n---\n```bash\necho B1_LIVE\n```\n```bash\necho B2\n```\n",
    );
    let r1 = wb(h.path())
        .args([
            "run",
            a.to_str().unwrap(),
            "-q",
            "--bail",
            "--checkpoint",
            "shared",
        ])
        .output()
        .unwrap();
    assert_ne!(code(&r1), 0, "first run (a.md) should fail:\n{}", both(&r1));

    let r2 = wb(h.path())
        .args([
            "run",
            b.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "shared",
        ])
        .output()
        .unwrap();
    assert_eq!(
        code(&r2),
        0,
        "second run (b.md) should pass fresh:\n{}",
        both(&r2)
    );
    assert!(
        out(&r2).contains("B1_LIVE") && out(&r2).contains("B2"),
        "a different workbook path should start fresh, running its block 1 live:\n{}",
        out(&r2)
    );
}

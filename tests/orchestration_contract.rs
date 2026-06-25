use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

#[derive(Debug)]
struct CapturedRequest {
    headers: String,
    body: serde_json::Value,
}

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

fn recv_requests(rx: &mpsc::Receiver<CapturedRequest>, n: usize) -> Vec<CapturedRequest> {
    (0..n)
        .map(|_| {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("expected callback request")
        })
        .collect()
}

#[test]
fn callbacks_carry_workflow_outputs_and_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let workbook = dir.path().join("contract.md");
    let artifacts = dir.path().join("artifacts");
    std::fs::write(
        &workbook,
        r#"---
workflow:
  slug: agent/contract
  version: test
  nodes:
    export:
      primitive: file/export
      title: Export report
---
```bash {#export}
mkdir -p "$WB_ARTIFACTS_DIR"
printf 'id,name\n1,Ada\n' > "$WB_ARTIFACTS_DIR/report.csv"
printf '{"label":"Contract report","description":"CSV export"}' > "$WB_ARTIFACTS_DIR/report.csv.meta.json"
echo "output: report_path=$WB_ARTIFACTS_DIR/report.csv"
```
"#,
    )
    .unwrap();

    let (url, rx) = start_callback_sink(3);
    let output = Command::new(wb_binary())
        .args(["run", workbook.to_str().unwrap(), "--callback", &url])
        .env("WB_RECORDING_RUN_ID", "run-contract")
        .env("WB_ARTIFACTS_DIR", &artifacts)
        .output()
        .expect("spawn wb");
    assert!(
        output.status.success(),
        "wb run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = recv_requests(&rx, 3);
    let events: Vec<_> = requests
        .iter()
        .map(|r| r.body["event"].as_str().unwrap())
        .collect();
    assert_eq!(
        events,
        vec!["step.artifact_saved", "step.complete", "run.complete"]
    );
    assert!(
        requests[0].headers.contains("X-WB-Sequence: 1"),
        "first callback should carry sequence 1:\n{}",
        requests[0].headers
    );
    assert!(
        requests[1].headers.contains("X-WB-Idempotency-Key: "),
        "callbacks should carry idempotency keys:\n{}",
        requests[1].headers
    );

    let artifact = &requests[0].body;
    assert_eq!(artifact["event_version"], "1");
    assert_eq!(artifact["run_id"], "run-contract");
    assert_eq!(artifact["block"]["step_id"], "export");
    assert_eq!(artifact["workflow"]["slug"], "agent/contract");
    assert_eq!(artifact["workflow_node"]["id"], "export");
    assert_eq!(artifact["workflow_node"]["primitive"], "file/export");
    assert_eq!(artifact["artifact"]["filename"], "report.csv");
    assert_eq!(artifact["artifact"]["content_type"], "text/csv");
    assert_eq!(artifact["artifact"]["label"], "Contract report");
    assert_eq!(artifact["artifact"]["description"], "CSV export");

    let complete = &requests[1].body;
    assert_eq!(complete["event_version"], "1");
    assert_eq!(complete["block"]["step_id"], "export");
    assert_eq!(complete["outputs"]["report_path"]["type"], "string");
    assert!(complete["outputs"]["report_path"]["value"]
        .as_str()
        .unwrap()
        .ends_with("report.csv"));

    let run = &requests[2].body;
    assert_eq!(run["event_version"], "1");
    assert_eq!(run["status"], "pass");
    assert_eq!(run["blocks"]["total"], 1);
}

#[test]
fn wait_pause_resume_persists_agent_contract_state() {
    let dir = tempfile::tempdir().unwrap();
    let checkpoint_dir = dir.path().join("checkpoints");
    let workbook = dir.path().join("wait-contract.md");
    std::fs::write(
        &workbook,
        r#"---
workflow:
  slug: wait/contract
  nodes:
    approval:
      primitive: wait/manual-approval
      title: Approval
---
```wait {#approval}
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

    let (url, rx) = start_callback_sink(1);
    let checkpoint_id = "wait-contract";
    let paused = Command::new(wb_binary())
        .args([
            "run",
            workbook.to_str().unwrap(),
            "--checkpoint",
            checkpoint_id,
            "--callback",
            &url,
        ])
        .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
        .env("WB_RECORDING_RUN_ID", "run-wait-contract")
        .output()
        .expect("spawn wb run");
    assert_eq!(
        paused.status.code(),
        Some(42),
        "run should pause\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&paused.stdout),
        String::from_utf8_lossy(&paused.stderr)
    );

    let requests = recv_requests(&rx, 1);
    let paused_event = &requests[0].body;
    assert_eq!(paused_event["event"], "workbook.paused");
    assert_eq!(paused_event["event_version"], "1");
    assert_eq!(paused_event["run_id"], "run-wait-contract");
    assert_eq!(paused_event["checkpoint_id"], checkpoint_id);
    assert_eq!(paused_event["wait"]["kind"], "manual");
    assert_eq!(
        paused_event["wait"]["bind"],
        serde_json::json!(["approved"])
    );
    assert!(paused_event["wait"]["timeout_at"].is_string());
    assert_eq!(paused_event["workflow_node"]["id"], "approval");

    let checkpoint_path = checkpoint_dir.join(format!("{checkpoint_id}.json"));
    let checkpoint: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&checkpoint_path).unwrap()).unwrap();
    assert_eq!(checkpoint["status"], "paused");
    assert_eq!(checkpoint["next_block"], 0);
    assert_eq!(checkpoint["next_step_id"], "after");
    assert_eq!(checkpoint["workflow"]["slug"], "wait/contract");

    let pending_path = checkpoint_dir.join(format!("{checkpoint_id}.pending.json"));
    let pending: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&pending_path).unwrap()).unwrap();
    assert_eq!(pending["next_step_id"], "after");
    assert_eq!(pending["callback_url"], url);

    let resumed = Command::new(wb_binary())
        .args(["resume", checkpoint_id, "--value", "yes"])
        .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
        .output()
        .expect("spawn wb resume");
    assert!(
        resumed.status.success(),
        "resume failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&resumed.stdout),
        String::from_utf8_lossy(&resumed.stderr)
    );
    let stdout = String::from_utf8_lossy(&resumed.stdout);
    assert!(
        stdout.contains("approved=yes"),
        "resume should bind wait value into the next step:\n{stdout}"
    );

    let final_checkpoint: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&checkpoint_path).unwrap()).unwrap();
    assert_eq!(final_checkpoint["status"], "complete");
    assert!(
        !pending_path.exists(),
        "pending descriptor should be removed after resume"
    );
}

#[test]
#[cfg(unix)]
fn browser_pause_emits_single_step_paused_and_workbook_paused() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let checkpoint_dir = dir.path().join("checkpoints");
    let sidecar = dir.path().join("fake-sidecar.sh");
    std::fs::write(
        &sidecar,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"type":"hello"'*)
      printf '%s\n' '{"type":"ready"}'
      ;;
    *'"type":"slice"'*)
      printf '%s\n' '{"type":"slice.paused","reason":"pause_for_human","resume_url":"https://live.example/session","verb_index":0,"message":"Approve this task","context_url":"https://example.com/task","resume_on":"timeout","timeout":"5m","actions":[{"kind":"goto_step","target":"after","label":"Skip ahead"}],"sidecar_state":{"session":"abc"}}'
      ;;
    *'"type":"suspend"'*|*'"type":"shutdown"'*)
      exit 0
      ;;
  esac
done
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&sidecar).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&sidecar, perms).unwrap();

    let workbook = dir.path().join("browser-pause.md");
    std::fs::write(
        &workbook,
        r#"---
workflow:
  slug: browser/contract
  nodes:
    pause:
      primitive: browser/pause
      title: Browser pause
---
```browser {#pause}
session: contract
verbs:
  - pause_for_human:
      message: Approve this task
      resume_on: timeout
      timeout: 5m
      actions:
        - kind: goto_step
          target: after
          label: Skip ahead
```

```bash {#after}
echo after
```
"#,
    )
    .unwrap();

    let (url, rx) = start_callback_sink(2);
    let checkpoint_id = "browser-contract";
    let paused = Command::new(wb_binary())
        .args([
            "run",
            workbook.to_str().unwrap(),
            "--checkpoint",
            checkpoint_id,
            "--callback",
            &url,
        ])
        .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
        .env("WB_BROWSER_RUNTIME", &sidecar)
        .env("WB_RECORDING_RUN_ID", "run-browser-contract")
        .output()
        .expect("spawn wb run");
    assert_eq!(
        paused.status.code(),
        Some(42),
        "browser run should pause\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&paused.stdout),
        String::from_utf8_lossy(&paused.stderr)
    );

    let requests = recv_requests(&rx, 2);
    let events: Vec<_> = requests
        .iter()
        .map(|r| r.body["event"].as_str().unwrap())
        .collect();
    assert_eq!(events, vec!["step.paused", "workbook.paused"]);

    let step_paused = &requests[0].body;
    assert_eq!(step_paused["run_id"], "run-browser-contract");
    assert_eq!(step_paused["checkpoint_id"], checkpoint_id);
    assert_eq!(step_paused["block"]["step_id"], "pause");
    assert_eq!(step_paused["progress"]["completed"], 1);
    assert_eq!(step_paused["progress"]["total"], 2);
    assert_eq!(step_paused["reason"], "pause_for_human");
    assert_eq!(step_paused["resume_url"], "https://live.example/session");
    assert_eq!(step_paused["actions"][0]["target"], "after");
    assert_eq!(step_paused["workflow_node"]["id"], "pause");

    let workbook_paused = &requests[1].body;
    assert_eq!(workbook_paused["event"], "workbook.paused");
    assert_eq!(workbook_paused["wait"]["kind"], "pause_for_human");
    assert!(workbook_paused["wait"]["timeout_at"].is_string());
    assert_eq!(workbook_paused["workflow_node"]["id"], "pause");

    let pending_path = checkpoint_dir.join(format!("{checkpoint_id}.pending.json"));
    let pending: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&pending_path).unwrap()).unwrap();
    assert_eq!(pending["next_step_id"], "pause");
    assert_eq!(pending["resume_on"], "timeout");
    assert_eq!(pending["on_timeout"], "abort");
    assert_eq!(pending["actions"][0]["target"], "after");
}

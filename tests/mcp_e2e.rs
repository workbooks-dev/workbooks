//! End-to-end test for `wb mcp`: drive the JSON-RPC stdio server through the
//! full author → run → pause → resume → read-results lifecycle, exactly as an
//! MCP client would. This is the verification called for by TODO #39.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// A live `wb mcp` server plus its stdin/stdout, with a private checkpoint dir.
struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    fn start(checkpoint_dir: &std::path::Path) -> Self {
        let mut child = Command::new(wb_binary())
            .arg("mcp")
            .env("WB_CHECKPOINT_DIR", checkpoint_dir)
            // Keep child-of-child runs quiet on stderr so test output is clean.
            .env("WB_LOG_LEVEL", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn wb mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        McpClient {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    fn notify(&mut self, method: &str) {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        writeln!(self.stdin, "{}", serde_json::to_string(&msg).unwrap()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Send a request and read exactly one response line back.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{}", serde_json::to_string(&msg).unwrap()).unwrap();
        self.stdin.flush().unwrap();

        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        let resp: Value = serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("bad json response: {e}\nline: {line}"));
        assert_eq!(resp["id"], id, "response id mismatch: {resp}");
        resp
    }

    /// tools/call returning the parsed `structuredContent` (the canonical
    /// machine-readable channel). Asserts the call did not error.
    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let result = &resp["result"];
        assert_eq!(
            result["isError"], false,
            "tool {name} returned error: {result}"
        );
        result["structuredContent"].clone()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Closing stdin makes the server loop hit EOF and exit cleanly.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_author_run_pause_resume_read_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    let ckpt_dir = tmp.path().join("checkpoints");
    std::fs::create_dir_all(&ckpt_dir).unwrap();
    let workbook_path = tmp.path().join("flow.md");
    let run_id = "mcp-e2e-run";

    let mut mcp = McpClient::start(&ckpt_dir);

    // 1. initialize handshake.
    let init = mcp.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "wb");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    mcp.notify("notifications/initialized");

    // 2. tools/list exposes the contract.
    let tools = mcp.request("tools/list", json!({}));
    let names: Vec<String> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for want in [
        "run_workbook",
        "resume_workbook",
        "list_pending",
        "get_run_events",
        "inspect_workbook",
    ] {
        assert!(names.contains(&want.to_string()), "missing tool {want}");
    }

    // 3. author_workbook — write a workbook that pauses on a `wait`.
    let content = r#"# MCP Flow

```bash
echo "before-pause"
```

```wait
kind: manual
bind: token
timeout: 5m
```

```bash
echo "resumed-with: $token"
```
"#;
    let authored = mcp.call_tool(
        "author_workbook",
        json!({ "path": workbook_path.to_str().unwrap(), "content": content }),
    );
    assert_eq!(authored["path"], workbook_path.to_str().unwrap());

    // 4. inspect_workbook — structure visible without running.
    let inspected = mcp.call_tool(
        "inspect_workbook",
        json!({ "file": workbook_path.to_str().unwrap() }),
    );
    assert!(
        inspected.is_object() || inspected.is_array(),
        "inspect json: {inspected}"
    );

    // 5. run_workbook — should pause, surfacing an elicitation for `token`.
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": workbook_path.to_str().unwrap(), "run_id": run_id }),
    );
    assert_eq!(run["status"], "input_required", "run result: {run}");
    assert_eq!(run["run_id"], run_id);
    assert_eq!(run["elicitation"]["bind"][0], "token");
    assert_eq!(
        run["elicitation"]["requestedSchema"]["properties"]["token"]["type"],
        "string"
    );

    // 6. list_pending — the run shows up as awaiting input.
    let pending = mcp.call_tool("list_pending", json!({}));
    let pending_str = pending.to_string();
    assert!(
        pending_str.contains(run_id),
        "pending list missing run: {pending_str}"
    );

    // 7. resume_workbook with the awaited value — run completes.
    let resumed = mcp.call_tool(
        "resume_workbook",
        json!({ "run_id": run_id, "value": "s3cr3t" }),
    );
    assert_eq!(resumed["status"], "completed", "resume result: {resumed}");

    // 8. get_run_events — replay the durable timeline and confirm the bound
    //    value flowed into the post-pause block.
    let events = mcp.call_tool("get_run_events", json!({ "run_id": run_id }));
    assert_eq!(events["status"], "complete", "events: {events}");
    assert_eq!(events["terminal"]["event"], "run.complete");
    let evs = events["events"].as_array().unwrap();
    assert!(!evs.is_empty(), "expected step events");
    let all_stdout: String = evs
        .iter()
        .filter_map(|e| e["stdout"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_stdout.contains("before-pause"),
        "missing pre-pause output: {all_stdout}"
    );
    assert!(
        all_stdout.contains("resumed-with: s3cr3t"),
        "bound value did not flow into resumed block: {all_stdout}"
    );
}

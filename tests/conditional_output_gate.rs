use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

// Monotonic per-test counter so concurrent tests (same pid, possibly same
// coarse-resolution timestamp) never share a temp dir — otherwise one test's
// cleanup races the other's running `wb` and yields empty output.
static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// A step's `output: name=value` is exported as `$WB_OUT_name` into the eval
/// env, so a later cell's `{when=...}` / `{skip_if=...}` can branch on it.
fn run_workbook(body: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "wb-cond-out-{}-{}",
        std::process::id(),
        DIR_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let wb = dir.join("gate.md");
    std::fs::write(&wb, body).unwrap();
    let output = Command::new(wb_binary())
        .arg("run")
        .arg(&wb)
        .output()
        .expect("failed to spawn wb");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn output_gates_when_truthy() {
    // needs_login=1 → the {when=$WB_OUT_needs_login} guard fires, the
    // {skip_if=...} step is skipped.
    let out = run_workbook(
        "---\nruntime: bash\n---\n\n\
         ```bash\necho \"output: needs_login=1\"\n```\n\n\
         ```bash {when=$WB_OUT_needs_login}\necho GUARD_RAN\n```\n\n\
         ```bash {skip_if=$WB_OUT_needs_login}\necho WARM_RAN\n```\n",
    );
    assert!(
        out.contains("GUARD_RAN"),
        "when= step should run; got:\n{out}"
    );
    assert!(
        !out.contains("WARM_RAN"),
        "skip_if= step should be skipped; got:\n{out}"
    );
}

#[test]
fn output_gates_when_falsy() {
    // needs_login=0 → the {when=...} guard is skipped, the {skip_if=...} runs.
    let out = run_workbook(
        "---\nruntime: bash\n---\n\n\
         ```bash\necho \"output: needs_login=0\"\n```\n\n\
         ```bash {when=$WB_OUT_needs_login}\necho GUARD_RAN\n```\n\n\
         ```bash {skip_if=$WB_OUT_needs_login}\necho WARM_RAN\n```\n",
    );
    assert!(
        !out.contains("GUARD_RAN"),
        "when= step should be skipped; got:\n{out}"
    );
    assert!(
        out.contains("WARM_RAN"),
        "skip_if= step should run; got:\n{out}"
    );
}

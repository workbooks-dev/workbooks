//! Integration coverage for src/lib.rs parameter handling and the
//! trust / signing / lock subcommands.
//!
//! Every test spawns the real `wb` binary (so llvm-cov instruments it) and
//! isolates ~/.wb by pointing $HOME at a fresh tempdir. Workbooks and keys are
//! written into tempdirs. No network / Docker / doppler / Redis.

use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// A `wb` command whose $HOME is isolated to `home`.
fn wb(home: &std::path::Path) -> Command {
    let mut c = Command::new(wb_binary());
    c.env("HOME", home);
    // Keep stderr clean and deterministic; don't let an ambient config leak in.
    c.env("WB_LOG_LEVEL", "error");
    c.env_remove("WB_CONFIG_PATH");
    c.env_remove("WB_REQUIRE_TRUST");
    c
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

/// Write a file and return its path inside `dir`.
fn write(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, body).unwrap();
    p
}

const PARAMS_WB: &str = r#"---
runtime: bash
params:
  region:
    type: enum
    one_of: [us-east-1, eu-west-1, ap-south-1]
    default: us-east-1
  replicas:
    type: int
    default: 2
  dry_run:
    type: bool
    default: true
  service: api
profiles:
  prod:
    region: eu-west-1
    replicas: 6
    dry_run: false
  staging:
    region: ap-south-1
    replicas: 2
---

```bash
echo "deploying $service x$replicas to $region (dry_run=$dry_run)"
```
"#;

// ---------------------------------------------------------------------------
// PARAMS — defaults / override / profile / param-file / precedence
// ---------------------------------------------------------------------------

#[test]
fn params_defaults_render() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(
        out(&o).contains("deploying api x2 to us-east-1 (dry_run=true)"),
        "got: {}",
        out(&o)
    );
}

#[test]
fn params_example_file_runs() {
    // Exercise the shipped examples/params-demo.md through the real binary.
    let home = tempdir().unwrap();
    let manifest = env!("CARGO_MANIFEST_DIR");
    let example = PathBuf::from(manifest).join("examples/params-demo.md");
    let o = wb(home.path())
        .args(["run", example.to_str().unwrap(), "--profile", "prod"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("eu-west-1"), "got: {}", out(&o));
}

#[test]
fn params_single_override() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--param", "replicas=10"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("api x10 to us-east-1"), "got: {}", out(&o));
}

#[test]
fn params_profile_applies_preset() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--profile", "prod"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(
        out(&o).contains("deploying api x6 to eu-west-1 (dry_run=false)"),
        "got: {}",
        out(&o)
    );
}

#[test]
fn params_param_beats_profile() {
    // --param has higher precedence than --profile.
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--profile",
            "prod",
            "--param",
            "region=ap-south-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    // region from --param wins, replicas still from profile.
    assert!(out(&o).contains("api x6 to ap-south-1"), "got: {}", out(&o));
}

#[test]
fn params_param_file_mapping() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let vals = write(&dir, "values.yaml", "region: ap-south-1\nreplicas: 4\n");
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--param-file",
            vals.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("api x4 to ap-south-1"), "got: {}", out(&o));
}

#[test]
fn params_param_beats_param_file() {
    // --param > --param-file.
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let vals = write(&dir, "values.yaml", "region: ap-south-1\n");
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--param-file",
            vals.to_str().unwrap(),
            "--param",
            "region=eu-west-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("to eu-west-1"), "got: {}", out(&o));
}

// ---------------------------------------------------------------------------
// PARAMS — validation errors (all exit 2)
// ---------------------------------------------------------------------------

#[test]
fn params_bad_enum_value() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--param", "region=mars"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 2, "stdout: {} stderr: {}", out(&o), err(&o));
    assert!(err(&o).contains("not one of"), "stderr: {}", err(&o));
}

#[test]
fn params_bad_int_value() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--param", "replicas=notanint"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 2, "stderr: {}", err(&o));
    assert!(err(&o).contains("not a valid int"), "stderr: {}", err(&o));
}

#[test]
fn params_undeclared_param_key() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--param", "nope=1"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 2, "stderr: {}", err(&o));
    assert!(
        err(&o).contains("not a declared parameter"),
        "stderr: {}",
        err(&o)
    );
}

#[test]
fn params_undeclared_param_file_key() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "p.md", PARAMS_WB);
    let vals = write(&dir, "bad.yaml", "bogus: 1\n");
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--param-file",
            vals.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 2, "stderr: {}", err(&o));
    assert!(
        err(&o).contains("not a declared parameter"),
        "stderr: {}",
        err(&o)
    );
}

#[test]
fn params_missing_required() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(
        &dir,
        "req.md",
        "---\nruntime: bash\nparams:\n  token:\n    type: string\n    required: true\n---\n\n```bash\necho \"tok=$token\"\n```\n",
    );
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 2, "stderr: {}", err(&o));
    assert!(
        err(&o).contains("missing required parameter"),
        "stderr: {}",
        err(&o)
    );
}

#[test]
fn params_required_provided_runs() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(
        &dir,
        "req.md",
        "---\nruntime: bash\nparams:\n  token:\n    type: string\n    required: true\n---\n\n```bash\necho \"tok=$token\"\n```\n",
    );
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--param", "token=abc123"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("tok=abc123"), "got: {}", out(&o));
}

// ---------------------------------------------------------------------------
// PARAMS — secret redaction
// ---------------------------------------------------------------------------

#[test]
fn params_secret_value_redacted_in_output() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(
        &dir,
        "sec.md",
        "---\nruntime: bash\nparams:\n  apikey:\n    type: string\n    secret: true\n    default: SUPERSECRET123\n---\n\n```bash\necho \"key is $apikey\"\n```\n",
    );
    let outpath = dir.path().join("out.md");
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "-o",
            outpath.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    let rendered = std::fs::read_to_string(&outpath).unwrap();
    assert!(
        !rendered.contains("SUPERSECRET123"),
        "raw secret leaked into rendered output:\n{}",
        rendered
    );
    assert!(
        rendered.contains("***"),
        "expected redaction marker in output:\n{}",
        rendered
    );
}

// ---------------------------------------------------------------------------
// PARAMS — checkpoint param identity
// ---------------------------------------------------------------------------

const CP_WB: &str = r#"---
runtime: bash
params:
  region:
    type: string
    default: us-east-1
---

```bash
echo "region=$region"
```

```bash
exit 1
```
"#;

#[test]
fn checkpoint_resume_same_params_skips_completed_block() {
    // Block 1 succeeds, block 2 fails under --bail. On resume with the SAME
    // params, the completed block 1 is skipped (not re-echoed).
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "cp.md", CP_WB);

    let run1 = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "cp",
            "--param",
            "region=eu-west-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&run1), 1, "stderr: {}", err(&run1));
    assert!(
        out(&run1).contains("region=eu-west-1"),
        "got: {}",
        out(&run1)
    );

    let resume = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "cp",
            "--param",
            "region=eu-west-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&resume), 1, "stderr: {}", err(&resume));
    // Block 1 was already completed under the same param identity → skipped.
    assert!(
        !out(&resume).contains("region=eu-west-1"),
        "completed block should have been skipped on resume, got: {}",
        out(&resume)
    );
}

#[test]
fn checkpoint_different_params_start_fresh() {
    // Re-running the same checkpoint id with DIFFERENT params must start fresh:
    // block 1 re-runs (re-echoes), and the persisted param_hash changes.
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "cp.md", CP_WB);

    let run1 = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "cp",
            "--param",
            "region=eu-west-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&run1), 1, "stderr: {}", err(&run1));
    let cp_path = home.path().join(".wb/checkpoints/cp.json");
    let hash1 = std::fs::read_to_string(&cp_path).unwrap();
    let hash1 = extract_param_hash(&hash1);

    let run2 = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "cp",
            "--param",
            "region=ap-south-1",
        ])
        .output()
        .unwrap();
    assert_eq!(code(&run2), 1, "stderr: {}", err(&run2));
    // Fresh run → block 1 re-executes with the new value.
    assert!(
        out(&run2).contains("region=ap-south-1"),
        "fresh run should re-execute block 1, got: {}",
        out(&run2)
    );
    let hash2 = extract_param_hash(&std::fs::read_to_string(&cp_path).unwrap());
    assert_ne!(
        hash1, hash2,
        "param_hash should change with different params"
    );
}

fn extract_param_hash(json: &str) -> String {
    // Tiny dependency-free extraction of "param_hash": "<hex>".
    let key = "\"param_hash\"";
    let i = json.find(key).expect("param_hash present");
    let rest = &json[i + key.len()..];
    let colon = rest.find(':').unwrap();
    let after = &rest[colon + 1..];
    let q1 = after.find('"').unwrap();
    let q2 = after[q1 + 1..].find('"').unwrap();
    after[q1 + 1..q1 + 1 + q2].to_string()
}

// ---------------------------------------------------------------------------
// KEYGEN / SIGN / VERIFY-SIG
// ---------------------------------------------------------------------------

const SIGN_WB: &str = "---\nruntime: bash\n---\n\n```bash\necho hi\n```\n";

/// Generate a keypair under `home`'s isolated ~/.wb and return the pubkey hex.
fn keygen_pubkey(home: &std::path::Path) -> String {
    let o = wb(home)
        .args(["keygen", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "keygen failed: {}", err(&o));
    let json = out(&o);
    // Extract "pubkey": "<hex>" (not pubkey_file).
    let key = "\"pubkey\"";
    let i = json.find(key).unwrap();
    let rest = &json[i + key.len()..];
    let colon = rest.find(':').unwrap();
    let after = &rest[colon + 1..];
    let q1 = after.find('"').unwrap();
    let q2 = after[q1 + 1..].find('"').unwrap();
    after[q1 + 1..q1 + 1 + q2].to_string()
}

#[test]
fn keygen_writes_keypair() {
    let home = tempdir().unwrap();
    let o = wb(home.path()).args(["keygen"]).output().unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    let key = home.path().join(".wb/keys/wb_signing_key");
    let pub_ = home.path().join(".wb/keys/wb_signing_key.pub");
    assert!(key.exists(), "private key not written");
    assert!(pub_.exists(), "public key not written");
}

#[test]
fn keygen_custom_out_prefix() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("mykey");
    let o = wb(home.path())
        .args(["keygen", "--out", prefix.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(prefix.exists(), "private key at prefix not written");
    assert!(
        dir.path().join("mykey.pub").exists(),
        "public key at prefix not written"
    );
}

#[test]
fn sign_writes_detached_sig() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    let o = wb(home.path())
        .args(["sign", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(
        dir.path().join("w.md.sig").exists(),
        "detached .sig not written"
    );
}

#[test]
fn verify_sig_passes_for_right_key() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let pubkey = keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let o = wb(home.path())
        .args(["verify-sig", wbf.to_str().unwrap(), "--pubkey", &pubkey])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("signature OK"), "got: {}", out(&o));
}

#[test]
fn verify_sig_fails_for_wrong_key() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let wrong = "00000000000000000000000000000000000000000000000000000000000000aa";
    let o = wb(home.path())
        .args(["verify-sig", wbf.to_str().unwrap(), "--pubkey", wrong])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should fail for wrong key");
}

#[test]
fn verify_sig_fails_for_tampered_file() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let pubkey = keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    // Tamper after signing.
    std::fs::write(&wbf, format!("{SIGN_WB}\nextra\n")).unwrap();
    let o = wb(home.path())
        .args(["verify-sig", wbf.to_str().unwrap(), "--pubkey", &pubkey])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should fail for tampered file");
    assert!(
        err(&o).contains("does not verify") || out(&o).contains("does not verify"),
        "stdout: {} stderr: {}",
        out(&o),
        err(&o)
    );
}

#[test]
fn run_verify_sig_runs_when_validly_signed() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let pubkey = keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--verify-sig",
            "--pubkey",
            &pubkey,
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("hi"), "got: {}", out(&o));
}

#[test]
fn run_verify_sig_refuses_when_unsigned() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let pubkey = keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    // No sign step.
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--verify-sig",
            "--pubkey",
            &pubkey,
        ])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse to run unsigned");
}

#[test]
fn run_verify_sig_refuses_when_tampered() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let pubkey = keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    std::fs::write(&wbf, format!("{SIGN_WB}\nextra\n")).unwrap();
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--verify-sig",
            "--pubkey",
            &pubkey,
        ])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse to run tampered");
}

#[test]
fn run_verify_sig_refuses_for_wrong_key() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    keygen_pubkey(home.path());
    let wbf = write(&dir, "w.md", SIGN_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["sign", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let wrong = "00000000000000000000000000000000000000000000000000000000000000aa";
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--verify-sig",
            "--pubkey",
            wrong,
        ])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse to run for wrong key");
}

// ---------------------------------------------------------------------------
// TRUST (TOFU)
// ---------------------------------------------------------------------------

const TRUST_WB: &str = "---\nruntime: bash\n---\n\n```bash\necho trusted\n```\n";

#[test]
fn trust_run_refuses_untrusted() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "t.md", TRUST_WB);
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--require-trust"])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse untrusted");
    assert!(err(&o).contains("untrusted"), "stderr: {}", err(&o));
}

#[test]
fn trust_add_then_run_succeeds() {
    // HOME stable across add + run so the trust store persists.
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "t.md", TRUST_WB);
    let add = wb(home.path())
        .args(["trust", "add", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&add), 0, "stderr: {}", err(&add));
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--require-trust"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("trusted"), "got: {}", out(&o));
}

#[test]
fn trust_run_refuses_after_file_changed() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "t.md", TRUST_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["trust", "add", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    // Mutate after trusting.
    std::fs::write(&wbf, format!("{TRUST_WB}\nmore\n")).unwrap();
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--require-trust"])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse changed file");
    assert!(err(&o).contains("changed"), "stderr: {}", err(&o));
}

#[test]
fn trust_check_and_list() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "t.md", TRUST_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["trust", "add", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let check = wb(home.path())
        .args(["trust", "check", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&check), 0, "stderr: {}", err(&check));
    let list = wb(home.path()).args(["trust", "list"]).output().unwrap();
    assert_eq!(code(&list), 0, "stderr: {}", err(&list));
    assert!(list_mentions(&list, "t.md"), "list stdout/err missing file");
}

fn list_mentions(o: &Output, needle: &str) -> bool {
    out(o).contains(needle) || err(o).contains(needle)
}

// ---------------------------------------------------------------------------
// LOCK
// ---------------------------------------------------------------------------

const LOCK_WB: &str = "---\nruntime: bash\n---\n\n```bash\necho locked\n```\n";

#[test]
fn lock_writes_lockfile() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "l.md", LOCK_WB);
    let o = wb(home.path())
        .args(["lock", wbf.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(
        dir.path().join("l.md.lock").exists(),
        "lockfile not written"
    );
}

#[test]
fn run_locked_runs_when_matching() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "l.md", LOCK_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["lock", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--locked"])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("locked"), "got: {}", out(&o));
}

#[test]
fn run_locked_refuses_on_drift() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "l.md", LOCK_WB);
    assert_eq!(
        code(
            &wb(home.path())
                .args(["lock", wbf.to_str().unwrap()])
                .output()
                .unwrap()
        ),
        0
    );
    // Add a second block → drift from the lockfile.
    std::fs::write(&wbf, format!("{LOCK_WB}\n```bash\necho extra\n```\n")).unwrap();
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--locked"])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse on drift");
    assert!(err(&o).contains("drift"), "stderr: {}", err(&o));
}

#[test]
fn run_locked_custom_lockfile_path() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(
        &dir,
        "c.md",
        "---\nruntime: bash\n---\n\n```bash\necho custom\n```\n",
    );
    let lockpath = dir.path().join("custom.lock");
    let lk = wb(home.path())
        .args([
            "lock",
            wbf.to_str().unwrap(),
            "--lockfile",
            lockpath.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&lk), 0, "stderr: {}", err(&lk));
    assert!(lockpath.exists(), "custom lockfile not written");
    let o = wb(home.path())
        .args([
            "run",
            wbf.to_str().unwrap(),
            "--locked",
            "--lockfile",
            lockpath.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "stderr: {}", err(&o));
    assert!(out(&o).contains("custom"), "got: {}", out(&o));
}

#[test]
fn run_locked_refuses_when_lockfile_missing() {
    let home = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let wbf = write(&dir, "c.md", LOCK_WB);
    // No lock step.
    let o = wb(home.path())
        .args(["run", wbf.to_str().unwrap(), "--locked"])
        .output()
        .unwrap();
    assert_ne!(code(&o), 0, "should refuse with no lockfile");
}

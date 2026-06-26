# Implementation Plan — Wave 3: CI, real subcommands, diagnostics, validate, doctor, step IR design

> Target implementer: Claude Sonnet. Source: TODO.md "Now → Next → Then" sequencing.
> Date drafted: 2026-04-29.
>
> **STATUS (reviewed 2026-06-25): SHIPPED.** All five PRs landed and released as
> v0.16.0 — CI (`.github/workflows/ci.yml`), clap subcommands + `WbExit`
> (`src/exit.rs`), `Diagnostic` + `wb validate` (`src/diagnostic.rs`,
> `src/validate.rs`), `wb doctor` (`src/doctor.rs`), and the step IR — which went
> beyond the "design only" scope here and was fully implemented (`src/step_ir.rs`,
> selective runs, fence attrs, dual-write checkpoints). Verified green: release
> build, `clippy -D warnings`, full test suite. Deferred tails are carried into
> **PLAN-wave4.md**.

## 1. Overview & dependency graph

The five items are one wave because they form a single foundation: CI must exist before any of this lands so churn on `main.rs` doesn't break `cargo test` silently; real clap subcommands must exist before `wb validate` and `wb doctor` can be added cleanly (otherwise we'd be wiring more "manual command interception" — the very thing #38 is killing); the shared `Diagnostic` type is the data shape that `wb validate` emits and that `wb doctor` borrows for shallow checks; and the step IR design is the next-wave anchor that `validate` reaches into for duplicate-id detection but does not implement yet.

```
CI workflow ──► clap Subcommand refactor (#38) ──► Diagnostic type + wb validate (#32a)
                       │                                      │
                       └──► wb doctor (#32b) ◄────────────────┘
                                                              │
                                                 Step IR sketch (#13/#29)  [design only]
```

CI is independent and ships first. The clap refactor gates everything else because adding `Validate` and `Doctor` to today's "manual interception" block is the wrong direction. The diagnostic type ships with `wb validate` (its only consumer in this wave); `wb doctor` reuses the diagnostic struct but with its own check engine. The step IR is design-only — written as types + module skeleton so the next wave can fill in the implementation against the shape the validator already half-uses.

---

## 2. Current-state findings (the things Sonnet should not re-derive)

### Repository shape

- Single binary crate. No workspace. `src/main.rs` is **4451 lines**; `src/parser.rs` is **2051 lines**; `src/executor.rs` is **1306 lines**; `src/exit_codes.rs` is **37 lines**, well-commented, already exports stable codes.
- `tests/` contains shell scripts only — no `tests/*.rs` integration tests yet. All Rust tests are inline `#[cfg(test)] mod tests` in source files.
- `.github/workflows/release.yml` exists (tag-triggered binary releases). **No PR/push CI exists.**
- `Cargo.toml` clap is already `version = "4", features = ["derive", "env"]`. No `clap_complete` or `clap_mangen`.

### Manual command interception (the thing #38 kills)

`src/main.rs:179-245` — `fn main()` reads `std::env::args()` directly and switch-matches `args[1]` against the strings `"update"`, `"version"`, `"run"`, `"inspect"`, `"transform"`, `"pending"`, `"cancel"`, `"resume"`, `"containers"`. Each arm hand-rewrites argv (e.g. `wb run folder/` becomes `wb folder/`) and re-parses with `Cli::parse_from`. The fall-through (`_ => {}`) calls `Cli::parse()` on the original argv, which is why `wb file.md` works without a subcommand.

Specific landmines:
- The top-level `Cli` struct (`src/main.rs:84-177`) is a flat 19-field bag where `file: Option<String>` does double duty as both "workbook to run" and "folder of workbooks." `--inspect` is a bool flag that the dispatcher routes to a different handler.
- `cmd_resume` (`src/main.rs:3087`) defines its own `ResumeCli` struct (`src/main.rs:3023-3085`) and parses `wb-resume` from argv — this is real clap, just hand-routed. It is the model for the rewrite.
- `cmd_containers` (`src/main.rs:2810-2826`) does its own string-match on `args[1]` of the sub-args slice for `build|list|prune` — needs to become a proper `Subcommand` enum.
- `cmd_pending` (`src/main.rs:2945`) hand-parses `--format=json`, `--json`, `--no-reap` from a `&[String]` — also needs to become a derive struct.
- `transform` is undocumented in CLAUDE.md but exists at `src/main.rs:211-218` / `2731`. Keep it; it's a frontmatter scaffolder.

### `process::exit` call sites — full census

Total: **62 across two files** (58 in main.rs, 4 in update.rs).

By exit code, in main.rs:
- `EXIT_WORKBOOK_INVALID` (3): line 34 (include resolution), 3178 (resume on non-paused).
- `EXIT_BLOCK_FAILED` (1): line 486 (folder run with failures).
- `EXIT_SANDBOX_UNAVAILABLE` (5): line 923 (sandbox image build failure on re-enter).
- `EXIT_USAGE` (2): line 1269 (no executable blocks).
- `EXIT_CHECKPOINT_BUSY` (6): lines 1125, 3153 (lock contention on run/resume).
- `EXIT_PAUSED` (42): lines 2532 (`pause_for_signal`), 2670 (`pause_browser_slice`).
- `EXIT_SIGNAL_TIMEOUT` (7): lines 3310, 3382 (resume hits expired wait).
- Bare `exit(0)`: lines 388, 2768, 2861, 3131, 3356 (no-op success paths: empty folder, transform with no vars, etc.).
- Bare `exit(1)` (about 35 sites): the bulk are "print error then exit" patterns for I/O failures, missing checkpoint/pending descriptors, JSON parse errors on signals, etc.
- Special: line 1005 — `process::exit(exit_code)` where `exit_code` is the docker container's status. This is the sandbox re-entry exit and must remain `process::exit` because it forwards a child status.
- Special: line 2024 — `drop(session); process::exit(1)` after run failure. Comment at lines 2019-2022 explains the `drop` is required so the browser sidecar gets graceful shutdown. Same pattern at 1508 / 2532 / 2670 (pause paths).

In update.rs: lines 16, 60, 78, 99 — all the "update failed" error paths.

The pause paths (`pause_for_signal`, `pause_browser_slice`) are typed `-> !` and **must** keep diverging. The plan doesn't try to convert these — they're correct as-is once we accept that "pause" is a control-flow exit, not a returned error.

### Frontmatter struct (the thing validate reads)

`src/parser.rs:6-33` — `pub struct Frontmatter`. All optional fields, derives `Deserialize` with no `#[serde(deny_unknown_fields)]`. Unknown keys silently ignored today. Sub-types: `RequiresConfig` (35), `DirConfig` / `ExecConfig` (untagged enums), `SetupConfig` (untagged enum), `SecretsConfig` (untagged enum, single or multiple `SecretProvider`).

Block-number-keyed maps live here:
- `timeouts: Option<HashMap<u32, String>>` (line 22)
- `retries: Option<HashMap<u32, u32>>` (line 28)
- `continue_on_error: Option<Vec<u32>>` (line 32)

Resolved at `src/parser.rs:791-823` via `Frontmatter::block_policy(block_number: u32) -> BlockPolicy`. Only **two call sites** consume it: `src/main.rs:773` (folder run) and `src/main.rs:1620` (single run). This is the surface area the step IR has to maintain compatibility with.

`parse_duration_secs` (`src/parser.rs:827-848`) accepts `30s`/`5m`/`2h`/`1d`/bare integers. It returns `Result<u64, String>` and is **already** called eagerly from `block_policy` with `eprintln!` warnings on bad input — those become diagnostics in this wave.

### YAML error handling (the line/col gap)

Three `eprintln!` warnings instead of structured errors:
- `src/parser.rs:459` — frontmatter parse error (`eprintln!("wb: frontmatter parse warning: {}", e)`). Falls back to `Frontmatter::default()`. **`serde_yaml::Error` already exposes `.location()` (returns `Option<Location>` with `line()`/`column()`)** — that's the fix the open follow-up is asking for.
- `src/parser.rs:634` — wait block parse error. Same shape.
- `src/parser.rs:676` — include block parse error. Same shape.
- `src/parser.rs:706` — browser block parse error. Same shape.

For `wb validate`, all four sites become diagnostic emissions; the legacy `eprintln!` path stays for `wb run` (so we don't break logs) but gets a `Diagnostic` underlay in a follow-up.

### Include resolver (already structured)

`resolve_includes` (`src/parser.rs:341`) returns `Result<Workbook, String>` — strings. Errors bubble up to `parse_and_resolve` (`src/main.rs:28`) which prints + exits 3. For validate, we want the structured form: target path, line number, parent file, error kind (`missing | cyclic | unreadable | malformed_yaml`). The line numbers are already on `IncludeSpec.line_number` (`src/parser.rs:266`), and the resolver already emits messages like `include at L{}: cannot resolve path '{}' (relative to {}): {}`. Plan: replicate the walk in a `Diagnostic`-emitting form for validate, leaving the existing string-based path for run.

### Per-block policy consumers

Two call sites only: `src/main.rs:773` (folder run) and `src/main.rs:1620` (single run). Both pass `(block_idx + 1) as u32` — i.e. a 1-based block number that excludes `{no-run}` blocks but includes browser slices. The step IR plan must preserve this counting rule (see §3.5).

### Existing inline tests (where new ones go)

- `src/parser.rs` has an extensive `mod tests` (line ~1900+ — verified by `grep`). New parser/validator tests go there.
- `src/main.rs` has `mod tests` at line 3568+ for `merge_signal_into_vars` etc. Subcommand wiring tests go there.
- New top-level `tests/cli_integration.rs` is the right home for `wb validate` / `wb doctor` end-to-end tests using `Command::cargo_bin("wb")` (`assert_cmd` would be added but is *out of scope* — use `std::process::Command` against `target/debug/wb` for now, or use existing patterns).

---

## 3. Per-item plan

### 3.1 PR/push CI workflow

**Files to add:**
- `.github/workflows/ci.yml` — new file.

**Workflow shape:**

```yaml
name: CI

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all-targets --locked
      - run: cargo build --release --locked
```

**Decisions / rationale:**
- Single job, ubuntu-latest only. Cross-platform matrix is `release.yml`'s job; for PR feedback Ubuntu is enough and keeps minutes low.
- `Swatinem/rust-cache@v2` with `shared-key: ci` so the same cache is reused across the four cargo invocations. No per-step cache split.
- `--locked` on test + build to catch un-committed `Cargo.lock` drift.
- `cancel-in-progress: true` so a force-push doesn't queue redundant runs.
- `permissions: contents: read` is least-privilege; CI doesn't write anywhere.
- `release.yml` is left untouched. The `test` job in there can in principle be deleted later (CI covers it), but that's a follow-up — leave it for the maintainer to decide.

**Required-status-check note for the maintainer (not the PR):**
The repo doesn't have branch protection on `main` (gh CLI can verify). Once this workflow is green once, the maintainer should go to GitHub repo → Settings → Branches → Branch protection rules → require `check` to pass before merge. **Do not put this in the README or commit message** — it's an admin action that lives outside the repo.

**Tests to add:** None. The workflow itself is the test.

**Risks:**
- First run will be slow (no cache). Document that in the PR body.
- `cargo clippy -D warnings` will likely flip up new warnings on the existing 7800 lines. Triage: fix anything trivial in the same PR; if clippy turns up something genuinely contentious (e.g. `clippy::too_many_arguments` already has `#[allow]` annotations), keep `#[allow]` and add a TODO. Do not turn off `-D warnings`.
- The release workflow uses `actions/checkout@v6`, `actions/upload-artifact@v7` etc. — match those versions for consistency.

---

### 3.2 #38 — real clap subcommands

**Files to edit:**
- `src/main.rs` — entire `fn main()` and dispatch architecture.
- `Cargo.toml` — *no new deps in this PR.* `clap_complete` and `clap_mangen` are deferred (see "wired but content deferred" below).

**New top-level shape:**

```rust
#[derive(Parser)]
#[command(name = "wb", version, about = "Run markdown workbooks")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Convenience: `wb file.md` is sugar for `wb run file.md` with default flags.
    /// When set, `command` must be None and the file is dispatched through `run`.
    #[command(flatten)]
    bare_run: BareRunArgs,
}

#[derive(clap::Args)]
struct BareRunArgs {
    /// Path to a markdown file or folder of workbooks
    file: Option<String>,

    // Subset of RunArgs: only the flags that worked at the bare level today.
    // (Reuses the same flag names so existing scripts keep working.)
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")]
    json: bool,
    #[arg(long, group = "format")]
    yaml: bool,
    #[arg(long, group = "format")]
    md: bool,
    // ... mirror the RunArgs flags exactly.
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Inspect(InspectArgs),
    Validate(ValidateArgs),
    Doctor(DoctorArgs),
    Pending(PendingArgs),
    Resume(ResumeArgs),
    Cancel(CancelArgs),
    Containers(ContainersArgs),
    Update(UpdateArgs),
    Version,
    /// Hidden — frontmatter scaffolding helper. Kept for backwards compat.
    #[command(hide = true)]
    Transform(TransformArgs),
}
```

**Per-subcommand args (shape only; signatures Sonnet should adopt):**

```rust
#[derive(clap::Args)]
struct RunArgs {
    file: String, // required; bare-run path collapses into here

    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")] json: bool,
    #[arg(long, group = "format")] yaml: bool,
    #[arg(long, group = "format")] md: bool,

    #[arg(long)] secrets: Option<String>,
    #[arg(long)] project: Option<String>,
    #[arg(long = "secrets-cmd")] secrets_cmd: Option<String>,

    #[arg(short = 'C', long)] dir: Option<String>,
    #[arg(short, long)] quiet: bool,
    #[arg(long)] bail: bool,
    #[arg(long)] no_setup: bool,

    #[arg(long, default_value = "a-z")] order: String, // folder mode only
    #[arg(long)] checkpoint: Option<String>,

    #[arg(long, env = "WB_CALLBACK_URL")] callback: Option<String>,
    #[arg(long = "callback-secret", env = "WB_CALLBACK_SECRET")] callback_secret: Option<String>,
    #[arg(long = "callback-key", env = "WB_CALLBACK_KEY")] callback_key: Option<String>,

    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")] set_vars: Vec<String>,
    #[arg(long = "env-file", value_name = "PATH")] env_files: Vec<String>,
    #[arg(long = "env-file-relative")] env_file_relative: bool,
    #[arg(long)] redact: Vec<String>,
}

#[derive(clap::Args)]
struct InspectArgs {
    file: String,
    /// Emit JSON instead of human prose
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
struct ValidateArgs {
    /// File or folder to validate. Folder mode validates every .md.
    file: String,
    /// Output format (text|json). Default: text.
    #[arg(long, default_value = "text")]
    format: String,
    /// Treat warnings as errors (raise exit code from 0 to 3).
    #[arg(long = "strict")]
    strict: bool,
}

#[derive(clap::Args)]
struct DoctorArgs {
    /// Run optional checks that probe Docker/Redis/sidecar (slower, network).
    #[arg(long)]
    deep: bool,
    /// Output format (text|json). Default: text.
    #[arg(long, default_value = "text")]
    format: String,
}

#[derive(clap::Args)]
struct PendingArgs {
    #[arg(long, default_value = "text")] format: String, // text|json
    #[arg(long = "no-reap")] no_reap: bool,
}

#[derive(clap::Args)]
struct ResumeArgs { /* lift the existing ResumeCli at src/main.rs:3023 verbatim */ }

#[derive(clap::Args)]
struct CancelArgs { id: String }

#[derive(clap::Args)]
struct ContainersArgs {
    #[command(subcommand)]
    sub: ContainersSub,
}

#[derive(Subcommand)]
enum ContainersSub {
    Build { path: Option<String> },
    #[command(alias = "ls")]
    List,
    Prune,
}

#[derive(clap::Args)]
struct UpdateArgs {
    #[arg(long)] check: bool,
}

#[derive(clap::Args)]
struct TransformArgs { file: String }
```

**Exit-code plumbing — replace `process::exit` with `WbExit`:**

Add a new module `src/exit.rs` (or extend `src/exit_codes.rs`):

```rust
// src/exit.rs
use crate::exit_codes;

/// Typed result of a command. The Display value is logged before exit;
/// `code()` produces the documented numeric exit code.
#[derive(Debug)]
pub enum WbExit {
    Success,
    BlockFailed,           // 1
    Usage(String),         // 2
    WorkbookInvalid(String), // 3
    SandboxUnavailable(String), // 5
    CheckpointBusy(String), // 6
    SignalTimeout(String), // 7
    Paused,                // 42
    /// I/O or environment failure. Lands on 1 unless mapped above.
    Io(String),
}

impl WbExit {
    pub fn code(&self) -> i32 {
        use WbExit::*;
        match self {
            Success => exit_codes::EXIT_SUCCESS,
            BlockFailed => exit_codes::EXIT_BLOCK_FAILED,
            Usage(_) => exit_codes::EXIT_USAGE,
            WorkbookInvalid(_) => exit_codes::EXIT_WORKBOOK_INVALID,
            SandboxUnavailable(_) => exit_codes::EXIT_SANDBOX_UNAVAILABLE,
            CheckpointBusy(_) => exit_codes::EXIT_CHECKPOINT_BUSY,
            SignalTimeout(_) => exit_codes::EXIT_SIGNAL_TIMEOUT,
            Paused => exit_codes::EXIT_PAUSED,
            Io(_) => 1,
        }
    }

    pub fn message(&self) -> Option<&str> {
        use WbExit::*;
        match self {
            Success | Paused | BlockFailed => None,
            Usage(s) | WorkbookInvalid(s) | SandboxUnavailable(s) | CheckpointBusy(s)
            | SignalTimeout(s) | Io(s) => Some(s),
        }
    }
}
```

`fn main() -> ExitCode`:

```rust
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let exit = match cli.command {
        Some(Command::Run(args)) => cmd_run(args),
        Some(Command::Inspect(args)) => cmd_inspect(args),
        Some(Command::Validate(args)) => cmd_validate(args),
        Some(Command::Doctor(args)) => cmd_doctor(args),
        Some(Command::Pending(args)) => cmd_pending(args),
        Some(Command::Resume(args)) => cmd_resume(args),
        Some(Command::Cancel(args)) => cmd_cancel(args),
        Some(Command::Containers(args)) => cmd_containers(args),
        Some(Command::Update(args)) => { update::cmd_update(args.check); WbExit::Success },
        Some(Command::Version) => { update::cmd_version(); WbExit::Success },
        Some(Command::Transform(args)) => cmd_transform(args),
        None => {
            // Bare-run sugar
            let Some(file) = cli.bare_run.file else {
                print_short_usage();
                return std::process::ExitCode::from(2);
            };
            cmd_run(promote_bare_run(file, cli.bare_run))
        }
    };
    if let Some(msg) = exit.message() {
        eprintln!("wb: {}", msg);
    }
    std::process::ExitCode::from(exit.code() as u8)
}
```

**Process::exit migration strategy** (this is where Sonnet will spend the most time):

- **Top level (`fn main`, `dispatch`, `cmd_*`) — convert.** Every `process::exit(N)` along the path the new `cmd_*` functions return through gets replaced with `return WbExit::Variant(msg)`. About 30 sites.
- **Sandbox re-entry (line 1005)** — keep `process::exit(exit_code)`. This is forwarding a child's status, not a wb-decided code. Document it.
- **Pause paths (`pause_for_signal` line 2532, `pause_browser_slice` line 2670)** — keep `process::exit(EXIT_PAUSED)`. Both are typed `-> !` for a reason: they need to drop the session before exit so the browser sidecar gets graceful shutdown (comments at lines 2019-2022 explain). Threading `WbExit::Paused` up through every call would force the entire run loop to be rewritten as `Result`-returning. *Out of scope this wave.* Mark with a TODO: "Convert to typed exit once the run loop returns Result."
- **`update.rs` (4 sites)** — the cleanest fix is to make `update::cmd_update` return `WbExit`. Do it; it's small and self-contained.
- **Drop-before-exit sites (e.g. line 2024)** — convert to `drop(session); return WbExit::BlockFailed;` from inside the cmd. The rest of the function is already structured to do this.

The plan does **not** require run_single to become `Result`-returning end-to-end in this PR. The realistic deliverable is: every direct entry point from `Command::*` returns `WbExit`, and any `process::exit` left behind is one of the three documented exceptions (sandbox forward, two pause divergences). That's enough to make every command testable via `assert_cmd`-style harnesses without spawning subprocesses for trivial cases.

**Stable exit codes — must not regress:**
- `EXIT_WORKBOOK_INVALID` (3): currently fires from include resolution failure, resume on non-paused checkpoint. After refactor, also from `wb validate` finding errors.
- `EXIT_SANDBOX_UNAVAILABLE` (5): unchanged.
- `EXIT_PAUSED` (42): unchanged (pause path keeps `process::exit`).
- All other codes unchanged.

**Bare-run promotion:**

`wb file.md` (no subcommand) must keep working. Implement by:
1. Letting clap's `Subcommand` be `Option<Command>` with `#[command(flatten)]` for `BareRunArgs`.
2. When `command` is `None` and `bare_run.file` is `Some`, build a `RunArgs` from `BareRunArgs` (fields are a strict subset) and dispatch to `cmd_run`.
3. When `command` is `None` and `bare_run.file` is `None`, print a short usage and exit 2.

Avoid `subcommand_required = false` + arg-required-on-subcommand traps; verify with a unit test that all three forms parse: `wb file.md`, `wb run file.md`, `wb run folder/`.

**Shell completions / man pages — wired but content deferred:**

Reserve, but don't ship:
- Add a hidden `Command::Completion { shell: clap_complete::Shell }` and a hidden `Command::Man` — but stub them with `eprintln!("not implemented yet — pending clap_complete/clap_mangen")` and exit 2. This way the names are reserved and the next wave just turns them on.
- Do **not** add `clap_complete` / `clap_mangen` to `Cargo.toml` in this PR. They're behind a feature flag in clap 4 and adding them now means committing to generated content that will drift. The next wave that actually ships completions can land both deps and the implementation together.

**Tests to add:**

In `src/main.rs` (`mod tests`):
- `parses_bare_run` — `Cli::parse_from(["wb", "file.md"])` resolves to `command=None, bare_run.file=Some("file.md")`.
- `parses_bare_run_with_json` — `Cli::parse_from(["wb", "file.md", "--json"])`.
- `parses_run_subcommand` — `Cli::parse_from(["wb", "run", "file.md", "--bail"])` resolves to `command=Some(Run(_))`.
- `parses_inspect_subcommand` — both `wb inspect file.md` and `wb inspect file.md --json`.
- `parses_containers_subcommands` — list, build, prune; alias `ls`.
- `parses_resume_with_value` — `wb resume my-id --value 12345`.
- `parses_pending_no_reap`.
- `wb_exit_codes_match_documented` — assert each `WbExit` variant returns the constant from `exit_codes` module.

New `tests/cli_smoke.rs`:
- `unknown_subcommand_exits_2` — spawn the binary with `wb nonsense`, expect exit 2.
- `version_subcommand_prints_version` — exit 0, stdout starts with `wb v`.

**Risks / open decisions:**
- **The flat top-level `Cli` struct's `--inspect` flag has to die.** Today `wb file.md --inspect` works; after this refactor it doesn't (you'd write `wb inspect file.md`). This is a small breaking change. CLAUDE.md and README reference both `wb inspect file.md` and `wb file.md --inspect`. **Decision:** support both — make `BareRunArgs` carry an `--inspect` bool too, and on bare-run promotion, if `inspect` is set, dispatch to `cmd_inspect` instead of `cmd_run`. That's three lines of routing and avoids breaking shell aliases.
- **`-v` / `--verbose` is hidden today (line 127-128).** Kept-for-backward-compat. Hide it on the new `RunArgs` too with `#[arg(hide = true)]`.
- **`wb update --check` semantics.** Today it's a free-form arg scan: `let check_only = args.iter().any(|a| a == "--check")`. Promote to a proper bool flag.
- **Concurrency with `wb run` calling `cmd_resume`.** Today `cmd_resume` builds a `RunConfig` and calls `run_single`. After refactor it'll build a `RunArgs` and call `cmd_run`. Make sure the `RunConfig` builder helper stays accessible (currently a private struct in main.rs at line 878).

---

### 3.3 #32a — `wb validate` + shared `Diagnostic` type

**Files to add:**
- `src/diagnostic.rs` — new module.
- `src/validate.rs` — new module (validators that consume the workbook + frontmatter and emit diagnostics).

**Files to edit:**
- `src/main.rs` — add `mod diagnostic; mod validate;` and the `cmd_validate` function (per §3.2).
- `src/parser.rs` — add `pub fn parse_with_diagnostics(input: &str) -> (Workbook, Vec<Diagnostic>)`. Existing `parse(input)` stays as a thin wrapper that drops the diagnostics. **No behavior change for `wb run`.**

**Diagnostic struct:**

```rust
// src/diagnostic.rs

use std::path::PathBuf;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Error, Warning, Note }

/// Source span. `len` is in bytes. `line` and `col` are 1-based, matching the
/// rest of the CLI (`L{}` prefix, `[N/total]` block labels). `byte_offset` is
/// the start offset into the source file, used for editor integrations and
/// for mapping serde_yaml error locations.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub byte_offset: u32,
}

impl Span {
    pub fn point(line: u32, col: u32) -> Self {
        Self { line, col, len: 0, byte_offset: 0 }
    }

    /// Map a byte offset in the source string to (line, col). Used to lift
    /// `serde_yaml::Location` (line/col within the YAML payload) into a span
    /// in the parent .md file by adding the YAML region's start offset.
    pub fn from_byte_offset(source: &str, offset: usize) -> Self { /* ... */ }
}

/// A diagnostic code. Stable strings, namespaced by area:
///   `wb-yaml-001` — frontmatter parse error
///   `wb-yaml-002` — wait fence YAML parse
///   `wb-yaml-003` — include fence YAML parse
///   `wb-yaml-004` — browser fence YAML parse
///   `wb-fm-001`   — unknown frontmatter key
///   `wb-fm-002`   — wrong type
///   `wb-fm-003`   — bad duration string in timeouts:
///   `wb-fm-004`   — retries: not a u32
///   `wb-fm-005`   — continue_on_error: not a list of u32
///   `wb-fm-006`   — block-number map references block N but workbook has only M blocks
///   `wb-inc-001`  — missing include target
///   `wb-inc-002`  — circular include
///   `wb-inc-003`  — unreadable include target
///   `wb-attr-001` — unknown fence attribute (deferred until #13 lands; emit nothing for now)
///   `wb-secret-001` — bad secret provider config (unknown provider name)
///   `wb-step-001` — duplicate explicit step id  (deferred until step IR lands; emit nothing for now)
pub type Code = &'static str;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub span: Option<Span>,
    pub file: PathBuf,
    /// Optional remediation hint shown after the message in text format.
    /// Skipped in JSON if None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: Code, file: impl Into<PathBuf>, message: impl Into<String>) -> Self { /* ... */ }
    pub fn warning(code: Code, file: impl Into<PathBuf>, message: impl Into<String>) -> Self { /* ... */ }
    pub fn with_span(mut self, span: Span) -> Self { self.span = Some(span); self }
    pub fn with_help(mut self, help: impl Into<String>) -> Self { self.help = Some(help.into()); self }
}

/// Render a slice of diagnostics for the user. Format mirrors rustc's
/// emitter at the prose level: file:line:col: severity[code]: message.
pub fn render_text(diags: &[Diagnostic]) -> String { /* ... */ }
pub fn render_json(diags: &[Diagnostic]) -> String { /* serde_json {"diagnostics": [...]} */ }

/// Aggregate counts for exit-code mapping.
pub fn counts(diags: &[Diagnostic]) -> (usize, usize) {
    /* (errors, warnings) */
}
```

**Validators (src/validate.rs):**

```rust
// src/validate.rs

use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::parser::{Workbook, Frontmatter};
use std::path::Path;

pub struct ValidateOptions {
    pub strict: bool,    // warnings counted as errors
    pub recursive: bool, // folder mode
}

/// Top-level entry. Reads + parses + validates without side effects.
/// Never spawns Docker. Never reads from the network. Never executes a block.
pub fn validate_file(path: &Path, opts: &ValidateOptions) -> Vec<Diagnostic> { /* ... */ }
pub fn validate_dir(path: &Path, opts: &ValidateOptions) -> Vec<Diagnostic> { /* ... */ }

/// The actual validators. Each takes the parsed workbook + the source string
/// (for byte-offset lookups) and pushes diagnostics into `out`.
fn check_frontmatter_schema(fm_yaml: &str, fm_offset: usize, file: &Path, out: &mut Vec<Diagnostic>) { /* ... */ }
fn check_block_policy_indices(wb: &Workbook, file: &Path, out: &mut Vec<Diagnostic>) { /* ... */ }
fn check_includes(wb: &Workbook, file: &Path, out: &mut Vec<Diagnostic>) { /* ... */ }
fn check_secrets_config(fm: &Frontmatter, file: &Path, out: &mut Vec<Diagnostic>) { /* ... */ }
fn check_durations(fm: &Frontmatter, file: &Path, out: &mut Vec<Diagnostic>) { /* ... */ }
```

**Validator scope (this wave):**

1. **Frontmatter YAML parse errors** with line/col from `serde_yaml::Error::location()`. Code: `wb-yaml-001`. The frontmatter starts at a known byte offset in the .md file (`extract_frontmatter` already computes it implicitly — surface it as a return value: `extract_frontmatter` becomes `extract_frontmatter(input: &str) -> (Frontmatter, String, FrontmatterRegion)` where `FrontmatterRegion { yaml_start_byte: usize, yaml_text: String }`). Then the validator can `parse_yaml_with_pos(yaml_text)` and translate the `Location` back into the parent file's span. **This fix also closes the open follow-up "Line/column for malformed frontmatter YAML parse errors."**

2. **Unknown frontmatter keys.** Add `#[serde(deny_unknown_fields)]` to `Frontmatter` *only in a parallel struct used by validate*. The runtime `Frontmatter` keeps its tolerant deserialization (don't break old workbooks). The validation struct re-deserializes the same YAML region with strict mode and converts any `unknown field` errors to diagnostics with code `wb-fm-001`. Span = the YAML key's position from `serde_yaml`'s error location.

3. **Wrong types** (`wb-fm-002`). E.g. `runtime: [python]` instead of `runtime: python`. Falls out of the strict deserialize path.

4. **Malformed durations in `timeouts:`** (`wb-fm-003`). Walk the existing `timeouts` map; for each value, call `parse_duration_secs`; on `Err`, emit a diagnostic. Span: best-effort point at the line of the `timeouts:` block — getting the exact value-line requires re-parsing the YAML with `serde_yaml::Mapping` (the unstructured form) so we can pull line numbers from each entry. Acceptable to emit a less-precise span (block-level) in this wave; tighten in follow-up.

5. **Block-number maps referencing nonexistent blocks** (`wb-fm-006`). Today `timeouts: {99: 30s}` on a 3-block workbook silently does nothing. Validate that each key in `timeouts` / `retries` and each entry in `continue_on_error` is `<= block_count`.

6. **Wait/include/browser fence YAML** (`wb-yaml-002/003/004`). Same pattern as frontmatter — re-parse from `extract_sections` with location info. The `line_number` is already on the spec; use it as the base and add the inner YAML offset.

7. **Include resolution errors** (`wb-inc-001/002/003`). Re-walk `resolve_includes` in a non-fatal mode that collects rather than returning `Err`. Plan: extract a `pub fn try_resolve_includes(wb: Workbook, parent_path: &Path) -> (Workbook, Vec<IncludeError>)` from the existing function. The existing `resolve_includes` becomes a thin wrapper that returns `Err(...)` if any error was collected (preserves runtime behavior).

8. **Bad secret provider config** (`wb-secret-001`). Shape-only check: walk every `SecretProvider` in `frontmatter.secrets`, assert `provider` is one of `{"env", "doppler", "yard", "command", "cmd", "dotenv", "file", "prompt"}`. Do **not** invoke doppler / yard. Do **not** check whether the binary is on PATH (that's `wb doctor`'s job).

9. **Unknown fence attrs** — defer. Today `parse_info_string` already "ignores unknown attrs to stay forward-compatible" (line 484-485 comment). Until #13 lands and there's a defined attr vocabulary, we can't say what's unknown. Plan: leave `wb-attr-001` reserved in the diagnostic codes table but emit nothing.

10. **Duplicate explicit step ids** — defer to step-IR wave (no explicit ids exist yet).

**Output formatting:**

- Text: rustc-style `file:line:col: error[code]: message\n   = help: ...`. Folder mode prefixes each block with the file path.
- JSON: `{"diagnostics": [{...}, {...}], "summary": {"errors": N, "warnings": N}}`. The shape is what agents will key on; lock it down now and never reshape.

**Exit code mapping for `wb validate`:**

- 0 errors, 0 warnings → exit 0.
- 0 errors, ≥1 warning → exit 0 (non-strict) or `EXIT_WORKBOOK_INVALID` (strict).
- ≥1 error → `EXIT_WORKBOOK_INVALID` (3).
- File missing / read error → `EXIT_USAGE` (2).

**Hard guarantees `wb validate` must enforce:**

Add an `#[cfg(test)]` mod with a guard test that `cmd_validate` does not transitively call:
- `executor::Session::new` / `execute_block` / `execute_browser_slice`
- `sandbox::build_image` / `run_in_sandbox` / `image_exists`
- `secrets::resolve_secrets` (do not actually invoke doppler/yard)
- `callback::CallbackConfig::*` (no HTTP)
- `signal::*` (no Redis)

This is impossible to test perfectly in unit tests, but a smoke integration test can run `wb validate examples/secrets-demo.md` with a chaos-mode env that points doppler at a nonexistent token; expect exit 0 (no errors at the shape level) and zero outbound network attempts (verify by setting `HTTP_PROXY=http://127.0.0.1:1`).

**Tests to add:**

In `src/diagnostic.rs` (`mod tests`):
- `span_from_byte_offset_basic` — multiple lines, verify line/col mapping.
- `render_text_includes_code` / `render_json_shape_locked`.
- `counts_separates_errors_and_warnings`.

In `src/validate.rs` (`mod tests`):
- `validates_clean_workbook_examples_hello_md` — every file in `examples/` should validate clean except known-bad fixtures.
- `unknown_frontmatter_key_errors` — temp file with `unknownKey: foo`, expect `wb-fm-001`.
- `bad_duration_in_timeouts` — `timeouts: {1: 5xyz}`, expect `wb-fm-003` with span pointing at line of value.
- `out_of_range_block_number` — 2-block workbook, `timeouts: {5: 30s}`, expect `wb-fm-006`.
- `malformed_frontmatter_yaml_has_line_col` — frontmatter with `runtime: [\n` (unterminated), expect `wb-yaml-001` with span on the broken line. **This is the test that closes the open follow-up.**
- `missing_include_emits_wb_inc_001`.
- `circular_include_emits_wb_inc_002`.
- `bad_secret_provider_emits_wb_secret_001`.
- `validate_does_not_open_docker` — fixture with `requires:` block, assert no docker subprocess (negative test by checking that `sandbox::*` is not called — easiest is to factor `validate_file` so it never imports `sandbox`).

New `tests/validate_cli.rs`:
- `wb_validate_examples_dir_zero_exit`.
- `wb_validate_format_json_shape` — parse the output as JSON, assert `diagnostics` array exists.
- `wb_validate_strict_promotes_warnings` — fixture with one warning, normal exit 0, strict exit 3.

**Risks / open decisions:**
- **Re-deserializing the frontmatter YAML twice** (tolerant for runtime, strict for validate) is the cleanest compatibility story. The cost is ~2 ms on a small workbook. Document the choice.
- **Span precision tradeoff**. `serde_yaml::Error::location()` gives line/col within the YAML region only. The validator needs to add the line/col of where the YAML region starts in the parent .md (typically line 2, col 1 for frontmatter — frontmatter follows `---\n`). Encode this as `FrontmatterRegion { start_line, start_col }` and include in the validator's input. For fence YAML (wait/include/browser), the start line is `spec.line_number + 1` (the line after the opening ``` ).
- **JSON shape lock-in**. Once agents write code against `diagnostics[].code` strings, those codes can't be renamed. List the codes in a `// STABILITY: codes are part of the public CLI contract` comment block at the top of `src/diagnostic.rs`.
- **Don't deny unknown fields on the runtime `Frontmatter`.** Tested workbooks in the wild have spurious keys; flipping this on at runtime would silently fail. Keep the strict struct alongside.

---

### 3.4 #32b — `wb doctor`

**Files to add:**
- `src/doctor.rs` — new module.

**Files to edit:**
- `src/main.rs` — add `mod doctor;` and `cmd_doctor` per §3.2.

**Doctor module shape:**

```rust
// src/doctor.rs

use crate::diagnostic::{Diagnostic, Severity};
use std::path::PathBuf;

#[derive(Debug)]
pub struct CheckResult {
    pub name: &'static str,           // "rust-runtime", "docker", "wb-browser-runtime"
    pub status: CheckStatus,
    pub detail: Option<String>,        // version string, path, etc.
    pub diagnostic: Option<Diagnostic>, // None when status == Pass
}

#[derive(Debug)]
pub enum CheckStatus { Pass, Warn, Fail, Skipped }

pub struct DoctorOptions { pub deep: bool, pub format: Format }
pub enum Format { Text, Json }

pub fn run(opts: &DoctorOptions) -> (Vec<CheckResult>, /*exit*/ i32) { /* ... */ }
```

**Shallow checks (run by default, no network, no Docker pulls):**

1. `wb_version` — print `CARGO_PKG_VERSION` and the binary path (`std::env::current_exe()`).
2. `runtime_bash` — `which bash` + `bash --version | head -1`.
3. `runtime_python` — `python3 --version`. Warn if absent.
4. `runtime_node` — `node --version`. Warn if absent.
5. `runtime_ruby` — `ruby --version`. Warn (low severity) if absent.
6. `docker_present` — `docker version --format '{{.Server.Version}}'` (a single subprocess; no image probes). Pass = exit 0, Warn = `docker` exists but daemon down.
7. `home_dir_writable` — assert `~/.wb/` exists (or `mkdir -p`), assert it's writable. Existing code (`checkpoint::checkpoint_path`) already uses `~/.wb/checkpoints/`; reuse the resolution helper if present, else `dirs`-style derivation from `$HOME`.
8. `wb_browser_runtime_present` — Use `which wb-browser-runtime` or check for `node_modules/.bin/wb-browser-runtime` in PWD. Don't run it.

**Deep checks (only with `--deep`; explicitly listed in help text):**

1. `docker_build_smoke` — build a tiny `FROM alpine` image with one RUN. Surfaces "Docker daemon is reachable but networking is broken."
2. `sidecar_handshake` — spawn the browser sidecar with a no-op verb list, confirm it exits 0 within ~5s. Skipped if `wb_browser_runtime_present` failed.
3. `redis_ping` — only if `WB_CALLBACK_URL` starts with `redis://` or `rediss://`, or if `WB_SIGNAL_URL` is set; PING the URL.

**Output:**

- Text: ASCII status column + per-check detail. Mimic `brew doctor` style.
  ```
  wb doctor
  ✓ wb v0.11.1 (/usr/local/bin/wb)
  ✓ bash 5.2.21
  ✓ python3 3.12.4
  ⚠ ruby (not on PATH)
  ✓ docker 26.0.1
  ✓ ~/.wb writable
  ⚠ wb-browser-runtime (not installed; install via npm if you use browser blocks)
  Pass: 5  Warn: 2  Fail: 0
  ```
- JSON: `{"checks": [{"name":"...", "status":"pass|warn|fail|skipped", "detail":"...", "diagnostic": {...}}], "summary": {...}}`.

**Exit codes for `wb doctor`:**

- All Pass → 0.
- Any Warn but no Fail → 0.
- Any Fail → 3 (`EXIT_WORKBOOK_INVALID`). *Note:* this exit code is reused; the user-facing meaning ("environment isn't ready") is documented in `--help` output.

**Decoupling from validate:**

Doctor must not import `validate.rs`. The two share `Diagnostic` (the type) and that's it. A workbook can be valid (`wb validate file.md` → exit 0) on a machine where `wb doctor` would fail (no Docker), and vice versa.

**Tests to add:**

In `src/doctor.rs` (`mod tests`):
- `which_resolution_runs` — call the internal `resolve_binary_version("bash")` helper on the test machine; assert it doesn't panic.
- `deep_mode_skips_when_runtime_missing` — feed a fake CheckResult vector through the aggregator, assert the dependency skip is recorded.
- `format_text_renders_warns_with_warn_glyph`.

New `tests/doctor_cli.rs`:
- `wb_doctor_shallow_zero_exit_on_dev_machine` — hostile to CI without bash, but bash is everywhere on Ubuntu runners, so this is OK.
- `wb_doctor_format_json_shape`.

**Risks / open decisions:**
- **CI implications.** `cargo test` will run the doctor smoke tests; CI runners have bash + python + node but no Docker daemon (typically). Make sure shallow doctor passes on the CI runner — Docker check should be `Warn` not `Fail` when the daemon is down.
- **`--deep` is destructive-adjacent.** Building a tiny image and running a sidecar are real side effects. Document very loudly in `--help` that `--deep` builds an image and may pull from Docker Hub.
- **Where does `~/.wb` live?** Today, scattered: `pending::descriptor_path`, `checkpoint::checkpoint_path`. The doctor should call those helpers, not duplicate the path logic. If a helper doesn't exist, add `pub fn wb_dir() -> PathBuf` somewhere central (probably `src/checkpoint.rs` or a new `src/paths.rs`) — out of scope unless it falls naturally out of the work.

---

### 3.5 #13/#29 step IR — design only, no implementation

**Files to add:**
- `src/step_ir.rs` — new module **with types + doc comments + `unimplemented!()` constructors**. No behavior wired up. The point is to land the design in code so the next wave can fill the bodies in without re-relitigating the shape.

**Files to edit:**
- `src/main.rs` — add `mod step_ir;`. Don't reference it yet from the run path.
- `src/parser.rs` — add doc comments pointing to the `step_ir` module from `Frontmatter::block_policy` so future readers understand the migration plan.

**Type sketch:**

```rust
// src/step_ir.rs
//
// DESIGN ONLY — types are reserved; implementations land in the next wave.
// See TODO.md #13 / #29.
//
// The current execution model keys per-block configuration (timeouts, retries,
// continue_on_error) on 1-based block number. That number is brittle: editing
// the workbook to insert a block silently shifts every downstream key. Stable
// step IDs replace block-number indexing as the canonical reference for
// per-block config, checkpoints, callbacks, cache entries, selective runs,
// and docs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Stable identifier for an executable step. Survives edits, includes, and
/// reorderings as long as the user-supplied `{#id}` is preserved.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepId(pub String);

/// The new universal identifier vocabulary for fence attributes. Currently
/// only id is implemented; tags / classes / kv attrs land with #13.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FenceAttrs {
    /// Explicit id from `{#id}`. None means the id was hash-derived.
    pub explicit_id: Option<String>,
    /// `.tag` classes (Pandoc-style).
    pub classes: Vec<String>,
    /// Key/value attrs (`timeout=30s`, `retries=2`, `continue_on_error`).
    /// Values are stored as strings; the Frontmatter compatibility shim
    /// (see below) populates this from the existing maps.
    pub kv: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub byte_offset: u32,
}

/// Where in the include tree this step came from. The chain is identical to
/// `parser::IncludeFrame` but tracked per-step so the run loop can ask "what
/// chain produced this step?" without re-walking the section list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeFrame {
    pub id: String,           // path relative to id_root, mirrors parser::IncludeFrame
    pub title: Option<String>,
    /// Position within the parent's section list at which this include opened.
    /// Used as input to the position-hash for stable IDs.
    pub call_site: u32,
}

/// Origin metadata used to build a content-addressed step id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub file: PathBuf,
    /// Position within the *current* file's section list (0-based).
    /// Hashed into the step id so two identical fenced blocks at different
    /// positions get different ids.
    pub position: u32,
}

/// One executable step in the resolved workbook. Replaces the (block_idx,
/// block_number) tuple as the canonical handle. A `Vec<Step>` replaces the
/// today's filtered iteration over `Section::Code | Section::Browser`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: StepId,
    pub attrs: FenceAttrs,
    pub span: Span,
    pub source: Source,
    pub language: String,        // "bash", "python", "browser", ...
    pub body: String,            // code text or browser slice raw YAML
    pub include_chain: Vec<IncludeFrame>,
}

impl Step {
    /// Stable id rules:
    ///   1. If `attrs.explicit_id` is Some, use `StepId(explicit_id.clone())`.
    ///      Duplicate ids are an error caught by validate (`wb-step-001`).
    ///   2. Otherwise hash the include chain ids + position-within-parent +
    ///      language + body-prefix into a short hex string with prefix `auto-`.
    ///      Deterministic so the same workbook produces the same auto ids on
    ///      every parse.
    ///
    /// Hash function: SHA-256 of "{include_chain}\0{position}\0{language}\0{first_64_bytes(body)}",
    /// truncated to 12 hex chars.
    pub fn compute_id(/* ... */) -> StepId { unimplemented!() }
}

/// Translate the existing block-number-keyed maps into per-step config. Runs
/// once at parse time; the run loop reads `step.policy()` instead of calling
/// `frontmatter.block_policy(block_number)`.
pub struct StepPolicy {
    pub timeout_secs: Option<u64>,
    pub retries: u32,
    pub continue_on_error: bool,
}

pub fn resolve_step_policies(
    steps: &[Step],
    fm: &crate::parser::Frontmatter,
) -> Vec<StepPolicy> {
    // Compatibility shim:
    // - Legacy `timeouts: {1: 30s}` maps the *1-based runtime block number* (which
    //   excludes {no-run} blocks but includes browser slices). For each step in
    //   `steps`, compute its 1-based runtime block number and look it up in the
    //   legacy map. Same for retries and continue_on_error.
    // - Future fence-attr `timeout=30s` on the step itself wins over the legacy map.
    //   Conflict resolution: warn via diagnostic, fence-attr wins.
    unimplemented!()
}
```

**Migration path (described in this file as comments, executed in next wave):**

1. Parser produces `Vec<Step>` alongside today's `Vec<Section>`. Both views live in `Workbook` for one release; the run loop reads sections (unchanged), but `wb inspect`, `wb validate`, callbacks (`step.complete`'s `step_id` field), and the new selective-run flags read from `Vec<Step>`.
2. Once external tooling (the website's `step_id` consumer in particular) is keying on `Step.id`, the run loop migrates to iterating `Vec<Step>` directly. Block-number-keyed maps stop being the source of truth — they're translated into per-step policies at parse time.
3. Eventually `Frontmatter::block_policy(u32)` is deprecated; `block_policy_by_id(&StepId)` replaces it.

**Compatibility-shim invariants — must hold:**
- `wb run workbook.md` produces identical behavior with or without the step-IR layer present, as long as the workbook doesn't use any new fence attrs. Tested by parameterizing existing run-loop tests over both code paths.
- `timeouts: {1: 30s}` still applies to "the first executable runtime block" (excluding `{no-run}`). When `{#first} timeout=30s` is on that same block, fence-attr wins; emit `wb-step-002` warning ("legacy block-number timeout shadowed by fence attr").

**Duplicate-id detection (validator):**

`wb validate` runs `compute_step_id` on every step, builds `HashMap<StepId, Vec<Span>>`, and emits one `wb-step-001` per id with > 1 span. Implemented in this wave **only after the step IR module exists**; if the step IR is design-only, the duplicate-id check is also deferred.

**Decision: Land step IR types this wave, but no consumers.**

The `#[allow(dead_code)]` annotations will fire — accept them, they're the marker that this is held-back design work. Add a `// LATER: see TODO.md #13/#29 — implementation pending.` at the top of the file.

**Tests to add:**

In `src/step_ir.rs` (`mod tests`) — only what's actually wired up:
- `step_id_is_serializable`.
- `fence_attrs_default_is_all_empty`.

That's it. The interesting tests (compute_id stability, policy migration) come with the implementation.

**Risks / open decisions:**
- **Hash truncation.** 12 hex chars = 48 bits, ~2^24 collisions on a single workbook. That's ample for a single-doc lookup but if we ever cross-reference cache entries across workbooks, want 16+ chars. Document the choice; revisit when #18 (cache) lands.
- **Explicit id syntax.** `{#login}` per Pandoc is non-controversial, but `{#login .critical timeout=30s}` ordering matters for the parser. Defer the actual `parse_info_string` extension until the implementation wave; the IR types just say "explicit_id is Some when the user wrote `{#X}`."
- **`Section::IncludeEnter` / `IncludeExit` interaction.** Today they advance neither block_idx nor section iteration logic mutates them. The Step list flattens these into `include_chain` fields, but the original sections list keeps the markers for the run loop. **Don't** delete `Section::IncludeEnter/Exit` in this wave.

---

## 4. Out of scope for this wave

Sonnet will be tempted; resist:

- **#13 fence-attr parsing implementation.** Module exists, types defined, no parser change. `parse_info_string` keeps its current "ignore unknown attrs" behavior.
- **#14 typed parameters / `--param`.** Not touched.
- **#16 / #31 `expect`/`assert` fences and `wb test`.** Not touched.
- **#18 source-hash cache.** Not touched.
- **#19-23 browser reliability work.** Not touched.
- **#25 thiserror migration.** Tempting (the validators want a unified error type), but pulling in `thiserror` for one module would be inconsistent with the rest of the crate. Wait until `parser` and `executor` are also being unified.
- **#26 typing the sidecar/checkpoint state.** Not touched.
- **The four open follow-ups** other than the YAML line/col one:
  - Persisted `--callback` URL across resume/reap — separate change to `pending::Descriptor`. Not this wave.
  - HTTP callback ordering — separate work in `callback.rs`. Not this wave.
  - `reap_expired` should acquire the per-ckpt lock — small fix, but in `pending.rs`, doesn't share files with this wave's PRs. Not this wave.
- **Removing `process::exit` from pause paths.** Documented exception above.
- **Shell completions / man pages content.** Hidden subcommand stubs only.
- **Branch protection rule on `main`.** Note for the maintainer; not a code change.
- **Cross-platform CI matrix** (macOS / Windows). Ubuntu only this wave; release.yml already covers cross-platform builds at tag time.

---

## 5. Suggested commit / PR breakdown

Five PRs, in order. Each one should pass CI on its own and not depend on the next one being merged.

### PR 1 — `ci: add PR/push workflow`

- One file: `.github/workflows/ci.yml`.
- Likely also: small follow-up commits to fix any clippy warnings or fmt diffs the workflow surfaces. Land those with the workflow, not after — otherwise the workflow's first run goes red.
- Body: "Adds `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, and `cargo build --release` on PRs and pushes to main. Uses Swatinem/rust-cache. Note for maintainers: enable `check` as a required status check in repo settings once this is green."
- Target review time: 30 minutes.

### PR 2 — `cli: replace manual command interception with clap subcommands (#38)`

- Files: `src/main.rs` (heavy churn), `src/exit.rs` (new), maybe `src/exit_codes.rs` (touched only for re-exports).
- Diff size: ~600-800 lines changed. Most of the churn is mechanical (turning `args[2..]` slicing into struct fields).
- Tests: parse-shape unit tests + smoke `tests/cli_smoke.rs`.
- Risk callouts in PR body:
  - Pause paths still use `process::exit` (intentional).
  - Sandbox re-entry still uses `process::exit` (intentional).
  - `wb file.md --inspect` still works via bare-run promotion.
  - Hidden `Command::Completion` / `Command::Man` stubs reserved.
- Target review time: 2 hours (reviewer should diff-walk every former `process::exit` site).

### PR 3 — `validate: add Diagnostic type and wb validate command (#32a)`

- Files: `src/diagnostic.rs` (new), `src/validate.rs` (new), `src/parser.rs` (touched: `extract_frontmatter` returns region, `try_resolve_includes` factored out, no behavior change), `src/main.rs` (adds `cmd_validate`).
- Tests: in-module + `tests/validate_cli.rs`.
- Risk callouts: locks in `wb-*` diagnostic codes as public API; double frontmatter parse cost (~2ms) accepted.
- This PR also closes the open follow-up "Line/column for malformed frontmatter YAML parse errors" — call that out.
- Target review time: 90 minutes.

### PR 4 — `doctor: add wb doctor with shallow + deep modes (#32b)`

- Files: `src/doctor.rs` (new), `src/main.rs` (adds `cmd_doctor`).
- Tests: in-module + `tests/doctor_cli.rs`.
- Risk callouts: `--deep` builds a Docker image; documented in `--help`. Doctor and validate are decoupled (no cross-imports).
- Target review time: 45 minutes.

### PR 5 — `step_ir: design types for stable step ids (#13/#29)`

- Files: `src/step_ir.rs` (new, types + `unimplemented!()` only), `src/main.rs` (adds `mod step_ir;` only), comment touches in `src/parser.rs`.
- Tests: trivial serializability tests only.
- Risk callouts: the `#[allow(dead_code)]` is intentional. Implementation lands in the next wave per TODO.md sequencing.
- Could be merged as a doc-only PR or held back entirely — at the maintainer's discretion. If held back, the design lives in `TODO.md` instead. Recommended: land it as code so the symbols are reserved.
- Target review time: 30 minutes.

**Sequencing inside Sonnet's run:**

1. PR 1 first, alone. Verify CI is green on a throwaway branch before continuing.
2. PR 2 next. CI from PR 1 will catch every regression in the clap rewrite — that's the whole point.
3. PR 3 + PR 4 can theoretically interleave but are cleaner sequentially because validate's `Diagnostic` type lands first and doctor reuses it.
4. PR 5 last; trivial.

---

## Critical files for implementation

- `/Users/jmitch/Dev/workbooks-dev/workbooks/src/main.rs`
- `/Users/jmitch/Dev/workbooks-dev/workbooks/src/parser.rs`
- `/Users/jmitch/Dev/workbooks-dev/workbooks/src/exit_codes.rs`
- `/Users/jmitch/Dev/workbooks-dev/workbooks/.github/workflows/ci.yml` (to be created)
- `/Users/jmitch/Dev/workbooks-dev/workbooks/Cargo.toml`

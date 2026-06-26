# Implementation Plan — Wave 4: CLI-UX completion, diagnostics tails, config

> Source: the deferred tails of PLAN-wave3.md (#32, #38) plus the next
> TODO.md sequencing step. Date drafted: 2026-06-25.
>
> Wave 3 shipped the foundation (CI, clap subcommands, `Diagnostic`/`validate`,
> `doctor`, step IR). It left two categories of follow-up: the **deferred
> diagnostics** (`#32` — unknown fence attrs, broader callback checks) and the
> **CLI-UX completion** (`#38` — real completions/man, `wb config`, structured
> logging, consistent JSON). This wave closes the first batch and scopes the rest.

## 1. Dependency graph

```
4.1 wb-attr-001 (validate)  ─┐
4.2 callback-config validate ─┼─► [shipped this increment]
4.3 completions + man        ─┤
4.4 wb config                ─┘
                                │
4.5 structured logging flags ◄──┤  [remaining]
4.6 consistent management JSON ◄─┘  [remaining]
```

4.1–4.4 are independent and were implemented together (they touch disjoint
files: `validate.rs`/`parser.rs`/`step_ir.rs`, `callback.rs`/`main.rs`,
`main.rs`/`Cargo.toml`, `config.rs`/`main.rs`). 4.5 and 4.6 are the genuinely
remaining product work and are scoped but not implemented here.

---

## 2. Delivered increment (✅ shipped + tested, 2026-06-25)

### 4.1 — `wb-attr-001`: unknown fence attributes ✅

Closed the fence-attr vocabulary so `wb validate` can surface attrs the runtime
silently ignores.

- **`step_ir::FenceAttrs`** gained `unknown: Vec<String>` (serde
  `default`/`skip_serializing_if`) to retain unrecognized **bare** flags. Unknown
  `key=value` attrs already survive in `kv`.
- **`parser::parse_info_string`** pushes unrecognized bare flags into
  `attrs.unknown` instead of dropping them at the `_ => {}` arm.
- **`validate::check_fence_attrs`** iterates `wb.build_steps()` and emits
  `wb-attr-001` (**warning**, promoted to error under `--strict`) for:
  - kv keys ∉ `{timeout, retries, continue_on_error}` (`KNOWN_KV_ATTRS`)
  - any retained `unknown` bare flag
  `when=`/`skip_if=` are pulled into dedicated `InfoString` fields and never reach
  `kv`, so they don't false-positive; `#id`/`.class` are structural, not attrs.
- Severity is warning because the runtime stays forward-compatible (ignores
  unknown attrs); strict mode is the opt-in to fail on them.
- Diagnostic-code table in `diagnostic.rs` updated (`wb-attr-001` no longer
  "reserved").
- **Tests** (`validate.rs`): unknown kv warns, unknown flag warns, known attrs +
  conditionals don't warn, strict promotes to error.

### 4.2 — Callback-config validation ✅

Callbacks are CLI/env-only (no `callback:` frontmatter), so this is **run-start**
validation, not a static file check.

- **`callback::validate_callback_config(url, secret) -> Result<Vec<String>, String>`**
  (pure, unit-tested): fatal `Err` on a URL scheme ∉
  `{http, https, redis, rediss}`; warnings for redis-URL-with-HMAC-secret (the
  redis path doesn't sign) and plaintext `http://`.
- Wired into `resolve_callback_config`: warnings print, a scheme error exits
  `EXIT_USAGE` (2) — one upfront message instead of a per-event curl failure.
- **Tests** (`callback.rs`): known schemes accepted, unknown rejected, the two
  warning cases.

### 4.3 — Shell completions + man pages ✅

- Added `clap_complete` + `clap_mangen` (proc-macro/build-time; no runtime cost).
- `Command::Completion { shell: clap_complete::Shell }` and `Command::Man` are now
  **visible** commands backed by `cmd_completion` / `cmd_man` (replacing the
  hidden "not implemented" stubs). Generation uses `Cli::command()` via
  `CommandFactory`.
- `wb completion <bash|zsh|fish|elvish|powershell>` → completion script on stdout;
  `wb man` → roff man page on stdout. Bad shell → clap usage error (2) with a
  suggestion.
- **Tests**: parse shapes, non-empty script mentions subcommands, man page has a
  `.TH` header; CLI smoke for `completion bash`.

### 4.4 — `wb config` ✅

Allowlisted machine-wide defaults at `~/.wb/config.yaml` (override
`$WB_CONFIG_PATH`). Uses `serde_yaml` — **no new dependency**.

- **`src/config.rs`**: flat `BTreeMap<String,String>`, `KNOWN_KEYS` allowlist with
  descriptions, `load`/`load_lenient`/`save`/`get`, `is_known_key`, `config_path`.
- **`ConfigSub`**: `get`/`set`/`unset`/`list`/`path`. `set` rejects unknown keys
  (usage error 2); `get` on an unset key is a usage error; `list` prints set
  values + the known-key catalog.
- Known keys = `callback.url`/`callback.secret`/`callback.key`, consumed as the
  **lowest-precedence** fallback in `resolve_callback_config`
  (flag > `WB_CALLBACK_*` env > config). A malformed config file warns
  (`load_lenient`) rather than aborting a run.
- Honesty rule: every allowlisted key is actually read by the run path — no
  decorative settings.
- **Tests**: load-missing-empty, set/save/load roundtrip, malformed-is-error +
  lenient-recovers, allowlist; CLI smoke for set/get/unknown-key.

**Verification:** `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test --all-targets --locked`, `cargo build --release --locked` — all green.
Docs updated in CLAUDE.md (new `wb config` section + CLI table) and TODO.md
(#32 → done, #13 vocabulary note, #38 tails closed).

---

## 3. Second increment (✅ shipped + tested, 2026-06-25)

### 4.5 — Structured logging flags ✅

Hand-rolled to preserve "zero runtime deps" (no `log`/`tracing`).

- **`src/logging.rs`**: process-global `AtomicU8` level + gate macros
  `log_error!`/`log_warn!`/`log_info!`/`log_debug!` (`#[macro_export]`, callable
  as `crate::log_warn!`). The macros forward to `eprintln!` only when the
  severity is enabled, so message text is **unchanged** — they only add
  suppression. Levels: error(0) < warn(1) < info(2, default) < debug(3).
- **Global flag** `--log-level <error|warn|info|debug>` on `Cli`
  (`global = true`, `env = WB_LOG_LEVEL`, default `info`), parsed and applied in
  `main()` before dispatch; an invalid level exits 2. Works before or after the
  subcommand token.
- **Conversion**: the 31 warning-class `eprintln!` sites (`warning:`-prefixed
  diagnostics for checkpoint/outputs/upload/callback/config) → `log_warn!`,
  across `main.rs` (24), `callback.rs` (6), `artifacts.rs` (3), `config.rs`,
  `step_outputs.rs`, `parser.rs`. The leaf-module conversions were done by two
  parallel sub-agents (disjoint files, no conflict). Two `style_fail`-wrapped
  run-loop warnings (stale-checkpoint, replay-failure) were intentionally left as
  `eprintln!` — they're integral styled run feedback, not suppressible noise.
- **Result**: `--log-level error` (or `WB_LOG_LEVEL=error`) gives clean stderr for
  CI/agents; essential UX output is never gated.
- **Tests** (`logging.rs`): level parsing, `enabled` ordering.

### 4.6 — Consistent JSON for management commands ✅

- A reusable `FormatArg { format: String }` (flattened into each subcommand so
  `--format` parses *after* the subcommand token, matching the
  `validate`/`doctor`/`pending` convention), plus `want_json()` (validates
  `text|json`, else usage error 2) and `print_json()` (pretty-print to stdout).
- `--format json` added to: **`version`** (`{"version":…}`), **`config`**
  get/set/unset/list/path, **`containers`** list/build/prune, **`cancel`**.
  Imperative commands carry an explicit `"ok"`/result field (e.g. `config set` →
  `{"ok":true,"key":…,"value":…}`; `cancel` missing → `{"ok":false,…}` + exit 1).
- JSON → stdout; human messages/errors → stderr; **exit codes unchanged**
  (`config get` on an unset key still exits 2, after emitting
  `{"key":…,"value":null}`). `containers build` returns `WbExit::BlockFailed` (1)
  on any build error, matching prior `process::exit(1)` behavior.
- Did **not** force an `{ok,data,error}` envelope onto the already-locked
  domain-shaped JSON of validate/doctor/pending/inspect — consistency = "every
  command emits a pretty-printed JSON object on stdout", not a uniform wrapper.
- **Tests**: `version --format json` parseable, bad format exits 2, `config list
  --format json` shape, containers `--format` parse; manual matrix across all
  commands verified.

**Verification (both increments):** `cargo fmt --check`, `clippy --all-targets
-D warnings`, `cargo test --all-targets --locked` (357 bin + integration), and
`cargo build --release --locked` — all green. CLAUDE.md + TODO.md updated.

## 5. Remaining (next product step, per TODO sequencing)

Wave 4 is **complete** — both increments (4.1–4.4, 4.5–4.6) shipped. What's left
under #38 is no longer "tails" but new feature work:

- **#30 typed parameters + profiles** and **#31 inline assertions + `wb test`** are
  the next *feature* wave, not CLI-UX tails. They depend on the now-stable step IR
  and should get their own plan (PLAN-wave5) rather than being folded in here.

---

## 6. Critical files

- `src/validate.rs`, `src/parser.rs`, `src/step_ir.rs`, `src/diagnostic.rs` (4.1)
- `src/callback.rs`, `src/main.rs` (4.2, 4.4, 4.6)
- `src/config.rs` (new), `Cargo.toml` (4.3, 4.4)
- `src/logging.rs` (new), `src/update.rs` (4.5)
- `tests/cli_smoke.rs`, `tests/validate_cli.rs` (integration coverage)

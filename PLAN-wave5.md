# Implementation Plan — Wave 5: typed parameters + inline assertions

> Source: TODO.md #30/#14 (typed parameters + profiles) and #31/#16 (inline
> assertions + `wb test`). These were the next *feature* wave that PLAN-wave4
> explicitly deferred ("depend on the now-stable step IR and should get their
> own plan (PLAN-wave5)"). Date drafted: 2026-06-25.

Both features are pure-Rust, zero new dependencies, and build on the existing
parser / step-IR / checkpoint machinery.

---

## 1. Typed parameters + profiles (#30/#14) ✅

New `src/params.rs` (self-contained, unit-tested).

- **Frontmatter**: `params: Option<HashMap<String, ParamSpec>>` and
  `profiles: Option<HashMap<String, HashMap<String, serde_yaml::Value>>>`.
  `ParamSpec` is an untagged enum: a full `ParamDef`
  (`type`/`default`/`required`/`one_of`/`secret`) or a scalar shorthand (becomes
  the default, type string). `validate.rs`'s `FrontmatterStrict` learned both
  keys so they aren't flagged `wb-fm-001`.
- **CLI**: `--param KEY=VALUE` (repeatable), `--param-file <yaml>`,
  `--profile <name>` on `RunArgs`, `BareRunArgs`, and `TestArgs`.
- **Resolution** (`params::resolve`): precedence `--param` > `--param-file` >
  `--profile` > declared `default`. Type validation (`int`/`bool`/`enum`),
  `one_of` membership, undeclared-key rejection, and missing-`required:`
  detection — all surfaced as run-start usage errors (exit 2).
- **Injection**: resolved values go into `ctx.env` under their bare name (so
  `$region` works and `{when=}`/`{skip_if=}` can branch on them); `secret: true`
  values are appended to `redact_values`. Injected at highest precedence over
  env/secrets/vars in `build_execution_context` (run) and via a new `extra_env`
  arg to `run_single_collect` (folder/test).
- **Checkpoint identity**: `Checkpoint.param_hash` (12-hex digest of the sorted
  resolved set) + `Checkpoint.params` (the values). `prepare_checkpoint` takes
  the current hash; a mismatch on resume starts fresh. The values are persisted
  so the two `wb resume` paths re-inject them as synthetic `--param` inputs
  (resume carries no param flags, and a required param has no default).
- **Static checks**: `check_params` → `wb-param-001` (unknown type, default
  type/`one_of` mismatch, enum-without-choices) and `wb-param-002` (profile
  references an undeclared param or a value violating its type/choices).

## 2. Inline assertions + `wb test` (#31/#16) ✅

New `src/assertion.rs` (DSL parse + evaluate, std-only) and a new
`parser::Section::Expect(ExpectSpec)` variant.

- **Parser**: an ` ```expect ` / ` ```assert ` fence is recognized in
  `extract_sections` and parsed eagerly into `Vec<(source, Assertion)>` + a list
  of malformed-line errors. It is non-executable and consumes no block index
  (the `Code | Browser` filters in `code_block_count`/`build_steps` ignore it).
- **DSL**: `exit <N>` / `exit != <N>`, `stdout|stderr contains|not-contains|
  equals <text>`, `stdout|stderr empty|not-empty`. Quoted args supported. No
  regex, no shell — intentionally tiny.
- **`wb test <file|dir>`**: new subcommand. Runs each workbook via
  `run_single_collect`, then re-walks the sections tracking the same block index
  the collect loop assigns, and evaluates each `expect` fence against the
  immediately preceding block's `BlockResult`. Text report + `--format json`
  (`{ok, passed, failed, files[]}`). Exit `0` all-pass, `1` any-fail/file-error,
  `2` no-assertions/usage. `--bail` stops a file at its first failure.
- **`wb run`**: leaves `expect` fences as no-ops (parsed + validated, not
  evaluated) — `wb test` is the evaluator. Markdown round-trip output re-emits
  the fence; `wb inspect --json` lists it as a non-indexed `expect` entry.
- **Static checks**: `check_expects` → `wb-expect-001` for each malformed line.

---

## 3. Verification

`cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test --all-targets --locked` (all module + integration tests green,
including new `params.rs`/`assertion.rs` unit tests and 5 new `cli_smoke.rs`
integration tests), and `cargo build --release --locked`. End-to-end smoke:
param injection/override/profile/param-file, required-param + bad-value usage
errors, checkpoint param-change→fresh, resume re-applying params, and
`wb test` pass/fail/json/dir/no-assertions paths. Docs updated in CLAUDE.md +
TODO.md; runnable `examples/params-demo.md` + `examples/test-demo.md`.

## 4. Critical files

- `src/params.rs` (new), `src/assertion.rs` (new)
- `src/parser.rs` (frontmatter fields, `ExpectSpec`, `Section::Expect`, fence parse)
- `src/main.rs` (CLI flags, `cmd_test`, param resolution + injection, checkpoint wiring)
- `src/checkpoint.rs` (`param_hash` + `params` fields)
- `src/validate.rs` (`check_params`, `check_expects`, strict-schema keys)
- `src/diagnostic.rs` (code registry), `src/output.rs` (`Section::Expect` arms)

## 2b. Follow-on increment — selection + dry-run

Two more self-contained, in-repo capabilities folded into the same PR:

- **`--tag <class>` selection (#33)** — runs only blocks whose step carries a
  matching fence `.class`. `resolve_tag_set` builds the index set; a new
  `block_selected(range, tags, idx)` ANDs the tag set with the existing
  `--from`/`--until` range (so they compose). Repeatable (union of tags),
  conflicts with `--only` at the clap layer, and a tag matching no block is a
  usage error. The source-hash cache (`--changed`/`--no-cache`, #18) is still
  open.
- **`wb run --dry-run` (#37)** — `dry_run_preview` resolves params, selection,
  conditionals, and per-step policy and prints each block as run/skip (with the
  reason + resolved command), then exits. No sandbox, no secrets, no setup, no
  block execution. Conditionals are evaluated against frontmatter env + vars +
  params only.

Both covered by new `cli_smoke.rs` integration tests (15 total in that file).
Signed/trusted workbooks, sandbox-by-default, and allowlists (#37 remainder) and
the cache (#18/#33) remain future waves.

## 2c. Follow-on increment — artifact manifest + commands (#36)

- `Artifacts::record(step_id, &records)` writes a per-run `manifest.json` into
  the artifacts dir: each entry carries filename, bytes, content type, sha256
  checksum, label/description (from the `.meta.json` sidecar), producing step
  id, and `updated_at`. Called at all four `sync()` sites (run live + browser,
  folder/test code + browser).
- New subcommands: `wb artifacts list|open|export` and `wb runs list|show`.
  Runs resolve under `~/.wb/runs/<id>/artifacts`; no `--run` targets the latest;
  manifest-less runs fall back to a directory scan. `--format json` on the read
  commands. Covered by a new `cli_smoke.rs` integration test.

## 2d. Follow-on increment — source-hash cache (#18, core)

- New `src/cache.rs`: `CacheStore` persisted at `~/.wb/cache/<id>.json`,
  `cache_key(language, body, param_hash)` (sha256, 16 hex). `--cache <id>`
  enables it; `--no-cache` disables; the `{no-cache}` fence flag opts a block
  out. In the live run loop, a block whose key matches a prior **success** is
  skipped (`step.skipped` kind `cache`); every executed block records its
  outcome, and the store is saved at run end.
- Limitation (documented): a cached block is skipped, not replayed (no output
  reproduction); env/secret identity, included-file hashes, and runtime
  versions are not yet in the key. Covered by `cache.rs` unit tests + a
  `cli_smoke.rs` integration test.

## 5. Deferred (next steps)

- #18/#33 source-hash execution cache (param hash is ready to feed cache keys).
- Include-level param passing (#30 tail).
- `wb test`: artifact/file assertions, browser selector assertions, JUnit /
  GitHub-annotation output.

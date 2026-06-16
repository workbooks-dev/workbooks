# TODO — wb improvements

Originally generated 2026-04-20 from a multi-agent audit. Waves 1 (data-loss fixes) and 2 (agent-UX wins) shipped — see git log for `1fe0323`, `f2e21b5`, `87d462a`, `09a8d79`, and the v0.9.7–v0.9.11 releases. This file is what's left.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[-]` dropped

---

## Top-tier CLI roadmap

These are the product-level gaps that would move `wb` from a strong internal tool
to a top-tier CLI for executable runbooks. Items that overlap the implementation
backlog below should be treated as the user-facing product shape for that work.

- [x] **29. Stable step IDs + full fence attrs.** Wave 4 shipped the parse-time + IR layer: Pandoc-style fence attrs (`{#id .class key=value}`), per-step `Vec<Step>` IR (`crate::step_ir`) with deterministic `auto-<hash>` ids, `step_id` on `step.complete` / `checkpoint.failed` / `step.artifact_saved` payloads + `wb inspect --json` blocks, fence-attr policy that overrides legacy block-number maps (with `wb-step-002` warning on shadowing), and `wb-step-001` duplicate-id detection in `wb validate`. Phase 1 shipped step_id dual-write: `Checkpoint.next_step_id` + `SavedResult.step_id` + `PendingDescriptor.next_step_id` are persisted alongside the existing `block_idx`-keyed state (legacy JSON still parses); the reaper's `checkpoint.failed` callback now carries `step_id` from the descriptor. Phase 2 shipped step-id-keyed resume: `prepare_checkpoint` resolves the resume position by locating `next_step_id` in the current workbook's step list, so a checkpoint survives upstream block insertions/deletions; `total_blocks` mismatch no longer invalidates v2 checkpoints; v1 (no step id) checkpoints still take the legacy block-count path; new `wb-resume-001` diagnostic when the saved step id is missing and we fall back to block_idx. Phase 3 shipped: `reap_expired` acquires the per-checkpoint file lock before mutating ckpt state, skipping (not blocking) when a live `wb run` holds it. Phase 4 shipped step ids as the substrate for selective runs: `--only <step-id>`, `--from <step-id>`, `--until <step-id>` (clap rejects `--only` combined with the others; unknown ids fail before any block runs; selective runs are ephemeral and refused with `--checkpoint`). The cache/`--tag`/`--changed` story stays in #33.
- [ ] **30. Typed parameters + profiles.** Build on #14 with frontmatter-declared params, required/default/type validation, `--param`, `--param-file`, secret params, named profiles (dev/staging/prod), and include-level param passing. Parameter hashes should feed checkpoint and cache identity.
- [ ] **31. Inline assertions + `wb test`.** Build on #16 with `expect`/`assert` fences plus a test-oriented command mode: JSON predicates, stdout/stderr matching, exit-code assertions, artifact/file assertions, browser selector assertions, and CI outputs such as JUnit/GitHub annotations.
- [~] **32. `wb validate` + `wb doctor`.** Wave 3 shipped `wb validate` with structured diagnostics, frontmatter schema/type checks, malformed YAML line/column spans, bad durations, missing/cyclic includes, bad secret provider names, text/JSON output, and strict mode; `wb doctor` shipped shallow runtime checks plus deep Docker/sidecar/Redis probes. Remaining: unknown fence attrs and duplicate step IDs after step IR lands, plus broader callback config checks.
- [ ] **33. Selective and cached execution.** Pair #18 with explicit selection flags: `--only <step-id>`, `--from`, `--until`, `--tag`, `--changed`, and `--no-cache`. Cache keys should include source, params, env/secrets identity, included files, artifact inputs, and relevant runtime versions.
- [ ] **34. Browser reliability pack.** Treat #19-23 as one product milestone: network-idle waits, text fallback selectors, auto-screenshot plus console buffer on verb failure, local browser mode, and structured sidecar error codes.
- [ ] **35. Live local run viewer.** Add `wb watch` / `wb ui` for local runs: include tree, live stdout/stderr, pending waits, browser screenshots, artifacts, checkpoint state, retry/resume affordances, and callback-event inspection without needing a separate operator UI.
- [ ] **36. Artifact manifest + artifact commands.** Extend artifact capture with a manifest keyed by run/step, labels/descriptions/checksums/content types, and commands like `wb artifacts list`, `wb artifacts open`, `wb artifacts export`, and `wb runs show <id>`.
- [ ] **37. Trust, policy, and dry-run model.** Since workbooks execute arbitrary markdown code, add signed/trusted workbooks, `--dry-run` command preview, sandbox-by-default mode for untrusted sources, command/network/file allowlists, and explicit secret exposure policy.
- [~] **38. First-class CLI UX.** Wave 3 replaced manual command interception with real clap subcommands for run/inspect/validate/doctor/pending/resume/cancel/containers/update/version/transform, reserved hidden completion/man placeholders, and documented stable exit codes. Remaining: real shell completion and man-page generation, `wb config`, structured logging flags, and consistent JSON output for every management command.

Suggested product sequencing after the 2026-04-29 multi-agent battle test:

1. **CI and command foundation:** Add PR/push CI, then move off manual command interception (#38) so new commands can return structured reports and exit codes instead of calling `process::exit` from deep paths.
2. **Structured diagnostics:** Build a shared diagnostic model and ship `wb validate` (#32) for deterministic workbook checks: frontmatter schema, YAML line/column errors, unknown attrs, bad durations, missing/cyclic includes, and stable machine output.
3. **Environment health checks:** Ship `wb doctor` (#32) separately from validate. Keep it shallow by default; put network probes, Docker builds, sidecar handshakes, Redis checks, and other side effects behind an explicit deep mode.
4. **Step identity and fence attrs:** Implement #29/#13 as a real step model: stable IDs, shared attrs, source spans, include call-site identity, duplicate detection, and compatibility for existing block-number maps.
5. **State correctness blockers:** Fix checkpoint/pending locking, persist run IDs and callback config across resume/reap, add event sequence/idempotency fields, and close the current callback/order gaps before building viewer/cache features.
6. **Typed params and profiles:** Implement #30/#14 after stable step identity. Parameter hashes should feed checkpoint identity, cache identity, and include-level passing.
7. **Browser reliability:** Ship #34/#19-23 in reliability order: runtime capability negotiation + doctor checks, structured sidecar errors, automatic failure screenshots/console buffers, explicit network-idle waits, constrained text fallback, then explicit local browser mode.
8. **Selective execution first, cache later:** Split #33/#18. Ship `--only`, `--from`, `--until`, and `--tag` after step IDs; defer transparent cache until cacheable/pure steps, params, secrets identity, artifacts, and runtime versions are modeled.
9. **Artifact manifest and commands:** Build #36 on top of the already-shipped artifact events/labels. Persist checksums, source step, upload status, content type, and labels before adding `wb artifacts ...` / `wb runs ...`.
10. **Small `wb test`:** Ship #31/#16 after validate and stable IDs. Start with exit/stdout/stderr/file/artifact assertions; defer browser selector assertions, JUnit, and GitHub annotations until diagnostics and line mapping are solid.
11. **Trust and policy before public sharing:** Move #37 earlier than "operator polish" for any public/third-party story. Start with dry-run and explicit trust/policy gates; do not market current `requires:` containers as a security sandbox.

---

## 🧠 Strategic bets

- [x] **13. Pandoc-style fence attrs** — ``` ```python {#step-3 .retryable timeout=30s} ```. Shipped in wave 4 alongside #29. `parse_info_string` accepts `{#id}`, `{.class}`, `key=value`, and bare flags (`no-run`, `silent`, `continue_on_error`). Attrs land on `CodeBlock::attrs` / `BrowserSliceSpec::attrs` as a `step_ir::FenceAttrs`. Unknown bare attrs are still ignored for forward-compat; `wb-attr-001` stays reserved until the vocabulary is closed.
- [ ] **14. Parameterized runs.** `wb run deploy.md --param region=us-east-1`. Frontmatter declares defaults + types. Param hash feeds into checkpoint identity.
- [ ] **16. Inline `expect` / `assert` fences.** Turn runbooks into test suites. `expect exit 0`, `expect stdout contains "ok"`.
- [ ] **18. Source-hash execution cache.** Skip blocks whose source + env + inputs haven't changed. Massive for iterative agent re-runs.
- [x] **27. `include:` fence — workbook composition.** Shipped — `Section::Include` + parse-time expansion via `parser::resolve_includes`. Target workbook's blocks splice into the parent's section list, inheriting env + `$WB_ARTIFACTS_DIR`. Cycle detection + missing-file errors exit with code 3 at load time. Target frontmatter is ignored (parent controls runtime/secrets/env). Params still scoped for #14. Example: `examples/include-demo.md` + `examples/include-login.md`.
- [x] **28. `required:` frontmatter — declarative prerequisites.** Shipped — `Frontmatter::required: Option<Vec<String>>` synthesized into `Section::Include` entries at position 0 in `resolve_includes`. Reuses the include pipeline (cycle detection, path resolution, IncludeEnter/Exit sentinels). Inner workbooks' `required:` is intentionally not recursive (mirrors include's "target frontmatter ignored" contract). Errors say `required 'login.md': ...` rather than `include at L0: ...` via a new `IncludeOrigin` enum. `wb validate` understands the field. Example: `examples/required-demo.md`.

## 🌐 Browser runtime

- [ ] **19. `wait_for_network_idle` verb.** Every SPA workbook is fragile without this.
- [ ] **20. Text-fallback selectors.** `click: { selector, text_fallback: "Send" }`. One change, ~half fewer brittle workbooks.
- [ ] **21. Auto-screenshot + console buffer on verb failure.** Current failures = single line of stderr; post-hoc debugging is impossible.
- [x] **22. `WB_BROWSER_VENDOR=local` provider.** Shipped — third provider alongside browserbase/browser-use that drives a host-installed Playwright Chromium directly via `chromium.launch()`. No API keys, no network calls, no per-session cost. The provider returns a pre-built `Browser` handle in `_browser`; the sidecar entry point uses it directly instead of `connectOverCDP(cdpUrl)`. Trade-offs documented: no live URL, no persistent profile, no resume-after-pause (in-process browser dies with the sidecar). Knobs: `WB_BROWSER_LOCAL_HEADLESS` (default 1), `WB_BROWSER_LOCAL_EXECUTABLE_PATH`, `WB_BROWSER_LOCAL_CHANNEL`. First-run hint points at `npx playwright install chromium`.
- [ ] **23. Structured error codes in sidecar events.** `SELECTOR_NOT_FOUND`, `NAV_TIMEOUT`, `AUTH_FAILED` — not freeform strings.

## 🧹 Code-health (do alongside, not instead)

- [x] **24. Extract `run_single()`.** Shipped — 19-param signature replaced with a `RunConfig` struct; sandbox re-entry, execution-context build, checkpoint lock+load, callback resolution, and output writing all extracted into private helpers. `run_single` went from ~814 lines to ~559; main execution loop deliberately left intact (it's the state machine and will absorb fence-attr changes cleanly now that setup/teardown are out of the way). Side cleanup: fixed flaky `test_reap_expired_returns_entry_fields` that surfaced under increased test-parallelism pressure — root cause is `reap_expired` not locking the shared ckpt dir, papered over in the test by asserting the stronger on-disk post-condition instead of per-call provenance.
- [ ] **25. Unified error type** (thiserror) — before more structured-error work multiplies the current `anyhow`/`String`/`unwrap` mix.
- [ ] **26. Type the sidecar↔checkpoint↔pending state** instead of opaque `serde_yaml::Value` — will otherwise become a scavenger hunt once browser recording metadata needs to survive pause/resume.

## Open follow-ups from shipped work

- [x] Line/column for malformed frontmatter YAML parse errors (follow-up to #11). Shipped via `wb validate` diagnostic spans (`wb-yaml-001`).
- [x] Pending-wait descriptors should persist the original run's `--callback` URL so timeout reaping can emit `checkpoint.failed` callbacks (follow-up to #17). Shipped — `PendingDescriptor.callback_url` + `callback_secret` round-trip through save/load, and `reap_expired` fires `checkpoint.failed` against the original endpoint with HMAC signing.
- [ ] HTTP callback ordering guarantees (currently best-effort; Redis XADD side already orders — follow-up to #12).
- [x] `reap_expired` should acquire the per-ckpt file lock before mutating — currently uses a sibling reap lock that serializes reapers against each other but not against a live `wb run` (follow-up to #17 / surfaced during #24). Shipped as Phase 3 of the #29 work: reaper now `try_lock_for`s the checkpoint path inside `with_pending_lock`, skips the descriptor (non-blocking) on contention, and releases before firing callbacks so HTTP doesn't hold disk locks.

## Runbook-library features (formerly `features-request.md` F1–F7)

These were the longer-form Xatabase run-page specs. F1–F6 shipped across v0.11–v0.14; F7 is the one open item and its spec stays in `features-request.md`.

- [x] **F1. `step.artifact_saved` callback** — bash/python/sandbox cells that write into `$WB_ARTIFACTS_DIR` now emit a `step.artifact_saved` event (mirrors browser `slice.artifact_saved`), fired by `sync()` before `step.complete`, include-scoped, excluding wb-internal sidecars. Shipped v0.11.0 (`c8ffd4e`); wired in `callback.rs` + `main.rs` run/sandbox paths.
- [x] **F2. Artifact labels + `announce_artifact:` verb** — `<file>.meta.json` sidecar convention (`label`/`description`) read fresh per sync and attached to `step.artifact_saved`; `announce_artifact:` browser verb writes the sidecar via the same `resolveInside` guard as `save:`. Shipped wb v0.13.2 (`bec2216`) + browser runtime v0.9.0.
- [x] **F3. Structured step outputs** — `output: name=value` / `output-json: name=<json>` capture (`step_outputs.rs`), surfaced on `step.complete`, persisted in the checkpoint, and written to `$WB_OUTPUTS_PATH` (`$WB_ARTIFACTS_DIR/.wb/outputs.json`). Capture lines are stripped from terminal rendering; `{silent}` keeps dataflow but drops callback emission. Shipped v0.13.2 (`bec2216`).
- [x] **F4. `step.skipped` events** — `{no-run}` / `when=` / `skip_if=` / selection (`--from`/`--until`/`--only`) skips emit a `step.skipped` callback with skip kind/expression/reason; progress advances for skipped executable steps. Shipped v0.13.2 (`bec2216`); wired at `main.rs:2445/2708`.
- [x] **F5. Workflow metadata manifest** — opaque `workflow:` frontmatter (slug/version/nodes) retained, persisted in the checkpoint, and emitted as compact `workflow` + `workflow_node` fragments on `step.*` / `checkpoint.failed` / pause callbacks when a block's `step_id` matches a declared node. Shipped v0.13.2 (`bec2216`).
- [x] **F6. Default block timeout is opt-in** — removed the silent 300s `DEFAULT_BLOCK_TIMEOUT`; blocks run unbounded unless capped via fence attr, `timeouts._default`, or `--default-block-timeout`. Timeout errors name the block + source knob. Shipped v0.14.0 (`22ed8a8`).
- [x] **F7. Operator-driven in-flight cell control** — shipped in two parts. **F7a** (conditional pauses): step outputs now export into the session eval env as `$WB_OUT_<name>` (`step_outputs::export_to_session`), so a later cell's `{when=...}` / `{skip_if=...}` can gate a pause on a value an earlier step produced (fence-level `{when=}` on browser slices already worked); demo `examples/conditional-pause-demo.md`. **F7b** (navigation actions): `wb resume --rerun-step [id]` / `--goto-step <id>` and the equivalent resume-signal `{"action":{...}}` let an operator re-run the current/earlier step or skip ahead by `step_id` at a `pause_for_human` — overrides the resume cursor (`ckpt.next_step_id`/`next_block`), suppresses browser sidecar restore so the target slice runs fresh from verb 0, truncates stale results on a backward jump, and emits `step.skipped` (kind `goto`) for forward-skipped steps. Pause-emit validation rejects action targets that don't resolve to a step id. Per-*verb* conditionals (vs fence-level) remain out of scope (JS sidecar repo).

---

## Sequencing

- **Now - command and CI foundation:** Add normal PR/push CI; replace manual command interception with real clap subcommands (#38); normalize exit-code use and command return paths.
- **Next - diagnostics:** Add a shared diagnostic type, then split `wb validate` and `wb doctor` out of #32. `validate` is deterministic workbook analysis; `doctor` is environment/runtime health, shallow by default.
- **Then - step identity:** Implement #13/#29 as a shared step IR rather than another parser field: stable IDs, attrs, spans, include call-site IDs, duplicate checks, and compatibility with existing `timeouts` / `retries` / `continue_on_error` maps.
- **Before new stateful features:** Fix state correctness follow-ups: checkpoint/pending locking, persisted callback URL + run ID, HTTP callback ordering/idempotency, and event sequencing.
- **Then - reuse:** Implement #14/#30 typed params and include-level param passing. Feed resolved param/profile hashes into checkpoint/cache identity.
- **Browser track:** Do #23 and #21 first, plus capability negotiation and `doctor browser`; then #19, #20, and explicit #22 local mode. Avoid blind text fallback and silent local fallback.
- **Selective execution:** Split #18/#33. Selection flags can ship after stable IDs; automatic cache waits for explicit cacheability/dependency semantics.
- **Artifacts:** #36 should persist a manifest before adding commands. Also make sure nested artifact paths are captured before relying on screenshots/failure artifacts.
- **Testing:** #16/#31 comes after validate + stable IDs. Start small with exit/stdout/stderr/file assertions; CI adapters and browser assertions are later.
- **Trust and public growth:** #37 blocks public registry/gallery, remote URL execution, hosted runs, browser-profile automation for third-party workbooks, and shared cache. Current containers are dependency containers until hardened.
- **Long-term:** #28 (`required:` sugar) after includes + params have baked; #35 full local UI after stable events, manifests, and step IDs. A thin `wb watch --events jsonl` can ship earlier if useful.

## Notes

- `features-request.md` at repo root holds longer-form specs for fence-flags and browser recording — keep as canonical reference, this file is the checklist.

# TODO — wb improvements

Generated 2026-04-20 from a multi-agent audit. Items are grouped by theme, not priority within a theme. See the "Sequencing" section at the bottom for recommended order.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[-]` dropped

---

## 🚨 Fix first — silent data-loss risks

- [~] **1. Atomic checkpoint + pending writes.** `src/checkpoint.rs:153-160` and `src/pending.rs:46-53` use `fs::write()` directly. SIGKILL mid-write corrupts JSON. Fix: write to `.tmp` + `rename()`, set `0o600` perms on Unix.
- [~] **2. Remove `checkpoint_id.as_ref().unwrap()` panics.** `src/main.rs:1259, 1268, 1403, 1411, 1434`. Invariant holds today but is implicit. Combine the two `Option` matches.
- [~] **3. File lock on concurrent checkpoint writes.** Two `wb run --checkpoint same-id` runs silently clobber each other. Add `flock()` in `checkpoint::save()`.
- [~] **4. Reserved-name blocklist for bound_vars.** A `wait` block can `bind: PATH` (or `HOME`, `WB_*`) and replace env vars on resume. Reject at parse time with a clear error.
- [~] **5. Verify signal-validation ordering.** Agent claimed malformed signal JSON wedges resume by mutating state before validation. `src/main.rs:2569-2575` looks OK — confirm fully and plug any leaks.

## 🎯 Highest agent-leverage UX wins

- [ ] **6. Structured error types in JSON output.** Add `error: { type, message, line, column }` to each block result in `src/output.rs`. Agents stop regex-parsing stderr.
- [ ] **7. Real exit-code vocabulary.** Today it's `0 | 1 | 42`. Agents can't distinguish block failure vs parse error vs sidecar crash. Define the table, document it, update `src/main.rs` dispatch.
- [ ] **8. `wb inspect --json`.** Emit `[{index, language, line, heading, flags}]` instead of human prose. `src/main.rs` inspect command.
- [ ] **9. Trace-correlation field.** Propagate `TRIGGER_RUN_ID` / generated ULID into every callback payload, result file header, artifact upload. Single join key across a run.
- [ ] **10. Partial output capture on timeout/SIGKILL.** Ring-buffer stdout so killed blocks still report what they emitted, with `stdout_partial: true` flag.
- [ ] **11. Line+column + "did-you-mean" on parse/runtime errors.** Bad YAML, unknown runtime, typo'd flag — each is an agent retry loop today.
- [ ] **12. Callback `event_version` + retries/ordering.** Today it's fire-and-forget `curl`. Version the schema, queue in-order, retry 5xx.

## 🧠 Strategic bets

- [ ] **13. Pandoc-style fence attrs** — ``` ```python {#step-3 .retryable timeout=30s} ```. Canonical home for ALL future per-block config. Do this early — items 6/7/14/15 slot in cheaply afterward.
- [ ] **14. Parameterized runs.** `wb run deploy.md --param region=us-east-1`. Frontmatter declares defaults + types. Param hash feeds into checkpoint identity.
- [ ] **15. Per-block `timeout`, `retry`, `continue-on-error`.** Agents currently wrap every block in shell conditionals for this.
- [ ] **16. Inline `expect` / `assert` fences.** Turn runbooks into test suites. `expect exit 0`, `expect stdout contains "ok"`.
- [ ] **17. Pending-wait timeout reaper.** `on_timeout: abort` never fires until a human manually resumes. Background reaper (or auto-fire on next `wb pending`).
- [ ] **18. Source-hash execution cache.** Skip blocks whose source + env + inputs haven't changed. Massive for iterative agent re-runs.

## 🌐 Browser runtime

- [ ] **19. `wait_for_network_idle` verb.** Every SPA workbook is fragile without this.
- [ ] **20. Text-fallback selectors.** `click: { selector, text_fallback: "Send" }`. One change, ~half fewer brittle workbooks.
- [ ] **21. Auto-screenshot + console buffer on verb failure.** Current failures = single line of stderr; post-hoc debugging is impossible.
- [ ] **22. `WB_BROWSER_MODE=local` fallback.** Dev iteration without Browserbase cost/latency.
- [ ] **23. Structured error codes in sidecar events.** `SELECTOR_NOT_FOUND`, `NAV_TIMEOUT`, `AUTH_FAILED` — not freeform strings.

## 🧹 Code-health (do alongside, not instead)

- [ ] **24. Extract `run_single()`** (`src/main.rs:761-1455` — 700 lines, 13 params). Will become painful as 6/7/13/14/15 all touch execution dispatch.
- [ ] **25. Unified error type** (thiserror) — before all the new structured-error work multiplies the current `anyhow`/`String`/`unwrap` mix.
- [ ] **26. Type the sidecar↔checkpoint↔pending state** instead of opaque `serde_yaml::Value` — will otherwise become a scavenger hunt once browser recording metadata needs to survive pause/resume.

---

## Sequencing

- **Wave 1 — foundation (this session):** 1, 2, 3, 4, 5. All small, independent, no structural changes.
- **Wave 2 — agent-UX wins:** 6, 7, 8, 9 (then 10, 11, 12 as capacity allows).
- **Wave 3 — fence-attr foundation:** 13 first, then 6/7 refinements.
- **Wave 4 — power-ups:** 14 (params), 15 (retry/timeout), 16 (assertions).
- **Parallel track — browser:** 19-23 in any order; independent of core.
- **Health track:** 24-26 interleave with whichever wave touches the same files.

## Notes

- Silent/no-run flags were flagged as half-implemented by one audit; verified **fully wired** in `src/parser.rs` + `src/main.rs:1234, 1375`. No action needed.
- SIGKILL-on-shutdown race in sidecar was already fixed in commit `fee72a3`.
- `features-request.md` at repo root holds longer-form specs for fence-flags and browser recording — keep as canonical reference, this file is the checklist.

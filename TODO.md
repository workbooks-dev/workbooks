# TODO — wb improvements

Originally generated 2026-04-20 from a multi-agent audit. Waves 1 (data-loss fixes) and 2 (agent-UX wins) shipped — see git log for `1fe0323`, `f2e21b5`, `87d462a`, `09a8d79`, and the v0.9.7–v0.9.11 releases. This file is what's left.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[-]` dropped

---

## 🧠 Strategic bets

- [ ] **13. Pandoc-style fence attrs** — ``` ```python {#step-3 .retryable timeout=30s} ```. Canonical home for ALL future per-block config. Do this early — #14 and #16 slot in cheaply afterward, and #15's block-number-keyed maps can grow to accept attribute ids.
- [ ] **14. Parameterized runs.** `wb run deploy.md --param region=us-east-1`. Frontmatter declares defaults + types. Param hash feeds into checkpoint identity.
- [ ] **16. Inline `expect` / `assert` fences.** Turn runbooks into test suites. `expect exit 0`, `expect stdout contains "ok"`.
- [ ] **18. Source-hash execution cache.** Skip blocks whose source + env + inputs haven't changed. Massive for iterative agent re-runs.
- [x] **27. `include:` fence — workbook composition.** Shipped — `Section::Include` + parse-time expansion via `parser::resolve_includes`. Target workbook's blocks splice into the parent's section list, inheriting env + `$WB_ARTIFACTS_DIR`. Cycle detection + missing-file errors exit with code 3 at load time. Target frontmatter is ignored (parent controls runtime/secrets/env). Params still scoped for #14. Example: `examples/include-demo.md` + `examples/include-login.md`.
- [ ] **28. `required:` frontmatter — declarative prerequisites** *(long-term; depends on #27)*. Sugar for "prepend these workbooks as `include` blocks at position 0; bail if any fail." Shape mirrors GitHub Actions `needs:`. Example: `required: [login.md, warm-cache.md]`. Same execution path as #27 — different ergonomics (order-independent, declarative vs positional fence).

## 🌐 Browser runtime

- [ ] **19. `wait_for_network_idle` verb.** Every SPA workbook is fragile without this.
- [ ] **20. Text-fallback selectors.** `click: { selector, text_fallback: "Send" }`. One change, ~half fewer brittle workbooks.
- [ ] **21. Auto-screenshot + console buffer on verb failure.** Current failures = single line of stderr; post-hoc debugging is impossible.
- [ ] **22. `WB_BROWSER_MODE=local` fallback.** Dev iteration without Browserbase cost/latency.
- [ ] **23. Structured error codes in sidecar events.** `SELECTOR_NOT_FOUND`, `NAV_TIMEOUT`, `AUTH_FAILED` — not freeform strings.

## 🧹 Code-health (do alongside, not instead)

- [ ] **24. Extract `run_single()`** (`src/main.rs:761-1455` — 700 lines, 13 params). Will become painful as #13/14 touch execution dispatch. Strongly consider doing this *before* #13.
- [ ] **25. Unified error type** (thiserror) — before more structured-error work multiplies the current `anyhow`/`String`/`unwrap` mix.
- [ ] **26. Type the sidecar↔checkpoint↔pending state** instead of opaque `serde_yaml::Value` — will otherwise become a scavenger hunt once browser recording metadata needs to survive pause/resume.

## Open follow-ups from shipped work

- [ ] Line/column for malformed frontmatter YAML parse errors (follow-up to #11).
- [ ] Pending-wait descriptors should persist the original run's `--callback` URL so timeout reaping can emit `checkpoint.failed` callbacks (follow-up to #17).
- [ ] HTTP callback ordering guarantees (currently best-effort; Redis XADD side already orders — follow-up to #12).

---

## Sequencing

- **Next — fence-attr foundation:** #24 (extract `run_single`) → #13 (fence attrs). Do #24 before #13 because #13+#14+#15 together will make dispatch untenable; doing #24 while it's still 700 lines is cheaper than after three more additions.
- **Then — power-ups:** #14 (params — lets includes pass values, not just env), #16 (assertions). Both want fence attrs as substrate.
- **Parallel track — browser:** #19-23 in any order; independent of core.
- **Health track:** #25, #26 interleave with whichever change touches the same files.
- **Long-term:** #28 (`required:` sugar) after #27 has baked. #18 (execution cache) once fence attrs give us stable block identity — pairs well with #27 to cache login-style includes.

## Notes

- `features-request.md` at repo root holds longer-form specs for fence-flags and browser recording — keep as canonical reference, this file is the checklist.

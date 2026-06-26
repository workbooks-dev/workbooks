# IDEA_GOALS.md — session goals to implement the 10–100x bets

Copy-paste `/goal` commands, in the order I'd implement them. Each is scoped to be
**one session = one bet = one branch**, not "all of them in one goal" (a single
Stop-hook that only releases when all 10 ship would run unbounded and couple
unrelated work). Full rationale in `docs/enhancement-ideas.md`; dependency-ordered
plan in `PLAN-wave5-bets.md`.

## Before you start

- **The doc + plan live on branch `enhancement-ideas`** (not yet merged to `main`,
  to avoid colliding with the active session there). Either:
  - work each bet in this worktree / off this branch, **or**
  - merge `enhancement-ideas` → `main` first (clean fast-forward — do it once the
    other session pauses), then start fresh sessions on `main`.
- **`#37` (trust/policy/sandbox) is a hard gate** for bets #40, #42, and the
  sandbox part of #45. Don't start those until #37 lands.
- Three tracks can run in parallel without colliding: **agent** (#39→#42),
  **adoption** (#43→#41→#40), **foundation** (#37→#45/#46/#47).

## How to use each goal

Open a new session in the repo and paste one line. `/goal` installs a
session-scoped Stop hook that won't let the session end until the condition holds,
then starts working immediately.

---

### 1. #39 — `wb mcp` (start here: top reach-per-effort, no gate)

```
/goal Implement TODO #39 (wb mcp) per PLAN-wave5-bets.md Phase A: add src/mcp.rs exposing run/inspect/resume/list_pending/get_run_events as MCP tools over stdio, mapping checkpoint+pending onto MCP Tasks and pause_for_human onto elicitation, as a thin adapter over existing command internals. cargo fmt+clippy+test+release-build green; CLAUDE.md, TODO.md (#39→done), PLAN-wave5-bets.md updated. Verify an MCP client can author→run→pause→resume→read-results end-to-end.
```

### 2. #31 → #43 — `wb test` then docs-as-tests + GitHub Action

```
/goal Implement TODO #31 (wb test: expect/assert fences, exit/stdout/stderr/file/artifact assertions) then TODO #43 (wb verify for ordinary docs + a workbooks/verify-action GitHub Action with JUnit/annotation output), per PLAN-wave5-bets.md Phase A. cargo fmt+clippy+test+release-build green; docs + TODO.md updated. Verify a sample repo README is CI-checked and intentional drift fails the build.
```

### 3. #37 — trust / policy / sandbox-by-default (the gate; run in parallel with 1–2)

```
/goal Implement TODO #37 per PLAN-wave5-bets.md Phase B: --dry-run command preview, signed/trusted workbooks (wb trust add + verification), sandbox-by-default for untrusted sources (seccomp/landlock on Linux) with command/network/file allowlists and explicit secret-exposure policy. cargo fmt+clippy+test+release-build green; CLAUDE.md + TODO.md updated. Do not market requires: containers as a security sandbox until this lands.
```

### 4. #41 — `wb capture` (independent; feeds the registry)

```
/goal Implement TODO #41 (wb capture) per PLAN-wave5-bets.md Phase D: record a shell session via PTY and a browser session via the existing recording machinery, emit a draft MANIFEST.md-compatible workbook with steps/includes/artifact captures wired, with secret scrubbing. cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

### 5. #44 — `wb watch` (local viewer first)

```
/goal Implement TODO #44 part 1 (wb watch) per PLAN-wave5-bets.md Phase D: a local live viewer over the JSONL event stream showing include tree, live stdout/stderr, pending waits, browser screenshots, artifacts, and resume affordances. Defer hosted run pages. cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

### 6. #40 — registry / remote execution (AFTER #37)

```
/goal Implement TODO #40 per PLAN-wave5-bets.md Phase C (requires #37 landed): remote workbook refs (gh:/https:/wb:), signing + trust verification, and wb publish, with sandbox-by-default for untrusted sources. cargo fmt+clippy+test+release-build green; docs + TODO.md updated. The hosted gallery on workbooks.dev is a separate product, out of scope for this binary work.
```

### 7. #42 — `wb run --repair` self-healing (AFTER #37; pairs with #39)

```
/goal Implement TODO #42 per PLAN-wave5-bets.md Phase C (requires #37 landed): wb run --repair posts a failed block (stderr/exit/partial output/step context) to an agent endpoint and applies a structured {rerun|patch|skip|abort} response, gated behind trust + allowlists + dry-run, with a bounded retry loop. cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

### 8. #45 — native `sql` + `http` runtimes (sandbox part AFTER #37)

```
/goal Implement TODO #45 per PLAN-wave5-bets.md Phase C: an http runtime (declarative method/url/headers/body + status/json assertions, no heavy deps) and a sql runtime behind a feature-gated wb-full build so the default binary stays zero-dep ~650KB. cargo fmt+clippy+test+release-build green for both build variants; docs + TODO.md updated.
```

### 9. #46 — content-addressed execution cache (AFTER #30 params)

```
/goal Implement TODO #46/#18 per PLAN-wave5-bets.md Phase D (requires #30 params): inputs/outputs block declarations + content-addressed skip, with cache identity = source+params+env/secret identity+included files+runtime versions, and --no-cache. cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

### 10. #47 — `wb.lock` + signed run attestations (AFTER #40 signing)

```
/goal Implement TODO #47 per PLAN-wave5-bets.md Phase D (reuses #40 signing): a wb.lock capturing resolved runtime versions/image digests/included-file hashes/param values, and signed run attestations (verifiable run receipts). cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

### 11. #48 — `wb-core` crate + WASM target

```
/goal Implement TODO #48 per PLAN-wave5-bets.md Phase D: extract parser+executor into a wb-core crate and add a feature-gated WASM build that supports parse/validate/inspect plus a pure-runtime, with subprocess execution gated out of the WASM target. cargo fmt+clippy+test+release-build green; docs + TODO.md updated.
```

---

## Tracking

As each lands, flip its `[ ]` → `[x]` in `TODO.md` (#39–#48) and in
`PLAN-wave5-bets.md`. Keep `docs/enhancement-ideas.md` as the canonical rationale.

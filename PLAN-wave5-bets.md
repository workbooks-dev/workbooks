# Implementation Plan — Wave 5: the 10–100x strategic bets

> Source: `docs/enhancement-ideas.md` (TODO #39–#48). Date drafted: 2026-06-25.
>
> Wave 4 closed the CLI-UX/diagnostics foundation. This wave is *not* incremental
> feature work — it is the reach play: plug the existing durable-execution engine
> into the agent ecosystem, a sharing network, and a wider set of jobs. Each bet
> is scoped to ship as its own releasable unit (one session, one branch, one
> `/goal` from `IDEA_GOALS.md`).
>
> **Governing constraint:** #37 (trust/policy/sandbox/dry-run) is the *gate* for
> the three "open it up to untrusted code" bets (#40 registry, #42 repair, #45
> sql/http sandbox). It is a prerequisite, not polish — sequence it early.

## 1. Dependency graph

```
                 ┌─────────────────────────────────────────────┐
   #39 wb mcp ───┤ reuses ckpt/pause/callback — NO new deps     │  ← start here
                 └─────────────────────────────────────────────┘
   #31 wb test ──► #43 docs-as-tests + Action      (independent track)
                              │
   #37 trust/policy ──┬──► #40 registry / remote run
   (the gate)         ├──► #42 self-healing --repair      (also wants #39)
                      └──► #45 sql/http sandbox piece
   #30 params ──────────► #46 execution cache
   events (shipped) ───► #44 wb watch ──► hosted run pages
   #41 wb capture  (independent; feeds #40)
   signing (from #40) ─► #47 lockfile + attestations
   refactor ───────────► #48 wb-core crate + WASM
```

Three parallel tracks can run at once without colliding:
- **Agent track:** #39 → #42 (after #37)
- **Adoption track:** #43 → #41 → #40 (after #37)
- **Foundation track:** #37 (the gate) → #45/#46/#47

## 2. Phasing

### Phase A — reach with zero new risk (do first, parallelizable)

**#39 `wb mcp`** — *top pick, no gate.*
- New `src/mcp.rs`: JSON-RPC over stdio (HTTP/SSE later). New `Command::Mcp`.
- Tools: `author_workbook`, `run_workbook`, `inspect`, `validate`, `resume`,
  `list_pending`, `get_run_events`.
- Map state onto MCP primitives: checkpoint/pending → **Tasks** lifecycle;
  `pause_for_human` → **elicitation**; `step.*` callbacks → progress
  notifications; `--json`/`$WB_OUTPUTS_PATH` → structured results.
- Keep it a *thin adapter* over the structured command internals exposed in #38 —
  spec churn must not bleed into core. Pin a stable Tasks subset.
- Critical files: `src/mcp.rs` (new), `src/main.rs` (dispatch), reuse
  `checkpoint.rs`/`pending.rs`/`callback.rs`/`step_outputs.rs`.
- Verify: an MCP client (Claude/inspector) can author→run→pause→resume→read
  results end-to-end. `cargo test` + clippy green.

**#43 docs-as-tests + Action** — *needs #31 first.*
- Land #31 (`wb test` / `expect`/`assert` fences) per existing TODO sequencing.
- Add `wb verify <file>` posture for ordinary docs (run blocks, assert exit 0 +
  output match); GitHub-annotation + JUnit output.
- Ship `workbooks/verify-action` (composite action wrapping the single binary).
- Critical files: `src/validate.rs`/new `src/test.rs`, `.github/` action, README.
- Verify: a sample repo's README is CI-checked; intentional drift fails the build.

### Phase B — the gate (start in parallel with A)

**#37 trust / policy / dry-run / sandbox-by-default.**
- `--dry-run` command preview (no execution).
- Signed/trusted workbooks: `wb trust add <pattern>`, trust store, minisign/
  sigstore-style verification.
- Sandbox-by-default for untrusted sources (seccomp/landlock on Linux; document
  macOS limits). Command/network/file allowlists. Explicit secret-exposure policy.
- This unblocks #40, #42, #45. Do **not** market `requires:` containers as a
  security sandbox until this lands.
- Critical files: new `src/trust.rs`, `src/sandbox.rs`, `src/main.rs`.

### Phase C — open it up (after #37)

**#40 registry / remote execution** — remote refs (`gh:`, `https:`, `wb:`) +
signing + `wb publish`; hosted gallery on workbooks.dev (separate product).
**#42 `wb run --repair`** — failure → agent endpoint → structured action; bounded
retry loop; gated behind trust + allowlists + dry-run. Trivial agent side via #39.
**#45 `sql` + `http` runtimes** — `http` first (assertion grammar, no heavy deps);
`sql` behind a feature-gated `wb-full` build to protect the zero-dep core binary.

### Phase D — depth & durability (second wave)

**#41 `wb capture`** (can start anytime; independent) — PTY shell record + browser
recording reuse; secret scrubbing; MANIFEST.md-compatible draft output.
**#44 `wb watch`** — local viewer first (TUI or local web over the JSONL event
stream), hosted run pages later.
**#46 execution cache** (needs #30 params) — inputs/outputs graph + content-
addressed skip. Cache identity = source+params+env/secret id+includes+runtime ver.
**#47 lockfile + attestations** (needs #40 signing) — `wb.lock` + signed run
receipts.
**#48 `wb-core` + WASM** — extract crate, feature-gate subprocess execution out of
the WASM target (parse/validate/inspect + pure runtime only).

## 3. Cross-cutting guardrails

- **Zero-dep identity:** any bet adding runtime deps (#45 sql, #48 wasm) ships as
  a *feature-gated build variant* — the default `wb` binary stays ~650KB.
- **"wb stays a CLI":** #39/#40/#44's server/registry surfaces are *opt-in
  commands/separate products* that wrap the binary; the core never becomes a
  daemon you must run. (See `docs/enhancement-ideas.md` §6.)
- **Security before reach:** nothing in Phase C ships before #37.
- **Each bet:** own branch, `cargo fmt --check` + `clippy -D warnings` +
  `cargo test --all-targets --locked` + `cargo build --release --locked` green,
  CLAUDE.md + TODO.md + this plan updated, before merge.

## 4. Recommended order (matches `IDEA_GOALS.md`)

1. #39 `wb mcp`  ·  2. #31→#43 docs-as-tests  ·  3. #37 trust (parallel)  ·
4. #41 capture  ·  5. #44 watch (local)  ·  6. #40 registry  ·  7. #42 repair  ·
8. #45 sql/http  ·  9. #46 cache  ·  10. #47 attestations  ·  11. #48 wb-core/WASM.

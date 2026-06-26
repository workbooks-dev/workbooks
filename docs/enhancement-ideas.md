# wb — Enhancement Ideas for 10–100x Impact

> Research note, drafted 2026-06-25 in worktree `enhancement-ideas`.
> Goal: ideas that change the *order of magnitude* of wb's reach, not the next
> increment. The incremental roadmap (params, `wb test`, cache, browser
> reliability, trust model) already lives in `TODO.md` — this doc is deliberately
> bigger and more speculative, written so we can argue about it.

---

## 1. Where wb is today (the honest baseline)

`wb` is a mature, zero-dependency Rust binary that runs fenced code blocks in
markdown. It is *already* well past "minimal notebook runner": it has
checkpoint/resume, external-signal pauses (`wait`, exit 42), HTTP/Redis callbacks
with HMAC, `include:`/`required:` composition, conditional cells, stable step IDs
+ selective runs, a browser runtime with operator handoff, sandboxed Docker
execution, secrets providers, and structured diagnostics (`validate`/`doctor`).

Its **current impact surface is narrow but deep**: it is the execution engine for
one pipeline (Xatabase runbook library → Redis streams → run pages) and the
OpenClaw VPS fleet. It is essentially an internal tool with an unusually good
foundation.

**The asymmetry that creates the opportunity:** wb has *already paid the cost* of
the hard, rare features (durable pause/resume, event streaming, human-in-the-loop
handoff, composition). What it lacks is **reach** — the distribution, surfaces,
and integrations that would let those features serve 100x more runs. Almost every
idea below is "expose what already exists to a much larger audience" rather than
"build something hard from scratch." That's why 10–100x is realistic and not
hype.

### The three growth vectors

| Vector | Question | Multiplier ceiling |
|---|---|---|
| **Reach** | Who/what can invoke wb? | 100x — today it's a few humans + one pipeline |
| **Use-cases** | What kinds of work can a workbook do? | 10x — devops runbooks → data, API, docs-as-tests, agents |
| **Depth** | How much value per run? | 5–10x — self-healing, reproducibility, shareable runs |

Reach is where the order-of-magnitude lives. The single highest-leverage move is
making **every AI agent**, not every human, a wb user.

### Competitive context

- **Runme** (runme.dev) owns the human/VS-Code/DevOps-notebook niche: a VS Code
  extension, a CLI, a GitHub Action, cloud renderers. It is human-IDE-centric and
  not designed for headless durable agent execution. wb should *not* fight Runme
  for the IDE — it should own the lane Runme can't: **headless, durable,
  agent-native execution**.
- **Jupyter** owns interactive data science, but `.ipynb` is a JSON blob, not
  diffable, not a single binary, not agent-friendly. wb's markdown-native, zero-
  dep posture is the anti-Jupyter.
- **Temporal / Inngest / Restate** own durable execution, but as SDKs/services you
  write code against. wb is durable execution you *write as a document* — far
  lower authoring cost, no service to run.
- **MCP** (Model Context Protocol) became the de-facto agent-integration standard
  in ~18 months (97M monthly SDK downloads as of early 2026) and shipped a durable
  **Tasks** primitive plus **elicitation** (human-in-the-loop) and **sampling**.
  wb's pause/resume/checkpoint model maps almost 1:1 onto these. **This is the
  single biggest unforced opening for wb** — see Idea 1.

Sources: [Runme](https://runme.dev/), [Runme CLI docs](https://docs.runme.dev/getting-started/cli/),
[2026 MCP Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/),
[MCP spec 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25).

---

## 2. Tier 1 — The big bets (order-of-magnitude reach)

### Idea 1 — `wb mcp`: wb as a Model Context Protocol server  ⭐ top pick

**What:** A `wb mcp` subcommand that runs wb as an MCP server over stdio (and
optionally HTTP/SSE), exposing wb's verbs as MCP tools: `author_workbook`,
`run_workbook`, `inspect`, `validate`, `resume`, `list_pending`, `get_run_events`.

**Why this is 10–100x:** Today wb is reachable by a human typing `wb run` or one
bespoke pipeline shelling out to it. MCP is how *every* agent platform
(Claude, OpenAI agents, Cursor, etc.) discovers and calls tools. Shipping an MCP
server turns wb from "a CLI Justin runs" into "the durable-execution tool any
agent can pick up" — instantly addressable by the entire MCP ecosystem.

The fit is uncanny because wb already has the primitives MCP's durable-task model
*needs*:

| MCP primitive | wb already has |
|---|---|
| Tasks (long-running, resumable) | `--checkpoint` + resume-by-step-id |
| Task pause / await input | `wait` fence + exit 42 + `wb resume --signal` |
| Elicitation (human-in-loop) | `pause_for_human` browser verb + resume actions |
| Progress notifications | `step.complete` / `step.skipped` callbacks |
| Structured results | `--json`, `$WB_OUTPUTS_PATH`, step outputs |

An agent could say *"run this deploy workbook, pause for my approval before the
prod step, and tell me when it's done"* and wb's existing machinery handles all
of it. We'd mostly be writing an MCP adapter over capabilities that exist.

**Effort:** Medium. New `src/mcp.rs` speaking JSON-RPC over stdio; reuse existing
command internals (now that #38 made them return structured reports). The main
work is mapping checkpoint/pending state onto MCP Tasks lifecycle.

**Risks:** MCP spec is still evolving (Tasks shipped experimental); pick a stable
subset. Keep it a thin adapter so spec churn doesn't bleed into core.

**Depends on:** #38 (done), trust/policy #37 (for non-local use). Pairs with
Idea 4 (registry) and Idea 6 (run pages).

---

### Idea 2 — A trust + registry layer: `wb run gh:org/repo/deploy.md`

**What:** Run workbooks by remote reference with a real trust model:
`wb run gh:org/repo/path.md`, `wb run https://…/x.md`, `wb run wb:acme/login`.
Back it with signing (sigstore-style or minisign), a trust store
(`wb trust add org/*`), and a lightweight registry/gallery on workbooks.dev where
runbooks are published like packages with versions, checksums, and provenance.

**Why this is 10–100x:** This is the network-effect play. Today every workbook is
private and hand-authored. A registry turns wb into a *distribution channel* — a
"login workbook for Stripe" or "Postgres health-check" written once and reused by
thousands. The Xatabase three-tier taxonomy (atoms/flows/tasks) is *already* a
package model waiting for a registry. Shared atoms = compounding value: every new
service login published makes every downstream task easier to author.

**Effort:** Large (registry + signing + trust UX), but stageable: ship remote
URL/`gh:` execution + signing first; the hosted gallery later.

**Risks:** This is the security frontier. `TODO #37` correctly gates public
sharing on a real trust model — do **not** ship remote execution without
signing + sandbox-by-default for untrusted sources. Marketing `requires:`
containers as a security sandbox today would be a mistake.

**Depends on:** #37 (trust/policy/dry-run), #30 (params, so shared workbooks are
configurable), Idea 7 (sandbox-by-default).

---

### Idea 3 — `wb capture`: record a session, emit a workbook (reverse the authoring cost)

**What:** A capture mode that records a human (or agent) doing a task — shell
history with `script`/PTY, or a browser session via the existing sidecar — and
emits a ready-to-edit workbook with steps, includes, and artifact captures
already wired. (Memory notes an existing `capture-runbook` skill in
xatabase-finance-assistant; `wb capture` should emit MANIFEST.md-compatible
output so it slots into that pipeline.)

**Why this is 10–100x:** The bottleneck on wb adoption is *authoring cost*.
"Write a runbook" is a chore; "do the thing once and get a runbook for free" is a
giveaway. This flips the funnel — every ad-hoc fix becomes a reusable, shareable,
agent-runnable artifact. It's the on-ramp that feeds the registry (Idea 2) and
the agent loop (Idea 1).

**Effort:** Medium for shell capture (PTY record + heuristic block splitting);
the browser side largely reuses the recording machinery the sidecar already has
(rrweb, screencast, artifact auto-capture).

**Risks:** Captured sessions leak secrets — must integrate scrubbing (the browser
runtime already redacts `{{ env.X }}`; extend to captured shell). Heuristic block
boundaries need human review; ship it as "draft, then edit," not "perfect."

**Depends on:** browser recording (shipped), secrets scrubbing, artifacts.

---

### Idea 4 — Self-healing runs: `wb run --repair` (close the agent loop inside wb)

**What:** On block failure, instead of just bailing, wb hands the failure
(stderr, exit code, partial output, step context) to a configured agent endpoint
and accepts a patch — a corrected command, a retry with different params, or a
"give up." The callback + resume machinery already exists; this makes the agent a
*first-class participant in the run loop* rather than an external observer.

**Why this is 10–100x:** This is the depth multiplier that makes *unattended*
operation real. Today a failed block needs a human to fix and re-run. A
self-healing loop means a runbook can survive flaky APIs, drifted state, and
small environment differences on its own — which is exactly what the OpenClaw VPS
fleet needs from headless agents. It turns "runbooks that document a process"
into "runbooks that *accomplish* a goal."

**Effort:** Medium. wb already emits `checkpoint.failed` and can resume; add a
`--repair <agent-url>` that POSTs the failure and applies a structured
`{action: rerun|patch|skip|abort, patch?}` response (reuses the resume-action
schema shape).

**Risks:** Letting an agent rewrite-and-rerun arbitrary commands is dangerous —
gate behind trust/policy (#37), allowlists, and dry-run preview. Bound the repair
loop (max attempts) to avoid runaway cost.

**Depends on:** callbacks (shipped), #37 trust, Idea 1 (MCP makes the agent side
trivial).

---

## 3. Tier 2 — Use-case expansion (each opens a new audience)

### Idea 5 — Docs-as-tests: `wb verify README.md` + a GitHub Action

**What:** A mode that runs the code blocks in *ordinary documentation* (README,
tutorials, docs site) and asserts they still work — `expect exit 0`, output
matching (builds on `wb test` #31). Ship a `workbooks/verify-action` GitHub
Action so any repo can add "our docs actually run" to CI.

**Why this is 10–100x reach:** This is the broadest possible top-of-funnel.
*Every* OSS project has rotting README examples. "Your docs are now CI-verified,
zero config, single binary" is a viral, horizontal value prop that has nothing to
do with devops runbooks — it's a different audience entirely (every maintainer).
Runme has a GitHub Action; wb's zero-dep binary + assertions can be the better one.
It also feeds adoption: people who verify docs with wb then discover runbooks.

**Effort:** Medium — mostly `wb test` (#31) plus a thin Action wrapper and
GitHub-annotation output (already scoped under #31).

**Depends on:** #31 (`wb test` / assertions), #33 (selective run for changed
blocks).

---

### Idea 6 — `wb watch` / `wb serve`: ship the run page as a product, not a bespoke pipeline

**What:** Two surfaces around the event stream wb already emits:
1. `wb watch` / `wb ui` — a **local** live viewer (TODO #35): include tree, live
   stdout/stderr, pending waits, browser screenshots, artifacts, resume buttons.
2. `wb serve` (or a hosted "wb cloud") — ingest callbacks into **shareable,
   linkable run pages**. "asciinema for runbooks": a run becomes a URL you send a
   teammate, with the full event timeline, artifacts, and outcome.

**Why this is 10–100x:** Today the run page is bespoke infrastructure that only
the Xatabase pipeline has. The events are already standardized
(`step.*`, `slice.artifact_saved`, `workflow_node`). Generalizing the viewer
means *anyone* gets the operator UI for free — and shareable run links are
inherently viral (every shared run is a wb advertisement + an onboarding
surface).

**Effort:** `wb watch` local viewer: Medium (the events exist; build a TUI or a
local web view). Hosted run pages: Large (a service — conflicts with "wb stays a
CLI", see §6).

**Depends on:** stable events (shipped), artifact manifest #36, step IDs (shipped).

---

### Idea 7 — Native declarative runtimes: `sql`, `http`, and a real sandbox-by-default

**What:** Two new first-class block runtimes that don't shell out to an
interpreter:
- **`sql`** — connect to a DB from frontmatter (`db: postgres://…` via secrets)
  and run query blocks, capturing rows as structured step outputs. Turns wb into
  a data-runbook / lightweight dbt-style tool.
- **`http`** — declarative API calls (a `.http`/REST-client-style block): method,
  URL, headers, body, assertions on status/JSON. Turns wb into a runnable,
  diffable Postman/Bruno for API runbooks and smoke tests.

Plus **sandbox-by-default for untrusted workbooks** (seccomp/landlock/wasm on
Linux) so remote/registry workbooks (Idea 2) can run safely.

**Why this is 10–100x use-cases:** `sql` and `http` are the two most common
things a runbook *actually does* (query state, hit an API), and today they
require boilerplate bash + `psql`/`curl`/jq. First-class support removes the
boilerplate and gives structured outputs that downstream `{when=}` conditionals
and assertions can use — opening data-ops and API-ops as whole new audiences.

**Effort:** `http` runtime: Medium (an HTTP client + assertion grammar). `sql`:
Medium-Large (driver deps threaten "zero runtime deps" — consider feature-gated
builds or a `cmd`-provider shim first). Sandbox-by-default: Large.

**Risks:** Driver dependencies vs. the single-binary/zero-dep identity — this is a
real tension. A `sql` runtime might justify a `wb-full` build variant.

**Depends on:** secrets (shipped), step outputs (shipped), assertions #31.

---

## 4. Tier 3 — Depth & durability (more value per run)

### Idea 8 — Content-addressed execution cache (TODO #18, framed as a build tool)

**What:** Skip blocks whose source + params + env/secret identity + included
files + declared inputs haven't changed. Make blocks declare `inputs:`/`outputs:`
so wb can build a dependency graph.

**Why it matters:** This is the difference between "a script runner" and "a build
tool." Iterative agent re-runs become near-instant; wb starts competing with
Make/Task/just for the glue-script niche, but with durability and human-in-loop
built in. Caching is what makes the self-healing loop (Idea 4) cheap to retry.

**Effort:** Large — cache identity is subtle (this is why TODO splits #18 from
#33 and defers it). Needs params (#30) and a real cacheability/purity model first.

---

### Idea 9 — Reproducibility & provenance: lockfiles + signed run attestations

**What:** A `wb.lock` capturing resolved runtime versions, image digests, included
file hashes, and param values; and signed **run attestations** ("this exact
workbook ran at this time with this result, here's the proof"). An attestation is
a verifiable receipt for a run.

**Why it matters:** This is what makes wb credible for **compliance and audited
ops** (SOC2 change management, regulated deploys) — a much higher-value audience
than "devops convenience." A signed, replayable runbook *is* an audit log. This
is also a natural premium/enterprise hook for the hosted side.

**Effort:** Medium-Large. Builds on checkpoint state + sandbox image hashing
(both partially exist) + the signing work from Idea 2.

---

### Idea 10 — `wb-core` as an embeddable library + WASM target

**What:** Factor the parser + executor into a `wb-core` crate, publish it, and
compile a WASM build so workbooks can be parsed/validated (and pure blocks run) in
the browser or inside other tools.

**Why it matters:** Reach via embedding. workbooks.dev could run/preview workbooks
client-side; other Rust tools could embed the runner; the parser becomes a
reusable standard for "executable markdown." Lower-probability but high-ceiling —
it makes the *format* a platform, not just the CLI.

**Effort:** Large; mostly refactor + careful feature-gating (subprocess execution
can't go to WASM, but parse/validate/inspect and a sandboxed pure-runtime can).

---

## 5. Summary: impact vs. effort vs. risk

| # | Idea | Vector | Impact | Effort | Risk | Leverage on existing code |
|---|------|--------|--------|--------|------|---------------------------|
| 1 | `wb mcp` server | Reach | ★★★★★ | Med | Low–Med | Very high (reuses ckpt/pause/callbacks) |
| 2 | Trust + registry / remote run | Reach | ★★★★★ | Large | High (security) | Med (needs #37) |
| 3 | `wb capture` | Reach/use | ★★★★☆ | Med | Med (secrets) | High (browser recording) |
| 4 | `wb run --repair` self-heal | Depth | ★★★★☆ | Med | High (safety) | High (callbacks/resume) |
| 5 | Docs-as-tests + Action | Use/reach | ★★★★☆ | Med | Low | High (needs #31) |
| 6 | `wb watch` / run pages | Reach | ★★★★☆ | Med–Large | Med (philosophy) | High (events exist) |
| 7 | `sql` / `http` runtimes + sandbox | Use | ★★★★☆ | Med–Large | Med (deps) | Med |
| 8 | Execution cache | Depth | ★★★☆☆ | Large | Med | Med (needs #30) |
| 9 | Lockfiles + attestations | Depth | ★★★☆☆ | Med–Large | Low | Med |
| 10 | `wb-core` lib + WASM | Reach | ★★★☆☆ | Large | Med | Low (big refactor) |

---

## 6. Honest tensions with current direction

Three of these ideas cut against stated non-goals — flagging them so the analysis
is real, not cheerleading:

1. **"wb stays a CLI; no cron/schedule layer; no service"** (features-request.md
   non-goals). Ideas 1 (`wb mcp` as a server), 6 (`wb serve`/hosted run pages),
   and 2 (registry) all push wb toward being a *service*, not just a CLI. The
   resolution: keep the **core binary** a pure CLI, and ship the server/registry
   surfaces as *separate, opt-in* commands/products that wrap it. `wb mcp` is
   still "one binary, one process you launch" — it doesn't make wb a daemon you
   must run.

2. **"Zero runtime deps / single ~650KB binary"** is core identity. Idea 7's
   `sql` runtime and Idea 10's WASM both threaten it. Mitigation: feature-gated
   builds (`wb` stays tiny; `wb-full` adds DB drivers), and lean on the existing
   `cmd`/`command` secret-provider pattern as the dependency-free escape hatch
   before adding native drivers.

3. **Security is the gate on reach.** Ideas 2, 4, and 7 all expand what untrusted
   code can do. `TODO #37` (trust/policy/dry-run/sandbox-by-default) is therefore
   not optional polish — it is the *prerequisite* for the entire reach story.
   Sequence it early.

---

## 7. Recommended portfolio (if forced to pick)

A balanced bet across the three vectors, ordered by leverage-to-effort:

1. **`wb mcp` (Idea 1)** — highest reach-per-effort; reuses everything already
   built; rides the biggest tailwind (MCP standardization). *Start here.*
2. **`wb test` → docs-as-tests + Action (Idea 5, building on TODO #31)** —
   broadest, lowest-risk top-of-funnel; a different audience entirely.
3. **Trust/policy #37 → remote run + registry (Idea 2)** — the network-effect
   engine; gated on security work, so start #37 in parallel with 1 & 2.
4. **`wb capture` (Idea 3)** — collapses authoring cost, feeds the registry.
5. **`wb watch` local viewer (Idea 6, part 1)** — generalize the run page;
   defer the hosted service until the local one proves the shape.

Ideas 4, 7, 8, 9, 10 are strong second-wave bets once the reach surfaces exist —
self-healing and `sql`/`http` runtimes especially become far more valuable once
agents (via MCP) and a registry are driving the runs.

**One-line thesis:** wb already built the expensive, rare durable-execution
engine. The 10–100x is not more engine — it's *plugging that engine into the
agent ecosystem (MCP), into a sharing network (registry), and into a much wider
set of jobs (docs-tests, sql/http, capture)*, with the trust model as the gate
that makes all of it safe to open up.

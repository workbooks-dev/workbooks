# wb features-request

Longer-form specs for features the Xatabase runbook library needs next. Companion to `TODO.md` — items here get promoted to numbered TODO entries once they're scoped enough to build.

---

## Context

The Xatabase runbook library (xata.paracord.sh) is restructuring around a three-tier taxonomy that leans hard on wb's composition primitives:

- **Atoms** — single-purpose reusable blocks. Canonical example: service login. One file per service at `services/<svc>/login.md`.
- **Flows** — technical end-to-end orchestrations. Include atoms, carry troubleshooting prose. Live at `services/<svc>/flows/<name>/README.md` with captures/screenshots colocated as siblings.
- **Tasks** — operator-facing entry points. Short narrative + includes. Cross-service tasks decompose into per-service subtasks under `tasks/<name>/<svc>.md`, with `tasks/<name>/README.md` as the orchestrator.

The operator doesn't read the markdown. They watch a live run page that streams events from the notify stream. The runbook source is for authors and AIs; the run page is the UI. **That split drives every feature below.**

Primitives this file builds on — all shipped:

- **`include:` fence** (TODO #27) — parse-time expansion, shared env + `$WB_ARTIFACTS_DIR`, cycle detection.
- **`pause_for_human:` browser verb** — generic operator handoff with `operator_click` / `poll` / `timeout` resume modes. Backwards-compat `wait_for_mfa` wrapper preserved. Shipped in `b63c8f3`; demo at `examples/pause-for-human-demo.md`.
- **`wait_for_drop:` browser verb** — polls a Drive folder through the Paracord relay and resumes once a file-predicate matches. Shipped in `b63c8f3` + `3f6514a`; demo at `examples/wait-for-drop-demo.md`.
- **Include-scoped step events** — `step_kind` / `step_id` / `step_title` / `parent_step_id` enrichments on `step.*` events let the run page render the three-include mental model instead of the expanded verb list. Shipped in `b63c8f3`.
- **`step.artifact_saved` (F1) + artifact labels (F2)**, **structured step outputs / `$WB_OUTPUTS_PATH` (F3)**, **`step.skipped` (F4)**, **`workflow:` metadata manifest (F5)**, **opt-in default block timeout (F6)**, and **operator-driven in-flight cell control (F7)** — all shipped (F1–F6 across v0.11–v0.14; F7 after). Their specs are recorded in `TODO.md`; `git show <tag>:features-request.md` recovers the original write-ups.

No open items remain in this file. New longer-form specs go below as they're proposed.

---

## Non-goals

- **No inline file upload UI on the run page.** `wait_for_drop`'s bet is that Drive is a better staging surface than building our own upload widget. If Drive proves unreliable we revisit, but the healthcheck track (see `kb/runbooks/health-checks/google-drive-probe.md`) monitors that.
- **No new runtime.** Any feature here should be an extension to existing browser-slice + sidecar machinery. If a feature wants a new runtime, it belongs in TODO, not here.
- **No cron/schedule layer.** That's the runbook library's concern (trigger stream, notify stream, run pages). wb stays a CLI.

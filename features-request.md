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

---

## F1. `step.artifact_saved` — callback event for auto-captured artifacts

### Motivation

wb already has the plumbing for "here is a file the runbook produced": `$WB_ARTIFACTS_DIR` is a scout-owned directory the orchestrator controls (`export WB_ARTIFACTS_DIR=./scout-artifacts/<run_id>/` before `wb run`), and `Artifacts::sync()` runs after every cell to pick up new/changed files. Scout is already in the same filesystem as `wb` — it doesn't need wb to round-trip files through Google Drive to see them.

**The gap is correlation, not transport.** Today, when a bash or python cell writes a CSV into `$WB_ARTIFACTS_DIR`, the auto-upload path (`src/artifacts.rs:107`) uploads silently and the callback stream sees nothing. The run page has no way to render "📄 statement.csv ready at step 4" in the timeline because the notify stream never got an event. Browser verbs (`save:`, `screenshot:`) already emit `slice.artifact_saved` — the inconsistency is that bash/python/sandbox cells don't.

### Proposal

Teach `Artifacts::sync()` to return the set of newly-seen files, and have the main loop emit a `step.artifact_saved` callback event for each — **after** `sync()` returns, **before** `step_complete` fires. That ordering groups artifacts visually under the step that produced them.

Event shape (mirrors browser `slice.artifact_saved` naming so cross-runtime consumers don't need a lookup table):

```json
{
  "event": "step.artifact_saved",
  "filename": "statement.csv",
  "path": "/abs/path/to/scout-artifacts/run-abc123/statement.csv",
  "bytes": 18234,
  "content_type": "text/csv",
  "step_index": 4,
  "step_total": 12,
  "workbook": "tasks/month-end-close/hsbc.md",
  "label": null
}
```

- Fires once per sync() detection. mtime-debounced like today — a rewrite with the same filename fires a fresh event (matches how the upload loop already behaves).
- Fires regardless of whether `WB_ARTIFACTS_UPLOAD_URL` is set — the event is about *existence*, not *transport*.
- `label` is null unless F2 (below) attaches one.
- Does not fire for `pause_result.json` or other wb-internal sidecar files — carve out an exclude-list (`pause_result.json`, `*.meta.json`, `*.wb.json`) to keep the signal clean.
- **Suppressed by `{silent}` blocks.** `{silent}` is a hard off-switch: the block and its side effects (including artifact events) stay out of the notify stream. If the operator needs an artifact surfaced, don't mark the block silent.
- **Include-scoped.** When the producing block runs inside an `include:` fence, the event carries the same `step_kind` / `step_id` / `step_title` / `parent_step_id` fields as `step.complete`, so the run page nests the artifact under the correct include in the timeline tree. Pass-through of the existing include-stack — no new data.

### Implementation sketch

1. Change `Artifacts::sync()` signature to return `Vec<ArtifactRecord>` of newly-seen files with `{path, filename, bytes, content_type, mtime}`.
2. Main loop in `src/main.rs:1626` takes the Vec and calls `cb.step_artifact_saved(...)` for each entry before `step_complete`.
3. Add `Callback::step_artifact_saved` in `src/callback.rs` following the same HTTP-POST-with-HMAC pattern as `step_complete`.
4. Emit `step.artifact_saved` from the sandbox exit path (`src/main.rs:1810`) too, so containerized runs get the same behavior.
5. `slice.artifact_saved` (browser, in-process) and `step.artifact_saved` (callback stream) stay as two separate events with different audiences — the browser one goes to the sidecar event ingester for live streaming, the new one goes to the persistent notify stream that scout consumes. (Or: consolidate later if consumers end up wanting one canonical event. Ship the simple thing first.)

---

## F2. Artifact labels — sidecar convention + `announce_artifact:` verb

### Motivation

`step.artifact_saved` gives the run page "a file appeared." F2 gives it "a file appeared **and here's what to call it**." A CSV titled `statement.csv` in the timeline is forgettable; "📄 April HSBC statement" is legible. Needed as soon as a task produces more than one artifact and the operator has to pick between them.

### Proposal — sidecar convention (language-agnostic)

When `sync()` sees `foo.csv`, it also checks for `foo.csv.meta.json` and, if present, parses it and attaches the label to the `step.artifact_saved` event. Shape:

```json
{
  "label": "April HSBC statement",
  "description": "Closing balance reconciliation, downloaded from HSBC export portal"
}
```

- `label` surfaces as the timeline line ("📄 April HSBC statement"). Required.
- `description` optional — hover/click reveal on the run page.
- Sidecars live next to the artifact in `$WB_ARTIFACTS_DIR` and are included in the sync() exclude-list (they don't fire their own `step.artifact_saved`).
- Unknown keys in the sidecar are ignored — leaves room for future fields (category, mime-override, checksum, etc.) without a schema version.

Usage from bash:
```bash
cp statement.csv "$WB_ARTIFACTS_DIR/"
cat > "$WB_ARTIFACTS_DIR/statement.csv.meta.json" <<EOF
{"label": "April HSBC statement"}
EOF
```

Usage from python:
```python
import json, os, shutil
dest = os.path.join(os.environ["WB_ARTIFACTS_DIR"], "statement.csv")
shutil.copy("statement.csv", dest)
with open(dest + ".meta.json", "w") as f:
    json.dump({"label": "April HSBC statement"}, f)
```

### Proposal — `announce_artifact:` browser verb

Ergonomic shorthand in browser slices. Takes a path already in `$WB_ARTIFACTS_DIR` (typically just-written by `save:` or `screenshot:`) and writes the sidecar JSON for it. No upload, no network — just label metadata.

```yaml
- save:
    path: statement.csv
    from: $last_result
- announce_artifact:
    path: statement.csv
    label: "April HSBC statement"
    description: "Reconciled balance export"
```

Internally: resolves `path` inside `$WB_ARTIFACTS_DIR` (same `resolveInside` guard `save:` uses), writes `{path}.meta.json`. The subsequent `Artifacts::sync()` picks up both files, emits `step.artifact_saved` for the artifact with the label populated from the sidecar.

Not strictly necessary — browser cells can call `save:` then drop a sidecar via a subsequent bash cell — but it's a one-liner in the common case and matches how `pause_for_human` / `wait_for_drop` read.

### Timing rules

- **Sidecar must be written in the same cell as the artifact** (or earlier — e.g. you can pre-write `foo.csv.meta.json` in block 3 and drop `foo.csv` in block 5). If the artifact lands first and the sidecar arrives in a later block, the first `step.artifact_saved` will fire with `label: null` and there is **no re-emission**. Scout can still correlate after-the-fact via `filename`, but the first timeline entry will be un-labeled. Document this; keep the mechanism simple.
- **On rewrite, the label is re-read fresh.** If a block overwrites both `foo.csv` and `foo.csv.meta.json`, the second `step.artifact_saved` event carries the updated label — sync() does not cache sidecar metadata across emissions. Cheap to re-read, avoids stale-label surprises.

---

## Non-goals

- **No inline file upload UI on the run page.** `wait_for_drop`'s bet is that Drive is a better staging surface than building our own upload widget. If Drive proves unreliable we revisit, but the healthcheck track (see `kb/runbooks/health-checks/google-drive-probe.md`) monitors that.
- **No new runtime.** Any feature here should be an extension to existing browser-slice + sidecar machinery. If a feature wants a new runtime, it belongs in TODO, not here.
- **No cron/schedule layer.** That's the runbook library's concern (trigger stream, notify stream, run pages). wb stays a CLI.

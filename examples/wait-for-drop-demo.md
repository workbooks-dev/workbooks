---
title: wait_for_drop demo
---

# Waiting for files in a Drive folder

`wait_for_drop` polls a Google Drive folder (through the Paracord relay's
`google_drive` connector) and only proceeds once the folder's contents
satisfy a predicate. Operator sees a live "waiting for N/M files" widget on
the run page driven by the `slice.drop_poll` heartbeat.

Unlike `pause_for_human`, this does **not** exit 42 — the sidecar loops in-
process until the files arrive, a timeout fires, or the slice deadline hits.
That trade-off keeps the operator experience simple (one widget, one
progress bar) at the cost of requiring `wb` to stay running through the
wait.

## Requirements

```yaml
# Required env:
# PARACORD_RELAY_URL         — base URL of the Paracord relay
# PARACORD_RELAY_API_KEY     — bearer token with google_drive read permission
#
# Optional env (for long waits):
# WB_SLICE_DEADLINE_MS       — sidecar-side per-slice cap (default 120000 = 2min);
#                              bump this when `timeout:` exceeds 2 minutes, e.g.
#                              export WB_SLICE_DEADLINE_MS=2000000    # ~33 min
```

## Example 1 — wait for any file to appear

```browser
session: default
verbs:
  - pause_for_human:
      message: "Drop the statement in the folder below, then resume to start processing"
      context_url: https://drive.google.com/drive/folders/REPLACE_ME
  - wait_for_drop:
      folder_url: https://drive.google.com/drive/folders/REPLACE_ME
      expect: at_least_one_file
      poll_every: 10s
      timeout: 30m
      bind_artifact: dropped_files
```

```bash
jq -r '.files[].name' "$WB_ARTIFACTS_DIR/dropped_files.json"
```

## Example 2 — wait for a specific filename pattern

Useful when the folder is a long-lived staging area and you only want to
react to files with a specific shape (statements, receipts, exports). The
glob supports `*` and `?`.

```browser
session: default
verbs:
  - wait_for_drop:
      folder_url: https://drive.google.com/drive/folders/REPLACE_ME
      expect: filename_matches
      filename_pattern: "statement-*.pdf"
      poll_every: 15s
      timeout: 1h
      bind_artifact: statements
```

## Notes

- **Poll budget**: with defaults (10s poll, 30m timeout), one `wait_for_drop`
  = up to 180 relay calls. Parallel waits multiply. If you hit rate limits,
  bump `poll_every:` rather than shortening `timeout:`.

- **Race condition**: the folder is read at every poll, not only on change.
  `at_least_one_file` returns true immediately if the folder already has
  content. For "wait for a *new* file," empty the folder first or use
  `filename_matches` with a timestamp in the pattern.

- **Relay failures**: a transient 5xx from the relay doesn't abort the wait —
  it's logged as `slice.drop_poll` with `error:` set and polling continues
  until timeout. A missing API key or misconfigured relay (401/403) throws
  immediately, since those won't self-heal.

- **No operator override** (yet): an operator who wants to proceed early
  can't short-circuit the poll loop. If the predicate is too strict or the
  file was already present, kill the run and retry with different config.
  The spec's symmetric `publish_artifact_to_drive:` sugar is deferred.

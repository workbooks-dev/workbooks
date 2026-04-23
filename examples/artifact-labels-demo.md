---
title: Artifact labels + step.artifact_saved events
---

# Labelled artifacts

`wb` emits a `step.artifact_saved` callback event for every new file a
cell drops into `$WB_ARTIFACTS_DIR`. An optional `<filename>.meta.json`
sidecar attaches a human-readable `label` (and optional `description`) to
that event, so orchestrators like scout can render "📄 April HSBC
statement" in a timeline rather than `statement.csv`.

## Write an artifact + sidecar from bash

The sidecar is a plain JSON file named `<artifact>.meta.json`. Write it
in the same cell as the artifact so the first `step.artifact_saved`
event carries the label (sidecars landing in a later block fire an
un-labelled event first — no re-emission).

```bash
cat > "$WB_ARTIFACTS_DIR/statement.csv" <<EOF
date,amount
2026-04-01,120.50
2026-04-02,-42.00
EOF

cat > "$WB_ARTIFACTS_DIR/statement.csv.meta.json" <<EOF
{
  "label": "April HSBC statement",
  "description": "Reconciled balance export"
}
EOF
```

## Or from a browser slice via announce_artifact

`save:` writes the artifact; `announce_artifact:` drops the sidecar next
to it. Both files are picked up by the same `sync()` pass at block end,
so the `step.artifact_saved` event carries the label.

```browser
session: demo
verbs:
  - goto: https://example.com
  - extract:
      selector: h1
      fields:
        title: .
  - save: page-heading
  - announce_artifact:
      path: page-heading.json
      label: "Example.com page heading"
      description: "Raw extract for the landing page title"
```

## Suppressed under `{silent}` blocks

`{silent}` is a hard off-switch — no `step.complete`, no
`step.artifact_saved`, no noise of any kind. If an operator needs to see
the artifact, don't mark the block silent.

```bash {silent}
# This block's output AND any artifacts it writes stay off the notify
# stream. Uploads (to WB_ARTIFACTS_UPLOAD_URL) still happen — silence is
# about the event stream, not the filesystem side effects.
echo "setup only" > "$WB_ARTIFACTS_DIR/setup.log"
```

## Sidecar fields

| Field         | Required | Description                                |
|---------------|----------|--------------------------------------------|
| `label`       | yes      | Short human-readable name for the artifact |
| `description` | no       | Longer prose; surfaced on hover/expand     |

Unknown keys are ignored, so the sidecar schema can grow without a
version field. Malformed JSON is silently skipped — you get an
un-labelled event rather than a run failure.

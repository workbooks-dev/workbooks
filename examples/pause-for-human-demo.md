---
title: pause_for_human demo
---

# Generic operator handoff — three flavors

`pause_for_human` is the single verb for any point where a workbook needs a
human to act before it can proceed: completing MFA, uploading a file,
approving a change, picking between options. One verb, one event shape
(`step.paused` with `reason: pause_for_human`), one run-page renderer — the
operator sees a consistent prompt no matter which pattern the workbook
author reaches for.

All three examples below share the same skeleton:

1. Fire `pause_for_human`, exit 42.
2. Operator sees the run page, takes the off-band action (MFA, upload, click),
   resumes with `wb resume <id>` (or `--value <choice>` for branching).
3. A downstream cell reads `$WB_ARTIFACTS_DIR/pause_result.json` when the
   pause used `actions:` — otherwise it just proceeds.

Run this as a browser workbook:

```bash
wb run examples/pause-for-human-demo.md --checkpoint pause-demo
# (exits 42 at the first pause)
wb resume pause-demo
```

## 1. MFA / in-browser action

```browser
session: demo
verbs:
  - goto: https://example.com/login
  - fill:
      selector: "input[name=email]"
      value: "{{ env.LOGIN_EMAIL }}"
  - pause_for_human:
      message: "Complete the 2FA challenge in the open browser, then resume"
      resume_on: operator_click
```

## 2. Drop a file in a Drive folder

No `actions:` — operator clicks a single "Resume" once their upload lands.
The `context_url` is what the run page renders as the deep-link.

```browser
session: demo
verbs:
  - pause_for_human:
      message: "Drop this month's receipts in the folder below, then resume"
      context_url: https://drive.google.com/drive/folders/REPLACE_ME
      resume_on: operator_click
      timeout: 1h
```

## 3. Approval decision (branching)

The `actions:` list becomes branching buttons on the run page. Whichever the
operator clicks ends up in `$WB_ARTIFACTS_DIR/pause_result.json` as
`{"value": "approved"}` (or `"denied"`), and the bash cell below branches on
it. No custom UI, no wait-for-webhook plumbing — just a resume with a
value.

```browser
session: demo
verbs:
  - pause_for_human:
      message: "Expense request looks high. Approve to continue, deny to abort."
      context_url: https://example.com/requests/42
      actions:
        - label: "Approved"
          value: approved
        - label: "Denied"
          value: denied
```

```bash
CHOICE=$(jq -r '.value' "$WB_ARTIFACTS_DIR/pause_result.json")
echo "operator chose: $CHOICE"
case "$CHOICE" in
  approved) echo "proceeding with expense submission" ;;
  denied)   echo "aborting"; exit 1 ;;
  *)        echo "unknown choice ($CHOICE), aborting"; exit 1 ;;
esac
```

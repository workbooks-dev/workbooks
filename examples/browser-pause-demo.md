---
title: Browser Pause Demo
---

# Browser pause/resume demo

Exercises the intra-step lifecycle:

1. A browser slice with `act:` → fires a `step.recovered` callback.
2. A browser slice with `wait_for_mfa:` → emits `slice.paused`; wb persists
   sidecar state, fires `step.paused` + `workbook.paused` callbacks, exits 42.
3. `wb resume <id>` restarts the sidecar, hands back the saved state + signal,
   the slice continues from the paused verb, run completes.

## Setup

```bash
echo "pre-check on host"
```

## Login with AI recovery

```browser
session: airbase
verbs:
  - goto: https://app.airbase.io
  - click: "button.sign-in"
  - act: "click the approve button"     # demo skeleton fires slice.recovered
  - fill:
      selector: "input[name=email]"
      value: "ops@example.com"
```

## MFA gate — pauses here

```browser
session: airbase
verbs:
  - wait_for_mfa:
      provider: totp
  - click: "button.continue"
  - assert:
      url_contains: /dashboard
```

## Post-login cleanup

```bash
echo "this runs only after resume"
```

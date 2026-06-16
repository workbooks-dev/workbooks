---
title: Conditional Pause Demo
runtime: bash
---

# Gating a step on a prior step's output

A step's `output: name=value` is exported into the session env as
`$WB_OUT_name`, so a later cell can branch on a value an earlier step computed.
This is the pattern for making a human pause conditional: detect the precondition
first, then gate the pause on it.

This demo is bash-only so it runs without a browser sidecar. In a real runbook,
step 1 would be a `browser` slice that evals login state, and step 3 would be a
`browser {when=$WB_OUT_needs_login}` slice containing `pause_for_human`.

## 1. Detect the precondition

Pretend we probed an authenticated session. Flip `needs_login` to `1` to see the
guarded step fire.

```bash
needs_login=0
echo "checked session: needs_login=$needs_login"
echo "output: needs_login=$needs_login"
```

## 2. The "pause" — only when login is actually needed (skipped when warm)

In a browser runbook this fence would be `browser {when=$WB_OUT_needs_login}`
wrapping a `pause_for_human` verb. With `needs_login=0` above, this step is
skipped and the run proceeds untouched.

```bash {when=$WB_OUT_needs_login}
echo "would pause for human login here"
```

## 3. Runs straight through when already authenticated

```bash {skip_if=$WB_OUT_needs_login}
echo "warm profile — proceeding without a pause"
```

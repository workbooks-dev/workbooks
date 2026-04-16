---
title: Wait Demo — Manual Signal Resolver
---

# Wait Demo

Demonstrates the `wait` primitive end-to-end with `kind: manual` — no
external resolver needed, a human runs `wb resume` to unblock the workbook.

This example requires the experimental flag:

```
export WB_EXPERIMENTAL_WAIT=1
wb run examples/wait-demo.md --checkpoint demo-1
```

When the run hits the wait block below, `wb` will:
- write a checkpoint under `~/.wb/checkpoints/demo-1.json`
- write a pending descriptor at `~/.wb/checkpoints/demo-1.pending.json`
- exit with code 42

## Step 1 — trigger something async

```bash
echo "Pretending to send a login request..."
echo "An OTP will arrive any moment now."
```

## Step 2 — wait for the OTP

```wait
kind: manual
bind: otp_code
timeout: 1h
on_timeout: abort
```

## Step 3 — use the bound value

```bash
echo "OTP received: $otp_code"
echo "Completing login..."
```

## Resuming

After the workbook pauses, resume it by providing the captured value:

```
# Shortest — single bind, inline value:
wb resume demo-1 --value 123456

# Or with a JSON payload:
echo '{"otp_code": "123456"}' > /tmp/otp.json
wb resume demo-1 --signal /tmp/otp.json

# Or piped from an agent:
echo '{"otp_code": "123456"}' | wb resume demo-1 --signal -
```

---
title: Deploy Check
runtime: bash
secrets:
  - provider: env
    keys: [DEPLOY_HOST]
  - provider: dotenv
    command: .env
---

# Deploy Check

Run after a deployment to verify everything is working.
Uses `--bail` to stop on first failure.

Usage: `wb run deploy-check.md --bail`

## Check the host is set

```bash
echo "Checking ${DEPLOY_HOST:-localhost}..."
```

## HTTP health check

```bash
HOST="${DEPLOY_HOST:-localhost}"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://${HOST}/health" 2>/dev/null || echo "000")

if [ "$STATUS" = "200" ]; then
    echo "Health check passed (HTTP $STATUS)"
else
    echo "Health check failed (HTTP $STATUS)" >&2
    exit 1
fi
```

## Check response time

```bash
HOST="${DEPLOY_HOST:-localhost}"
TIME=$(curl -s -o /dev/null -w "%{time_total}" "http://${HOST}/" 2>/dev/null || echo "timeout")
echo "Response time: ${TIME}s"

# Fail if over 2 seconds
if [ "$(echo "$TIME > 2" | bc 2>/dev/null || echo 0)" = "1" ]; then
    echo "Response time too slow" >&2
    exit 1
fi
```

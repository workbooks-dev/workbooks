---
source: deploy-check.md
title: Deploy Check
ran_at: 2026-03-27T21:12:31.272442+00:00
duration: 0.0s
status: fail
blocks: { total: 3, passed: 0, failed: 3 }
---


# Deploy Check

Run after a deployment to verify everything is working.
Uses `--bail` to stop on first failure.

Usage: `wb run deploy-check.md --bail`

## Check the host is set

```bash
echo "Checking ${DEPLOY_HOST:-localhost}..."
```

**[FAIL]** _0.0s_
**stderr:**
```
Failed to spawn bash: No such file or directory (os error 2)
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

**[FAIL]** _0.0s_
**stderr:**
```
Failed to spawn bash: No such file or directory (os error 2)
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

**[FAIL]** _0.0s_
**stderr:**
```
Failed to spawn bash: No such file or directory (os error 2)
```

---

_Ran 3 blocks in 0.0s — 0 passed, 3 failed_

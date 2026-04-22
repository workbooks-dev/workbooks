---
title: Include demo
---

# Deploy (using reusable login)

This workbook factors out its login into `include-login.md`. Every runbook that needs
the same login `include:`s it — change the login logic once, every caller picks it up.
You can also test the login in isolation: `wb run examples/include-login.md`.

The included blocks run inline with the parent's env + `$WB_ARTIFACTS_DIR`, so the
session the login writes is visible to this workbook's blocks.

```include
path: ./include-login.md
```

```bash
echo "deploy step — session was:"
cat "$WB_ARTIFACTS_DIR/session.json"
```

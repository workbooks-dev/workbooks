---
title: Reusable login (fake)
---

# Login

A stand-alone "login" workbook you can test on its own (`wb run examples/include-login.md`)
and reuse across other workbooks via an `include:` fence. Real logins would set session
tokens / cookies; this example just writes a `session.json` artifact that downstream
workbooks can read from `$WB_ARTIFACTS_DIR`.

```bash
echo "logging in as ${LOGIN_USER:-demo-user}..."
mkdir -p "$WB_ARTIFACTS_DIR"
printf '{"user":"%s","session":"%s"}\n' "${LOGIN_USER:-demo-user}" "fake-token-$$" \
  > "$WB_ARTIFACTS_DIR/session.json"
echo "wrote session to $WB_ARTIFACTS_DIR/session.json"
```

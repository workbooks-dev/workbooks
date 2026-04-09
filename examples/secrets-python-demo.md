---
title: Secrets in Python
runtime: python
secrets:
  provider: env
  keys: [HOME, USER, API_KEY]
---

# Secrets in Python

Secrets declared in frontmatter are injected as environment variables before
each block runs, so Python consumes them with `os.environ` — no extra plumbing.
The same mechanism works for every runtime `wb` supports.

## Verify the injected secrets

Print each declared secret with the value masked. Missing secrets are
reported, not crashed on — so this block is safe to run without real
credentials.

```python
import os

for key in ["HOME", "USER", "API_KEY"]:
    val = os.environ.get(key)
    if val is None:
        print(f"{key}: (not set)")
    else:
        masked = val[:4] + "…" if len(val) > 4 else "•••"
        print(f"{key}: {masked}")
```

## Use a secret for HMAC signing

Realistic local use of a secret — sign a webhook payload the same way
`wb`'s own `--callback-secret` flag does. No network required, stdlib only.

```python
import hmac
import hashlib
import json
import os

secret = os.environ.get("API_KEY", "dev-secret").encode()
payload = json.dumps({"event": "deploy.complete", "version": "1.2.3"}).encode()

signature = hmac.new(secret, payload, hashlib.sha256).hexdigest()

print(f"payload:   {payload.decode()}")
print(f"signature: sha256={signature}")
```

## Build an authenticated HTTP request

Construct an outbound request using a bearer token. The request isn't sent
— this just shows the idiomatic pattern for wiring a secret into headers
without leaking it into logs.

```python
import os
from urllib.request import Request

api_key = os.environ.get("API_KEY", "")
if not api_key:
    print("API_KEY not set — skipping request construction")
else:
    req = Request(
        "https://api.example.com/v1/deployments",
        method="POST",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        data=b'{"version": "1.2.3"}',
    )
    # Safe to log — never prints the Authorization header value
    print(f"{req.get_method()} {req.full_url}")
    print(f"Authorization: Bearer ***")
```

## Fail fast on missing secrets

Good practice for deploy checks and CI workflows: validate all required
secrets up front so the rest of the workbook can assume they're present.

```python
import os
import sys

required = ["API_KEY"]
missing = [k for k in required if not os.environ.get(k)]

if missing:
    print(f"error: missing required secrets: {', '.join(missing)}", file=sys.stderr)
    sys.exit(1)

print("All required secrets present")
```

## Running this workbook

The fence below is tagged `console` so `wb` treats it as documentation and
skips it — otherwise the workbook would recursively run itself.

```console
# Pull from shell environment
$ API_KEY=sk-test-xxxxx wb run secrets-python-demo.md

# Pull from doppler
$ wb run secrets-python-demo.md --secrets doppler --project my-project

# Pull from a .env file
$ wb run secrets-python-demo.md --secrets dotenv --secrets-cmd .env.local

# Prompt interactively for each key
$ wb run secrets-python-demo.md --secrets prompt
```

---
title: Secrets in Node.js
runtime: node
secrets:
  provider: env
  keys: [HOME, USER, API_KEY]
---

# Secrets in Node.js

Secrets declared in frontmatter are injected as environment variables before
each block runs, so Node reads them with `process.env` — no extra plumbing.
The same mechanism works for every runtime `wb` supports.

Requires Node 18+ for native `fetch`; the crypto and HMAC examples work on
any modern Node.

## Verify the injected secrets

Print each declared secret with the value masked. Missing secrets are
reported, not crashed on — so this block is safe to run without real
credentials.

```node
for (const key of ["HOME", "USER", "API_KEY"]) {
  const val = process.env[key];
  if (val == null) {
    console.log(`${key}: (not set)`);
  } else {
    const masked = val.length > 4 ? `${val.slice(0, 4)}…` : "•••";
    console.log(`${key}: ${masked}`);
  }
}
```

## Use a secret for HMAC signing

Realistic local use of a secret — sign a webhook payload the same way
`wb`'s own `--callback-secret` flag does. No network required, stdlib only.

```node
const { createHmac } = require("node:crypto");

const secret = process.env.API_KEY || "dev-secret";
const payload = JSON.stringify({ event: "deploy.complete", version: "1.2.3" });

const signature = createHmac("sha256", secret).update(payload).digest("hex");

console.log(`payload:   ${payload}`);
console.log(`signature: sha256=${signature}`);
```

## Build an authenticated HTTP request

Construct an outbound request using a bearer token. The request isn't sent
— this just shows the idiomatic pattern for wiring a secret into headers
without leaking it into logs.

```node
const apiKey = process.env.API_KEY;

if (!apiKey) {
  console.log("API_KEY not set — skipping request construction");
} else {
  const req = new Request("https://api.example.com/v1/deployments", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ version: "1.2.3" }),
  });
  // Safe to log — never prints the Authorization header value
  console.log(`${req.method} ${req.url}`);
  console.log("Authorization: Bearer ***");
}
```

## Fail fast on missing secrets

Good practice for deploy checks and CI workflows: validate all required
secrets up front so the rest of the workbook can assume they're present.

```node
const required = ["API_KEY"];
const missing = required.filter((k) => !process.env[k]);

if (missing.length > 0) {
  console.error(`error: missing required secrets: ${missing.join(", ")}`);
  process.exit(1);
}

console.log("All required secrets present");
```

## Running this workbook

The fence below is tagged `console` so `wb` treats it as documentation and
skips it — otherwise the workbook would recursively run itself.

```console
# Pull from shell environment
$ API_KEY=sk-test-xxxxx wb run secrets-nodejs-demo.md

# Pull from doppler
$ wb run secrets-nodejs-demo.md --secrets doppler --project my-project

# Pull from a .env file
$ wb run secrets-nodejs-demo.md --secrets dotenv --secrets-cmd .env.local

# Prompt interactively for each key
$ wb run secrets-nodejs-demo.md --secrets prompt
```

---
title: Native http runtime
runtime: bash
env:
  BASE: https://api.github.com
---

# The `http` runtime

`http` fences make REST calls a first-class block — no wrapping `curl` in bash.
The body is `METHOD URL`, then `Header: Value` lines, then an optional request
body. `$VAR` / `${VAR}` are substituted from the session env (frontmatter
`env:`, secrets, `--param`, …). stdout is the response body; the block exits 0
on a 2xx status and 1 otherwise, so it composes with `expect` / `wb test`.

```http
GET $BASE/repos/rust-lang/rust
Accept: application/vnd.github+json
User-Agent: wb-http-demo
```

```expect
exit 0
stdout contains "rust-lang"
```

A POST with a JSON body (a header block, a blank line, then the body):

```http {no-run}
POST $BASE/some/endpoint
Authorization: Bearer $TOKEN
Content-Type: application/json

{"hello": "world"}
```

Notes:

- `METHOD` is optional — a bare URL defaults to `GET`.
- Lines starting with `#` before the request line are ignored.
- The call has a built-in 60s timeout (`curl --max-time`).
- A non-2xx response sets `error_type: "http_status"` and a non-zero exit, so
  `--bail` stops and `wb test` can assert on it.
- The `sql` runtime is a separate, still-open item (gated on trust/#37).

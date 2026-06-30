# HARDENING_GOALS.md — next-session goals (post-v0.17.3 security sweep)

The feature roadmap (#13–#48, F1–F7) is shipped. The v0.17.1–v0.17.3 commits
are a tiered security/correctness sweep covering the **integrity gates**
(TOCTOU, resume bypass, lockfile blindness), the **sandbox** (secret leakage to
the process table, Dockerfile injection, no-network resume), and the **cache**
(upstream outputs, `{reads=}` producers), plus a tier-2 low-severity batch.

These three goals continue that sweep across the untrusted-input surfaces it
hasn't focused on yet, in priority order. Each is one session = one branch.
Same gate as before: `scripts/check.sh` green (rustfmt, clippy `-D warnings`,
locked test suite, release build), CLAUDE.md + TODO.md updated, then cut a
release. Drive each with `/code-review high` and `/security-review` over the
diff, and record findings in tiers (critical → fix immediately, low → batch).

---

### 1. Remote-fetch + signing + MCP boundary review (start here) — ✅ DONE (v0.17.4)

Shipped: checkpoint/run-id slug validation (traversal), MCP argv `--` separators +
run_id validation, `author_workbook` `.md`-only/no-`..`/`WB_MCP_ROOT` jail, JSON-RPC
16 MiB cap, `--verify-sig` no-pin authorship warning, remote `curl` proto pinning +
plaintext-http warning. See TODO.md "Security hardening (post-roadmap)".

```
/goal Adversarial security + correctness review of wb's untrusted-input boundaries that the v0.17.x sweep did not focus on: remote fetch (gh:/https: → ~/.wb/remote/<hash>.md — URL/scheme validation, SSRF, redirect handling, curl argv injection, hash-collision/path-traversal in the cache filename, and that the TOFU trust gate cannot be bypassed on a fetched file), ed25519 signing (src/signing.rs — sig-file parse robustness, pubkey pinning, content-binding completeness, key-file perms on the verify path), and the MCP server (src/mcp.rs — argv injection into current_exe, path traversal / overwrite in author_workbook, and robustness to malformed JSON-RPC input). Triage findings in tiers (critical/high → fix now; low → batch), fix the confirmed ones with regression tests, run scripts/check.sh green, update CLAUDE.md + TODO.md, and cut a release. Do not introduce new runtime dependencies.
```

### 2. Secret-redaction completeness audit across every output sink — ✅ DONE (v0.17.5)

Shipped: provider-resolved secrets (doppler/yard/dotenv/command/prompt) auto-join
the redaction set (length-guarded, ≥ 4 chars); a single `BlockResult::redact`
choke point at the executor http/sql boundary closes the http error-stderr +
resolved-URL leaks (transitively fixing the callback + `--events` sinks); artifact
`manifest.json` `label`/`description` redacted at the `sync()` choke. Audited-clean:
`run.complete`, `--repair`, `--dry-run`, `sql`, `watch --serve /state`. See TODO.md
"Security hardening (post-roadmap)".

```
/goal Audit secret + `secret:` param redaction for completeness across EVERY output sink, not just terminal rendering: the --events JSONL stream, wb watch --serve /state web payload, the artifacts manifest.json, callback payloads (step.complete/checkpoint.failed/run.complete, including failed_block.stderr), the --repair endpoint POST body, the --dry-run resolved-command plan, and the http/sql runtime output. Build (or extend) a single redaction choke point so a value marked secret cannot escape any path, add a test per sink that asserts a planted secret never appears, fix the leaks found, run scripts/check.sh green, update CLAUDE.md + TODO.md, and cut a release. Zero new dependencies.
```

### 3. Local-server + parser robustness hardening

```
/goal Harden the two remaining attack surfaces a malicious workbook/operator can reach: (a) wb watch --serve — confirm it binds 127.0.0.1 only, serves no file outside its run dir (path-traversal in any route), and cannot be made to leak secrets via /state; (b) the parser entry point that all untrusted workbooks pass through — add property/fuzz-style tests for frontmatter YAML, the {…} fence-attr cluster, include resolution (cycles, deep nesting, symlink/.. path escapes relative to the including file), and duration/expression parsing, asserting no panic/hang and bounded resource use on adversarial input. Fix what the tests surface, run scripts/check.sh green, update CLAUDE.md + TODO.md, and cut a release. No new runtime deps.
```

---

## Tracking

As each lands, note it in TODO.md under a new "Security hardening (post-roadmap)"
section and tick it here. The browser-runtime open items (`wbbr-todo.md`: rrweb
PII honesty + `maskCustom`, protocol-v1 capability negotiation, browser
pause/resume design, cross-origin rrweb repro) are a separate track to pick up
after this sweep settles.

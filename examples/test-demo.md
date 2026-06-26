---
title: Inline assertions + wb test
runtime: bash
---

# Inline assertions

Turn a runbook into a test suite by following executable blocks with `expect`
(or `assert`) fences. Each assertion is checked against the **immediately
preceding** block's result. Run it with:

```text
wb test test-demo.md                 # human report, exit 1 if any assertion fails
wb test test-demo.md --format json   # machine-readable report for CI
wb test ./examples                   # test every *.md in a folder
```

A plain `wb run` ignores `expect` fences (they never execute); `wb test` is the
command that evaluates them and sets a CI-friendly exit code.

## A passing step

```bash
echo "deploy succeeded"
```

```expect
exit 0
stdout contains "succeeded"
stderr empty
```

## Asserting a non-zero exit and stderr

```bash
echo "boom" 1>&2; ( exit 2 )
```

```expect
exit 2
stderr contains "boom"
stdout empty
```

## Assertion grammar

One assertion per line (`#` comments and blank lines ignored):

- `exit <N>` / `exit-code <N>` — exit code equals N
- `exit != <N>` — exit code does not equal N
- `stdout contains <text>` / `stderr contains <text>` — substring present
- `stdout not-contains <text>` — substring absent
- `stdout equals <text>` — exact match (trimmed)
- `stdout empty` / `stdout not-empty`

`<text>` may be quoted (`"…"` or `'…'`) to include spaces. The DSL is
intentionally tiny and dependency-free — no regex, no shell. `wb validate`
reports malformed assertion lines as `wb-expect-001`.

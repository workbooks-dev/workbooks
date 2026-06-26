# `workbooks/verify-action`

Docs-as-tests for GitHub Actions. Runs the code blocks in your Markdown docs
with [`wb`](https://workbooks.dev) and fails the job if any block errors — or if
any inline `expect`/`assert` fence fails. Keep your README's commands honest.

## Usage

```yaml
name: docs
on: [push, pull_request]
jobs:
  verify:
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v4
      - uses: workbooks-dev/workbooks/verify-action@main
        with:
          path: README.md          # a file or a folder of *.md (default: .)
          args: --bail              # extra flags for `wb verify`
```

## Inputs

| input     | default    | description                                              |
|-----------|------------|----------------------------------------------------------|
| `path`    | `.`        | File or folder of Markdown docs to verify.               |
| `args`    | `""`       | Extra flags for `wb verify` (e.g. `--format json`, `--bail`, `--param k=v`). |
| `version` | `latest`   | wb version to install.                                   |

## What "verify" means

`wb verify <path>` runs every executable fenced block in the doc(s). A file
**passes** when every block exits 0 **and** every `expect`/`assert` fence passes.
Unlike `wb test`, assertions are optional — a plain doc with runnable commands
passes when they all succeed. Exit code is `0` if all files pass, `1` otherwise.

Add assertions inline to check output, not just exit status:

````markdown
```bash
echo "build ok"
```

```expect
exit 0
stdout contains "ok"
```
````

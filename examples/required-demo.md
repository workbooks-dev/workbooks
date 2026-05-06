---
title: required-demo
required:
  - ./include-login.md
---

# Deploy (declarative prerequisites)

Same shape as `include-demo.md`, but the prerequisite workbook is declared in
the frontmatter via `required:` instead of an inline `include` fence. Useful
when prerequisites are *configuration* — "this runbook needs login + cache
warming" — rather than something you want to thread into the prose.

`required:` is sugar over `include:`: each entry is prepended at position 0
in the same order they appear in the list. Cycle detection, path resolution,
and `IncludeEnter`/`IncludeExit` sentinels work the same as fences. Multiple
prerequisites compose cleanly:

```yaml
required:
  - ./login.md
  - ./warm-cache.md
```

Inner workbooks' own `required:` is intentionally *not* recursively expanded
(treat this like a flat "needs:" list). Mirrors the include contract that the
parent's frontmatter wins.

```bash
echo "deploy step — session from required prerequisite:"
cat "$WB_ARTIFACTS_DIR/session.json"
```

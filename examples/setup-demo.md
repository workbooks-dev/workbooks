---
title: Setup Demo
setup:
  - echo "installing dependencies..."
  - echo "dependencies ready"
---

# Setup Demo

The `setup` field runs commands before any code blocks execute.
Useful for `uv sync`, `npm install`, or any environment prep.

## Verify setup ran

```bash
echo "code blocks run after setup completes"
```

## Structured form with dir

You can also specify a directory for setup commands:

```yaml
setup:
  dir: ../../
  run:
    - uv sync
    - npm install
```

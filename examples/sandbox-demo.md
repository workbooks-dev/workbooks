---
title: Sandbox Demo
runtime: python
requires:
  sandbox: python
  apt: [jq, curl]
  pip: [httpx]
---

# Sandbox Demo

This workbook runs inside a Docker container with `jq`, `curl`, and `httpx` installed.
Requires Docker to be installed and running.

## System deps available

```bash
jq --version
curl --version | head -1
```

## Python deps available

```python
import httpx
print(f"httpx {httpx.__version__} ready")
```

## Isolated from host

```bash
echo "Running as: $(whoami)"
echo "Container hostname: $(hostname)"
echo "Python: $(python3 --version)"
```

---
title: Hello Workbook
runtime: python
---

# Hello Workbook

A simple test workbook demonstrating multi-runtime execution.

## System info

```bash
echo "Running on $(uname -s) $(uname -m)"
echo "Date: $(date)"
```

## Python

```python
import sys
print(f"Python {sys.version}")
print(f"2 + 2 = {2 + 2}")
```

## Multi-step bash

```bash
for i in 1 2 3; do
  echo "Step $i"
done
echo "Done!"
```

## Non-executable blocks are skipped

This yaml block is just documentation, not executed:

```yaml
config:
  key: value
```

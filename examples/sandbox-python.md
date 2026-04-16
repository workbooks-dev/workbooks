---
title: Python Sandbox
runtime: python
requires:
  sandbox: python
  apt: [jq, curl]
  pip: [httpx, rich]
---

# Python Sandbox

Runs in an isolated container with `uv`-managed Python, system tools, and pip packages.

## Verify system deps

```bash
echo "jq: $(jq --version)"
echo "curl: $(curl --version | head -1)"
```

## Verify Python deps

```python
import httpx
from rich.console import Console

console = Console()
console.print("[bold green]rich[/] and [bold blue]httpx[/] are installed!")
console.print(f"httpx version: {httpx.__version__}")
```

## Fetch data with httpx

```python
import httpx
from rich.table import Table
from rich.console import Console

r = httpx.get("https://httpbin.org/headers")
data = r.json()

table = Table(title="Request Headers")
table.add_column("Header", style="cyan")
table.add_column("Value", style="green")

for k, v in sorted(data["headers"].items()):
    table.add_row(k, v)

Console().print(table)
```

## Process JSON with jq

```bash
curl -s https://httpbin.org/get | jq '{origin, url}'
```

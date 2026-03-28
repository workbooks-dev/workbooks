---
title: Multi-Runtime
---

# Multi-Runtime Workbook

A single workbook can mix languages. Each code block runs in its own process
using whatever runtime matches the language tag.

## Bash

```bash
echo "Hello from bash $BASH_VERSION"
```

## Python

```python
import platform
print(f"Hello from Python {platform.python_version()}")
```

## Node.js

```node
console.log(`Hello from Node ${process.version}`)
```

## Ruby

```ruby
puts "Hello from Ruby #{RUBY_VERSION}"
```

## Non-executable blocks are preserved as documentation

Config files, JSON examples, etc. are rendered but not executed:

```json
{
  "this": "is just documentation",
  "not": "executed"
}
```

---
title: Shared State Demo
exec:
   python: uv run python
   bash: zsh
---

# Shared State Between Blocks

## Python: set a variable

```python
x = 42
message = "hello from block 1"
print(f"Set x={x}")
```

## Python: use it in a later block

```python
print(f"x is still {x}")
print(message)
x += 8
print(f"Now x={x}")
```

## Bash: set a variable

```bash
MY_VAR="workbooks"
echo "Set MY_VAR=$MY_VAR"
```

## Bash: use it later

```bash
echo "MY_VAR is still $MY_VAR"
```

## Python: confirm state survived

```python
print(f"Final x={x}")
```

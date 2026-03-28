---
title: Data Pipeline
runtime: python
venv: ./.venv
---

# Data Pipeline

Fetch, transform, and summarize data.

## Generate sample data

```python
import json

data = [
    {"name": "Alice", "department": "Engineering", "salary": 120000},
    {"name": "Bob", "department": "Marketing", "salary": 95000},
    {"name": "Carol", "department": "Engineering", "salary": 135000},
    {"name": "Dave", "department": "Marketing", "salary": 88000},
    {"name": "Eve", "department": "Engineering", "salary": 145000},
]

with open("/tmp/wb_employees.json", "w") as f:
    json.dump(data, f)

print(f"Wrote {len(data)} records")
```

## Transform and analyze

```python
import json

with open("/tmp/wb_employees.json") as f:
    data = json.load(f)

by_dept = {}
for row in data:
    dept = row["department"]
    by_dept.setdefault(dept, []).append(row["salary"])

for dept, salaries in sorted(by_dept.items()):
    avg = sum(salaries) / len(salaries)
    print(f"{dept}: {len(salaries)} people, avg ${avg:,.0f}")
```

## Clean up

```bash
rm -f /tmp/wb_employees.json
echo "Cleaned up temp files"
```

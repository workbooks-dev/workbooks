# WORKBOOKS_PROJECT_FOLDER Environment Variable

## What is it?

`WORKBOOKS_PROJECT_FOLDER` is an environment variable automatically injected into every workbook kernel when it starts. It contains the **absolute path** to your project root directory.

## Why use it?

Instead of hardcoding file paths or using relative paths that might break, you can use `WORKBOOKS_PROJECT_FOLDER` to access files in your project reliably.

## How to use it

### Example 1: Read a CSV file

```python
import os
import pandas as pd

# Get the project folder path
project_folder = os.environ["WORKBOOKS_PROJECT_FOLDER"]

# Read a CSV file from the project
df = pd.read_csv(os.path.join(project_folder, "data", "sales.csv"))
print(df.head())
```

### Example 2: Save output files

```python
import os

# Save a file to the project root
project_folder = os.environ["WORKBOOKS_PROJECT_FOLDER"]
output_path = os.path.join(project_folder, "output.txt")

with open(output_path, "w") as f:
    f.write("Hello from Workbooks!")

print(f"File saved to: {output_path}")
```

### Example 3: Check if it exists

```python
import os

# Verify the environment variable is set
if "WORKBOOKS_PROJECT_FOLDER" in os.environ:
    print(f"Project folder: {os.environ['WORKBOOKS_PROJECT_FOLDER']}")
else:
    print("WORKBOOKS_PROJECT_FOLDER not set (are you running outside Workbooks?)")
```

## Common patterns

### Organized project structure

```
my-project/
├── notebooks/
│   └── analysis.ipynb         # Your workbook
├── data/
│   ├── raw/
│   │   └── sales.csv
│   └── processed/
│       └── clean_sales.csv
└── outputs/
    └── report.pdf
```

**In your workbook:**

```python
import os

project = os.environ["WORKBOOKS_PROJECT_FOLDER"]

# Read raw data
raw_data = os.path.join(project, "data", "raw", "sales.csv")

# Save processed data
processed_data = os.path.join(project, "data", "processed", "clean_sales.csv")

# Save final output
report = os.path.join(project, "outputs", "report.pdf")
```

### Helper function

```python
import os
from pathlib import Path

def project_path(*parts):
    """
    Construct a path relative to the project root.

    Example:
        project_path("data", "sales.csv")  # Returns /path/to/project/data/sales.csv
    """
    return str(Path(os.environ["WORKBOOKS_PROJECT_FOLDER"]).joinpath(*parts))

# Usage
df = pd.read_csv(project_path("data", "sales.csv"))
```

## Implementation details

- **Injected automatically**: No need to set it manually
- **Available on start**: Ready as soon as the kernel starts
- **Persists across restarts**: Available even after restarting the kernel
- **Absolute path**: Always gives you the full path, not a relative one

## Troubleshooting

**Q: What if I'm not in Workbooks?**

If you run the notebook in Jupyter or VS Code, `WORKBOOKS_PROJECT_FOLDER` won't be set. You can add a fallback:

```python
import os

project_folder = os.environ.get(
    "WORKBOOKS_PROJECT_FOLDER",
    "/path/to/default/project"  # Fallback for non-Workbooks environments
)
```

**Q: Can I change it?**

While you technically can modify `os.environ["WORKBOOKS_PROJECT_FOLDER"]`, it's not recommended. It's meant to be read-only and always point to your project root.

## Migration from hardcoded paths

**Before (hardcoded):**

```python
df = pd.read_csv("/Users/you/projects/my-project/data/sales.csv")
```

**After (with WORKBOOKS_PROJECT_FOLDER):**

```python
import os
project = os.environ["WORKBOOKS_PROJECT_FOLDER"]
df = pd.read_csv(os.path.join(project, "data", "sales.csv"))
```

**Benefits:**

- ✅ Works on any machine (macOS, Windows, Linux)
- ✅ Works if you move the project folder
- ✅ Works for teammates who clone your project
- ✅ No hardcoded paths to update

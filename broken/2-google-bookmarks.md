---
title: Google Bookmarks
runtime: bash
---

# Google Bookmarks

Parse the Chrome/Google bookmarks export, load it into Ghost Postgres, and verify the rows exist.


## Parse the export

```bash
export CWD="$(pwd)"
export PARENT="$(dirname "$CWD")"
cd $PARENT
```

```bash
uv run gists/google_bookmarks_to_csv.py \
    raw/chrome/bookmarks_4_9_26.html \
    raw/chrome/bookmarks.csv
```

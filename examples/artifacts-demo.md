---
title: Cross-cell artifacts
---

# Cross-cell artifacts

`wb` creates a per-run `$WB_ARTIFACTS_DIR` and injects it into every cell.
Anything written there is available to later cells and — if
`WB_ARTIFACTS_UPLOAD_URL` + `WB_RECORDING_UPLOAD_SECRET` are set — uploaded
to your storage endpoint after the cell that produced it completes.

## Write from a bash cell

```bash
echo '{"orders": [{"id": 1, "total": 42.00}, {"id": 2, "total": 17.50}]}' \
  > "$WB_ARTIFACTS_DIR/orders.json"
echo "wrote $(wc -c < "$WB_ARTIFACTS_DIR/orders.json") bytes"
```

## Read in a later cell

```bash
jq '.orders | length' "$WB_ARTIFACTS_DIR/orders.json"
jq '.orders[] | .total' "$WB_ARTIFACTS_DIR/orders.json" | awk '{s+=$1} END {print "total:", s}'
```

## Or capture from a browser slice

The `save:` verb writes the previous `extract`/`eval` result into the
artifacts dir. Downstream cells consume it the same way.

```browser
session: demo
verbs:
  - goto: https://example.com
  - extract:
      selector: h1
      fields:
        title: .
  - save: page-heading
```

```bash
cat "$WB_ARTIFACTS_DIR/page-heading.json"
```

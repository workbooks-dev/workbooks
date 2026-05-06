---
title: Stable step IDs + Pandoc fence attrs
runtime: bash
---

# Stable step IDs

Workbooks can name individual blocks with a Pandoc-style `{#id}` attribute.
Once a block has an explicit id, that id flows into:

- `step.complete` / `checkpoint.failed` callback payloads (`block.step_id`)
- `wb inspect --json` output (`blocks[].step_id`)
- Future selective-execution flags (`--only`, `--from`)

Blocks without an explicit id get a deterministic `auto-<hash>` id that is
stable across re-parses (hash inputs: include chain + position + language +
body prefix). Edits to one block don't shift the ids of unrelated blocks.

## Explicit id

```bash {#health}
echo "health check"
```

## Auto-derived id

Same workbook, no `{#id}`. The id will look like `auto-7b2c3f4a5e6d`.

```bash
echo "auto-id"
```

## Fence attrs replace block-number maps

Per-block timeouts and retries can live on the fence directly. Same effect as
`timeouts: {3: 30s}` in frontmatter, but the attr stays attached to the block
across edits.

```bash {#flaky timeout=30s retries=2}
curl -sf https://example.com > /dev/null && echo ok
```

## continue_on_error as a bare flag

```bash {#cleanup continue_on_error}
echo "best-effort cleanup; failure here doesn't bail the run"
```

## Try it

```bash {no-run}
wb inspect examples/step-ids-demo.md --json | jq '.blocks[] | {index, step_id, kind}'
```

When a block has both a fence attr and a legacy `timeouts:`/`retries:` entry
for the same block number, the fence attr wins and `wb validate` emits a
`wb-step-002` warning so you know to drop the legacy entry.

---
title: Conditional Blocks Demo
runtime: bash
env:
  DEPLOY_ENV: staging
---

# Conditional block execution

Illustrates `{when=…}` and `{skip_if=…}` info-string attributes. Blocks are
skipped at runtime based on env, with no execution, no callback, and no
checkpoint entry.

## Always runs

```bash
echo "always: DEPLOY_ENV=$DEPLOY_ENV"
```

## Runs only when DEPLOY_ENV=prod (skipped in this demo)

```bash {when=$DEPLOY_ENV=prod}
echo "prod-only deploy step"
```

## Runs when DEPLOY_ENV is not prod (runs in this demo)

```bash {when=$DEPLOY_ENV!=prod}
echo "non-prod: running integration smoke"
```

## Skipped if DRY_RUN is truthy (unset → runs)

```bash {skip_if=$DRY_RUN}
echo "performing the real action"
```

## Combined: run only when DEPLOY_ENV is set AND DRY_RUN is not

```bash {when=$DEPLOY_ENV, skip_if=$DRY_RUN}
echo "env is set and this is not a dry run"
```

## Negation: run only when DRY_RUN is *not* truthy

```bash {when=!$DRY_RUN}
echo "running live"
```

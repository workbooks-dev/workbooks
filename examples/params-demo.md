---
title: Typed parameters + profiles
runtime: bash
params:
  region:
    type: enum
    one_of: [us-east-1, eu-west-1, ap-south-1]
    default: us-east-1
    description: Target deployment region
  replicas:
    type: int
    default: 2
  dry_run:
    type: bool
    default: true
  # Shorthand form: a bare scalar is the default (type inferred as string).
  service: api
profiles:
  prod:
    region: eu-west-1
    replicas: 6
    dry_run: false
  staging:
    region: ap-south-1
    replicas: 2
---

# Typed parameters + profiles

Run this with declared defaults, override individual values, or apply a named
profile:

```text
wb run params-demo.md                              # all defaults
wb run params-demo.md --param replicas=10          # override one value
wb run params-demo.md --profile prod               # apply the prod preset
wb run params-demo.md --profile prod --param dry_run=true   # preset + override
wb run params-demo.md --param-file ./values.yaml   # values from a YAML file
```

Resolution precedence (highest first): `--param` > `--param-file` > `--profile`
> the declared `default`. Each value is validated against its declared `type`
(`string` | `int` | `bool` | `enum`) and `one_of` choices before any block runs.
A bad value, an undeclared `--param`, or a missing `required:` param is a usage
error (exit 2).

Resolved params are injected into every cell's env under their bare name and are
visible to `{when=}` / `{skip_if=}`:

```bash
echo "deploying $service x$replicas to $region (dry_run=$dry_run)"
```

```bash {skip_if=$dry_run}
echo "this only runs when dry_run is false (e.g. --profile prod)"
```

The resolved parameter set is hashed into the checkpoint identity, so resuming a
checkpointed run with different params starts fresh instead of mixing state, and
`wb resume` re-applies the original params automatically.

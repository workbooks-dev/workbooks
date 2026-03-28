---
title: Secrets Demo
runtime: bash
secrets:
  provider: env
  keys: [HOME, USER, SHELL]
---

# Working with Secrets

This workbook demonstrates secret injection. Secrets from the configured
provider are injected as environment variables before execution.

## Using injected secrets

```bash
echo "User: $USER"
echo "Home: $HOME"
echo "Shell: $SHELL"
```

## Override secrets from the CLI

You can override the frontmatter secrets provider from the command line:

- `wb run secrets-demo.md --secrets doppler --project my-project`
- `wb run secrets-demo.md --secrets yard --secrets-cmd "yard env get"`
- `wb run secrets-demo.md --secrets prompt` (interactive)
- `wb run secrets-demo.md --secrets dotenv --secrets-cmd .env.local`

These are all equivalent to configuring in the frontmatter:

```yaml
secrets:
  provider: doppler
  project: my-project
```

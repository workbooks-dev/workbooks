---
title: Browser Runtime Demo
---

# Browser Runtime Demo

Exercises the `browser` runtime end-to-end. Requires the `wb-browser-runtime`
sidecar on `$PATH` (or pointed to via `WB_BROWSER_RUNTIME=...`) and the
`WB_EXPERIMENTAL_BROWSER=1` flag.

```bash
echo "pre-check: this runs on the host"
```

## Mail check

```browser
session: ipostal1
verbs:
  - goto: https://app.ipostal1.com
  - click: "button.sign-in"
  - fill:
      selector: "input[name=email]"
      value: "ops@example.com"
  - screenshot: inbox.png
```

## Post-processing

```bash
echo "post-check: still on the host — sidecar kept alive between browser slices"
```

```browser
session: ipostal1
verbs:
  - click: "tr.unread:first-child"
  - download:
      selector: "a.attachment"
      path: ./downloads/
```

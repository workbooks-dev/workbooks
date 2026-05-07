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

## Downloads (auto-captured)

Anything the browser downloads — a click on an attachment, a redirect that
ends in a file, a popup that triggers a Save As — lands in
`$WB_ARTIFACTS_DIR` automatically. There's no `download:` verb to call;
the runbook just clicks, and the sidecar's context-level listener catches
the resulting `download` event, streams the bytes back over CDP (for
cloud providers), and emits a `slice.artifact_saved` frame so wb's
artifact uploader picks it up.

```browser
session: ipostal1
verbs:
  - click: "tr.unread:first-child"
  - click: "a.attachment"     # whatever this download is, it's captured
```

To filter what gets kept, set an extension allowlist via env (e.g. in the
workbook's `env:` frontmatter):

```yaml
env:
  WB_BROWSER_DOWNLOAD_EXTENSIONS: pdf,xlsx,csv,docx
```

Skipped downloads still emit a `slice.download_skipped` frame so you can
see what was discarded.

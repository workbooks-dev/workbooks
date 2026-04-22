// Minimal in-memory fake of the Playwright `Page` API that the verbs
// exercise. Every method the verb registry touches is stubbed here; tests
// configure return values via the factory's options and assert against the
// recorded `calls` log.
//
// Intentionally does NOT simulate real browser behavior — a verb that
// would throw against a real page (e.g. `fill` on a non-existent selector)
// resolves cleanly here. Tests asserting error paths use options like
// `handles: { "#nope": null }` to wire explicit misses.

export function createStubPage(opts = {}) {
  const calls = [];
  let currentUrl = opts.initialUrl ?? "about:blank";

  const screenshotBuf =
    opts.screenshotBuf ?? Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a]);
  const extractResult = opts.extractResult ?? [];
  const evalResult = opts.evalResult ?? null;
  // handles: selector -> { textContent } | null (null = selector not found).
  // Missing keys (undefined) also count as "not found" so tests only need
  // to wire what matters.
  const handles = opts.handles ?? {};

  const record = (call) => {
    calls.push(call);
    return call;
  };

  return {
    calls,
    _setUrl(u) {
      currentUrl = u;
    },
    url() {
      return currentUrl;
    },

    async goto(url, options) {
      record({ verb: "goto", url, options });
      currentUrl = url;
    },
    async fill(selector, value, options) {
      record({ verb: "fill", selector, value, options });
    },
    async click(selector, options) {
      record({ verb: "click", selector, options });
    },
    async press(selector, key, options) {
      record({ verb: "press", selector, key, options });
    },
    async waitForSelector(selector, options) {
      record({ verb: "waitForSelector", selector, options });
    },
    async screenshot(options) {
      record({ verb: "screenshot", options });
      return screenshotBuf;
    },
    async $$eval(selector, fn, fieldSpec) {
      record({ verb: "$$eval", selector, fieldSpec });
      return extractResult;
    },
    async $(selector) {
      record({ verb: "$", selector });
      const h = handles[selector];
      if (h == null) return null;
      return {
        async textContent() {
          return h.textContent ?? null;
        },
      };
    },
    async evaluate(script) {
      record({ verb: "evaluate", script });
      return evalResult;
    },
  };
}

// Capture JSON frames written via lib/io.js `send` during a test. Returns
// a disposer that restores `process.stdout.write`. Non-JSON writes pass
// through, so node:test's own reporter output (spec/tap/text) is still
// visible. Use inside a test with `t.after(disposer)`.
export function captureSendFrames() {
  const real = process.stdout.write.bind(process.stdout);
  const frames = [];
  process.stdout.write = (chunk, encoding, cb) => {
    const str = typeof chunk === "string" ? chunk : chunk?.toString?.();
    // Single JSON line ending in \n matches the shape of lib/io.js send()
    // writes. Non-JSON or multi-line writes fall through to the real stdout
    // so node:test's reporter output isn't swallowed.
    if (
      str &&
      str.startsWith("{") &&
      str.endsWith("\n") &&
      str.indexOf("\n") === str.length - 1
    ) {
      try {
        frames.push(JSON.parse(str.trim()));
        if (typeof encoding === "function") encoding();
        else if (typeof cb === "function") cb();
        return true;
      } catch {
        // Fall through to real write — wasn't actually a send() frame.
      }
    }
    return real(chunk, encoding, cb);
  };
  const dispose = () => {
    process.stdout.write = real;
  };
  return { frames, dispose };
}

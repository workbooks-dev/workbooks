import { log } from "./io.js";

export async function safeText(res) {
  try {
    return (await res.text()).slice(0, 200);
  } catch {
    return "<unreadable>";
  }
}

// Retry transient network + 5xx/429 failures with short exponential backoff.
// Each attempt gets its own AbortController + timeout; caller-passed signals
// are not plumbed through since we don't have a cancellation story above this
// layer. Non-retryable statuses (4xx except 429) are returned immediately for
// the caller to handle.
//
// `bodyFactory`, when set, is invoked per attempt to produce a fresh body —
// required for streaming uploads where the previous attempt consumed the
// stream. Takes precedence over opts.body.
export async function retryableFetch(
  url,
  opts = {},
  label,
  { timeoutMs = 30_000, bodyFactory = null } = {},
) {
  const delays = [100, 500];
  let lastErr = null;
  let lastRes = null;
  for (let attempt = 0; attempt <= delays.length; attempt++) {
    if (attempt > 0) {
      await new Promise((r) => setTimeout(r, delays[attempt - 1]));
      const prev = lastRes
        ? `status=${lastRes.status}`
        : `err=${lastErr?.message || lastErr}`;
      log(`[retry] ${label} attempt ${attempt + 1}/3 (${prev})`);
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const fetchOpts = { ...opts, signal: controller.signal };
      if (bodyFactory) {
        fetchOpts.body = bodyFactory();
        // undici requires duplex: "half" for streaming (non-Buffer, non-string)
        // request bodies. Omitting it throws at request time.
        fetchOpts.duplex = "half";
      }
      const res = await fetch(url, fetchOpts);
      if (res.ok) return res;
      if (res.status === 429 || (res.status >= 500 && res.status < 600)) {
        lastRes = res;
        continue;
      }
      return res;
    } catch (e) {
      lastErr = e;
      continue;
    } finally {
      clearTimeout(timer);
    }
  }
  if (lastRes) return lastRes;
  throw lastErr;
}

// browser-use cloud provider.
//
// Two differences vs. Browserbase worth knowing:
//   1. `POST /api/v3/browsers` returns `cdpUrl` AND `liveUrl` in one call —
//      no separate debug fetch. We stash liveUrl on the allocated handle so
//      getLiveUrl() is a sync property read, and the timing buckets in
//      slice.session_started stay shaped the same way (just with ~0ms
//      connect-phase fetch on this vendor).
//   2. Stealth + proxies are on by default (proxyCountryCode defaults to
//      "us"); no Scale-plan gate. Set BROWSER_USE_PROXY_COUNTRY=null to
//      disable the proxy, or to e.g. "gb" to route elsewhere.
//
// Release uses PATCH with action:"stop" (the documented update-session
// endpoint). "Unused time automatically refunded if the session ran less
// than 1 hour" per their docs, so early teardown on sidecar shutdown is
// real savings, not just quota cleanup.
//
// Not yet wired: customProxy, browserScreenWidth/Height, allowResizing,
// enableRecording (we have our own rrweb + screencast pipeline; enabling
// theirs would double-record).

import { retryableFetch, safeText } from "../http.js";
import { log } from "../io.js";

const BU_BASE = "https://api.browser-use.com/api/v3";

export function createBrowserUseProvider() {
  return {
    name: "browser-use",

    async allocate({ profile, sessionName: _sessionName } = {}) {
      const apiKey = process.env.BROWSER_USE_API_KEY;
      if (!apiKey) {
        throw new Error("BROWSER_USE_API_KEY must be set");
      }

      // Profile id is an opaque UUID from the human-driven `profile.sh`
      // bootstrap, baked into the runbook frontmatter by whatever generates
      // it (UI editor, codegen, hand-authored). The slice envelope carries
      // it through; this provider just forwards.
      const profileId = profile ?? null;

      const body = {};
      if (profileId) body.profileId = profileId;

      // Pass-through knobs. Only include when the operator set them — server
      // defaults (proxy=us, timeout=60min) are better behaved than anything
      // we'd invent here.
      const proxyCountry = process.env.BROWSER_USE_PROXY_COUNTRY;
      if (proxyCountry !== undefined) {
        // Explicit "null" string disables the proxy entirely.
        body.proxyCountryCode =
          proxyCountry.toLowerCase() === "null" ? null : proxyCountry;
      }
      const timeoutMin = Number.parseInt(
        process.env.BROWSER_USE_TIMEOUT_MIN || "",
        10,
      );
      if (Number.isFinite(timeoutMin) && timeoutMin > 0) {
        body.timeout = timeoutMin;
      }

      log(
        `[bu] session create profile=${profileId ?? "<none>"} proxy=${body.proxyCountryCode ?? "<default>"} timeout=${body.timeout ?? "<default>"}m`,
      );

      const res = await retryableFetch(
        `${BU_BASE}/browsers`,
        {
          method: "POST",
          headers: {
            "X-Browser-Use-API-Key": apiKey,
            "Content-Type": "application/json",
          },
          body: JSON.stringify(body),
        },
        "bu.create",
      );
      if (!res.ok) {
        throw new Error(
          `browser-use create failed (${res.status}): ${await safeText(res)}`,
        );
      }
      const created = await res.json();
      if (!created.cdpUrl) {
        throw new Error(
          `browser-use create returned no cdpUrl (status=${created.status ?? "?"}); session unusable`,
        );
      }
      return {
        sid: created.id,
        cdpUrl: created.cdpUrl,
        // Stashed so getLiveUrl() below is a property read, not a round-trip.
        _liveUrl: created.liveUrl ?? null,
      };
    },

    async getLiveUrl(allocated) {
      return allocated._liveUrl;
    },

    async release(sid) {
      const apiKey = process.env.BROWSER_USE_API_KEY;
      try {
        const res = await retryableFetch(
          `${BU_BASE}/browsers/${sid}`,
          {
            method: "PATCH",
            headers: {
              "X-Browser-Use-API-Key": apiKey,
              "Content-Type": "application/json",
            },
            body: JSON.stringify({ action: "stop" }),
          },
          "bu.release",
        );
        if (!res.ok) {
          // retryableFetch returns 4xx silently (only 5xx/429 are retried).
          // Surface non-2xx loudly — a 400/404 here means the session is
          // still alive and quota is still burning, which is the bug we
          // were silently masking before.
          log(
            `[bu] release session ${sid} returned HTTP ${res.status}: ${await safeText(res)}`,
          );
          return;
        }
        log(`[bu] released session ${sid}`);
      } catch (e) {
        log(`[bu] release session ${sid} failed: ${e.message}`);
      }
    },
  };
}

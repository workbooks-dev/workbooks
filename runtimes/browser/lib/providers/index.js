// Browser-session provider boundary.
//
// All vendor-specific work (session allocation, live-preview URL, release)
// lives behind a provider. Everything else in the sidecar — verbs, recording,
// session cache, substitutions — runs against a Playwright Page and doesn't
// care which vendor handed us the CDP endpoint.
//
// Provider interface:
//   {
//     name: string,
//     async allocate({ profile, sessionName }) -> { sid, cdpUrl, ...opaque },
//     async getLiveUrl(allocated) -> string,
//     async release(sid) -> void,
//   }
//
// Two-phase allocate/getLiveUrl split exists so vendors that return the live
// URL in the same call as session create (browser-use) can just stash it on
// the `allocated` object and return it synchronously from getLiveUrl —
// while vendors that require a second round-trip (browserbase) preserve
// today's timing buckets (allocate_ms vs. connect_ms in slice.session_started).
//
// Vendor selection is a single env var, resolved once at sidecar boot:
//   WB_BROWSER_VENDOR=browserbase (default)
//   WB_BROWSER_VENDOR=browser-use (future)

import { createBrowserbaseProvider } from "./browserbase.js";
import { createBrowserUseProvider } from "./browser-use.js";

export function getProvider() {
  const raw = (process.env.WB_BROWSER_VENDOR || "browserbase")
    .trim()
    .toLowerCase();
  switch (raw) {
    case "browserbase":
      return createBrowserbaseProvider();
    case "browser-use":
      return createBrowserUseProvider();
    default:
      throw new Error(
        `WB_BROWSER_VENDOR="${raw}" is not a known vendor (expected: browserbase | browser-use)`,
      );
  }
}

// Browserbase provider. Three REST calls wrapped behind the provider
// interface (allocate / getLiveUrl / release). Extracted from the sidecar
// entry point so browser-use (and future vendors) can slot in via the same
// shape without the verb/recording pipeline caring which chromium it's
// driving.

import { retryableFetch, safeText } from "../http.js";
import { log } from "../io.js";

const BB_BASE = "https://api.browserbase.com";

function envBool(v) {
  return v === "1" || (typeof v === "string" && v.toLowerCase() === "true");
}

export function createBrowserbaseProvider() {
  return {
    name: "browserbase",

    async allocate({ profile, sessionName: _sessionName } = {}) {
      const apiKey = process.env.BROWSERBASE_API_KEY;
      const projectId = process.env.BROWSERBASE_PROJECT_ID;
      if (!apiKey || !projectId) {
        throw new Error(
          "BROWSERBASE_API_KEY and BROWSERBASE_PROJECT_ID must be set",
        );
      }

      if (profile) {
        // Browserbase has "contexts" as its cross-session persistence concept
        // but wb doesn't thread them yet. Log so operators see the `profile:`
        // field arrived but had no effect — easier to debug than silent drop.
        log(
          `[bb] profile="${profile}" ignored — browserbase vendor has no profile binding yet`,
        );
      }

      // advancedStealth is Scale-plan-gated on Browserbase's side; proxies
      // adds residential-IP cost. Default off so a misconfigured plan doesn't
      // break unrelated runs; flip per vendor when the target sits behind
      // Cloudflare / similar bot detection.
      const advancedStealth = envBool(process.env.BROWSERBASE_ADVANCED_STEALTH);
      const proxies = envBool(process.env.BROWSERBASE_PROXIES);

      // keepAlive:false — slice lifetime is tied to wb process; on shutdown
      // we explicitly REQUEST_RELEASE so quota isn't burned by orphans.
      const body = { projectId, keepAlive: false };
      if (advancedStealth) body.browserSettings = { advancedStealth: true };
      if (proxies) body.proxies = true;

      log(
        `[bb] session create advancedStealth=${advancedStealth} proxies=${proxies}`,
      );

      const res = await retryableFetch(
        `${BB_BASE}/v1/sessions`,
        {
          method: "POST",
          headers: {
            "X-BB-API-Key": apiKey,
            "Content-Type": "application/json",
          },
          body: JSON.stringify(body),
        },
        "bb.create",
      );
      if (!res.ok) {
        throw new Error(
          `Browserbase create failed (${res.status}): ${await safeText(res)}`,
        );
      }
      const created = await res.json();
      return { sid: created.id, cdpUrl: created.connectUrl };
    },

    async getLiveUrl(allocated) {
      const apiKey = process.env.BROWSERBASE_API_KEY;
      const res = await retryableFetch(
        `${BB_BASE}/v1/sessions/${allocated.sid}/debug`,
        { headers: { "X-BB-API-Key": apiKey } },
        "bb.debug",
      );
      if (!res.ok) {
        throw new Error(
          `Browserbase debug fetch failed (${res.status}): ${await safeText(res)}`,
        );
      }
      const body = await res.json();
      return body.debuggerFullscreenUrl;
    },

    async release(sid) {
      const apiKey = process.env.BROWSERBASE_API_KEY;
      const projectId = process.env.BROWSERBASE_PROJECT_ID;
      try {
        const res = await retryableFetch(
          `${BB_BASE}/v1/sessions/${sid}`,
          {
            method: "POST",
            headers: {
              "X-BB-API-Key": apiKey,
              "Content-Type": "application/json",
            },
            body: JSON.stringify({ projectId, status: "REQUEST_RELEASE" }),
          },
          "bb.release",
        );
        if (!res.ok) {
          log(
            `[bb] release session ${sid} returned HTTP ${res.status}: ${await safeText(res)}`,
          );
          return;
        }
        log(`[bb] released session ${sid}`);
      } catch (e) {
        log(`[bb] release session ${sid} failed: ${e.message}`);
      }
    },
  };
}

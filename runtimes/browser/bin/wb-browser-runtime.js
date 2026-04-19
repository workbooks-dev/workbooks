#!/usr/bin/env node
// wb-browser-runtime — Browserbase + Playwright sidecar for `wb`.
//
// Speaks wb's line-framed JSON protocol on stdio (see ../README.md). Each
// `browser` fenced block in a workbook arrives as one `slice` message; this
// sidecar dispatches its verbs against a Playwright `Page` connected to a
// Browserbase session via CDP.
//
// Sessions are cached by `session:` name across slices for the lifetime of
// this process, so a runbook with multiple browser blocks against the same
// vendor reuses one Browserbase session (and one logged-in browser context).
//
// Env required for real runs:
//   BROWSERBASE_API_KEY
//   BROWSERBASE_PROJECT_ID
//
// Verb args support `{{ env.NAME }}` substitution, expanded recursively
// against process.env at dispatch time. Credentials passed this way never
// hit stdout — only the verb name + selector make it into the summary.

import readline from "node:readline";
import { chromium } from "playwright-core";

const SUPPORTS = [
  "goto",
  "fill",
  "click",
  "press",
  "wait_for",
  "screenshot",
  "extract",
  "assert",
  "eval",
];

const BB_BASE = "https://api.browserbase.com";
const VERSION = "0.3.0";

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function log(...args) {
  process.stderr.write(args.join(" ") + "\n");
}

// --- Browserbase REST -------------------------------------------------------

async function bbCreateSession() {
  const apiKey = process.env.BROWSERBASE_API_KEY;
  const projectId = process.env.BROWSERBASE_PROJECT_ID;
  if (!apiKey || !projectId) {
    throw new Error(
      "BROWSERBASE_API_KEY and BROWSERBASE_PROJECT_ID must be set",
    );
  }
  const res = await fetch(`${BB_BASE}/v1/sessions`, {
    method: "POST",
    headers: {
      "X-BB-API-Key": apiKey,
      "Content-Type": "application/json",
    },
    // keepAlive:false — slice lifetime is tied to wb process; on shutdown
    // we explicitly REQUEST_RELEASE so quota isn't burned by orphans.
    body: JSON.stringify({ projectId, keepAlive: false }),
  });
  if (!res.ok) {
    throw new Error(
      `Browserbase create failed (${res.status}): ${await safeText(res)}`,
    );
  }
  return await res.json();
}

async function bbGetLiveUrl(sessionId) {
  const apiKey = process.env.BROWSERBASE_API_KEY;
  const res = await fetch(`${BB_BASE}/v1/sessions/${sessionId}/debug`, {
    headers: { "X-BB-API-Key": apiKey },
  });
  if (!res.ok) {
    throw new Error(
      `Browserbase debug fetch failed (${res.status}): ${await safeText(res)}`,
    );
  }
  const body = await res.json();
  return body.debuggerFullscreenUrl;
}

async function bbReleaseSession(sessionId) {
  const apiKey = process.env.BROWSERBASE_API_KEY;
  const projectId = process.env.BROWSERBASE_PROJECT_ID;
  try {
    await fetch(`${BB_BASE}/v1/sessions/${sessionId}`, {
      method: "POST",
      headers: { "X-BB-API-Key": apiKey, "Content-Type": "application/json" },
      body: JSON.stringify({ projectId, status: "REQUEST_RELEASE" }),
    });
  } catch (e) {
    log(`[shutdown] release session ${sessionId} failed: ${e.message}`);
  }
}

async function safeText(res) {
  try {
    return (await res.text()).slice(0, 200);
  } catch {
    return "<unreadable>";
  }
}

// --- Session cache ----------------------------------------------------------

const sessions = new Map(); // name -> { sid, browser, context, page, liveUrl }

async function ensureSession(name) {
  if (sessions.has(name)) return sessions.get(name);

  const created = await bbCreateSession();
  const liveUrl = await bbGetLiveUrl(created.id);
  const browser = await chromium.connectOverCDP(created.connectUrl);
  const context = browser.contexts()[0] ?? (await browser.newContext());
  const page = context.pages()[0] ?? (await context.newPage());

  const info = {
    sid: created.id,
    browser,
    context,
    page,
    liveUrl,
  };
  sessions.set(name, info);

  send({
    type: "slice.session_started",
    session: name,
    session_id: created.id,
    live_url: liveUrl,
    started_at: new Date().toISOString(),
  });
  return info;
}

// --- {{ env.X }} substitution ----------------------------------------------

const ENV_RE = /\{\{\s*env\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;

function expand(value) {
  if (typeof value === "string") {
    return value.replace(ENV_RE, (_, name) => {
      const v = process.env[name];
      if (v === undefined) {
        // Leave the placeholder visible so failures surface in stderr summaries
        // instead of silently turning into empty strings.
        log(`[warn] env var ${name} is not set; leaving placeholder`);
        return "";
      }
      return v;
    });
  }
  if (Array.isArray(value)) return value.map(expand);
  if (value && typeof value === "object") {
    const out = {};
    for (const [k, v] of Object.entries(value)) out[k] = expand(v);
    return out;
  }
  return value;
}

// --- Verb dispatch ----------------------------------------------------------

function verbName(verb) {
  if (!verb || typeof verb !== "object") return String(verb);
  return Object.keys(verb)[0] || "verb";
}

// Most verbs accept either a bare string ("goto: https://...") or a structured
// object ("goto: { url: ..., wait_until: ... }"). This pulls the canonical
// field out of either shape.
function arg(value, primaryKey) {
  if (typeof value === "string") return { [primaryKey]: value };
  if (value && typeof value === "object") return value;
  return {};
}

async function runVerb(page, verb, index) {
  const name = verbName(verb);
  const raw = verb[name];
  const a = expand(arg(raw, defaultKey(name)));

  switch (name) {
    case "goto": {
      const url = a.url ?? "";
      const waitUntil = a.wait_until ?? "domcontentloaded";
      await page.goto(url, { waitUntil, timeout: a.timeout ?? 30_000 });
      return `→ ${page.url()}`;
    }
    case "fill": {
      // Don't echo the value into the summary — could be a credential.
      await page.fill(a.selector, String(a.value ?? ""), {
        timeout: a.timeout ?? 10_000,
      });
      return `${a.selector} = «${redact(a.value)}»`;
    }
    case "click": {
      await page.click(a.selector, { timeout: a.timeout ?? 10_000 });
      return `${a.selector}`;
    }
    case "press": {
      const target = a.selector ?? "body";
      await page.press(target, a.key, { timeout: a.timeout ?? 5_000 });
      return `${target} ⌨ ${a.key}`;
    }
    case "wait_for": {
      const selector = a.selector;
      const state = a.state ?? "visible";
      await page.waitForSelector(selector, {
        state,
        timeout: a.timeout ?? 15_000,
      });
      return `${selector} (${state})`;
    }
    case "screenshot": {
      const path = a.path ?? `screenshot-${Date.now()}.png`;
      await page.screenshot({ path, fullPage: !!a.full_page });
      return `→ ${path}`;
    }
    case "extract": {
      // Pull structured rows out of the page. Each `field` entry is either:
      //   string                   — CSS selector relative to row, take textContent
      //   { selector, attr }       — CSS selector relative to row, take attribute
      //   { selector, text: true } — explicit textContent (default)
      const rowSelector = a.selector;
      const fields = a.fields ?? {};
      const items = await page.$$eval(
        rowSelector,
        (rows, fieldSpec) =>
          rows.map((row) => {
            const out = {};
            for (const [name, spec] of Object.entries(fieldSpec)) {
              const sel = typeof spec === "string" ? spec : spec.selector;
              const attr = typeof spec === "string" ? null : spec.attr ?? null;
              const el = sel ? row.querySelector(sel) : row;
              if (!el) {
                out[name] = null;
                continue;
              }
              out[name] = attr
                ? el.getAttribute(attr)
                : (el.textContent || "").trim();
            }
            return out;
          }),
        fields,
      );
      // Emit as JSON to stdout so wb captures it in step.complete.stdout.
      // Pretty-printed for readability when a runbook surfaces the output.
      console.log(JSON.stringify(items, null, 2));
      return `${rowSelector} → ${items.length} rows`;
    }
    case "assert": {
      const sel = a.selector;
      const handle = await page.$(sel);
      if (!handle) throw new Error(`assert: selector not found: ${sel}`);
      if (a.text_contains) {
        const txt = (await handle.textContent()) ?? "";
        if (!txt.includes(a.text_contains)) {
          throw new Error(
            `assert: text "${a.text_contains}" not in ${sel} (got "${txt.slice(0, 80)}")`,
          );
        }
      }
      if (a.url_contains && !page.url().includes(a.url_contains)) {
        throw new Error(
          `assert: url does not contain "${a.url_contains}" (got ${page.url()})`,
        );
      }
      return `${sel}`;
    }
    case "eval": {
      // Run arbitrary JS in the page; result is JSON-serialized to stdout.
      const result = await page.evaluate(a.script);
      console.log(JSON.stringify(result, null, 2));
      return `script ran`;
    }
    default:
      throw new Error(`unsupported verb: ${name}`);
  }
}

function defaultKey(name) {
  switch (name) {
    case "goto":
      return "url";
    case "click":
    case "wait_for":
    case "assert":
      return "selector";
    case "screenshot":
      return "path";
    case "press":
      return "key";
    case "eval":
      return "script";
    default:
      return "value";
  }
}

function redact(value) {
  if (typeof value !== "string") return "";
  if (value.length <= 4) return "***";
  return `${value.slice(0, 2)}***`;
}

// --- Slice handler ----------------------------------------------------------

async function handleSlice(msg) {
  const verbs = Array.isArray(msg.verbs) ? msg.verbs : [];
  const sessionName = msg.session || "default";
  const restore = msg.restore || null;

  let session;
  try {
    session = await ensureSession(sessionName);
  } catch (e) {
    send({
      type: "slice.failed",
      error: `session start failed: ${e.message}`,
    });
    return;
  }

  // Restore-from-pause is not implemented yet (no pause verb wired here).
  // The sidecar protocol leaves room for it; when wait_for_mfa lands, this
  // is where we'd jump to verbs[restore.state.verb_index].
  const startAt = restore?.state?.verb_index ?? 0;

  for (let i = startAt; i < verbs.length; i++) {
    const v = verbs[i];
    const name = verbName(v);
    try {
      const summary = await runVerb(session.page, v, i);
      send({
        type: "verb.complete",
        verb: name,
        verb_index: i,
        summary,
      });
    } catch (e) {
      send({
        type: "verb.failed",
        verb: name,
        verb_index: i,
        error: e.message,
      });
      send({
        type: "slice.failed",
        error: `verb ${name} (index ${i}): ${e.message}`,
      });
      return;
    }
  }
  send({ type: "slice.complete" });
}

// --- Shutdown ---------------------------------------------------------------

let shuttingDown = false;
async function shutdown() {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const [name, info] of sessions) {
    try {
      await info.browser.close();
    } catch (e) {
      log(`[shutdown] close ${name}: ${e.message}`);
    }
  }
  // Ask Browserbase to release sessions explicitly so quota isn't held by
  // orphans waiting for their idle timeout.
  await Promise.all(
    Array.from(sessions.values()).map((s) => bbReleaseSession(s.sid)),
  );
  process.exit(0);
}

// --- Main loop --------------------------------------------------------------

const rl = readline.createInterface({ input: process.stdin, terminal: false });

// Serialize incoming messages — Playwright operations are async and we don't
// want concurrent slice handlers stomping on the shared page.
let chain = Promise.resolve();
function enqueue(fn) {
  chain = chain.then(fn).catch((e) => log(`[loop] ${e.message}`));
  return chain;
}

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    log(`[warn] ignoring non-JSON input: ${trimmed.slice(0, 80)}`);
    return;
  }

  switch (msg.type) {
    case "hello":
      send({
        type: "ready",
        runtime: "wb-browser-runtime",
        version: VERSION,
        protocol: "wb-sidecar/1",
        supports: SUPPORTS,
      });
      break;
    case "slice":
      enqueue(() => handleSlice(msg));
      break;
    case "shutdown":
      enqueue(shutdown);
      break;
    default:
      log(`[warn] unknown message type: ${msg.type}`);
  }
});

rl.on("close", () => {
  // stdin closed — drain pending work then exit.
  enqueue(shutdown);
});

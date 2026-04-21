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
// Verb args support two substitutions, expanded recursively at dispatch time:
//   {{ env.NAME }}        → process.env.NAME
//   {{ artifacts.NAME }}  → contents of $WB_ARTIFACTS_DIR/NAME.txt (or .../NAME)
// The artifacts form lets an earlier bash cell compute a value — OTP, magic
// link, export id — and feed it into a later browser verb without a sidecar
// round-trip. Credentials passed via either form never hit stdout — only the
// verb name + selector make it into the summary.

import readline from "node:readline";
import { chromium } from "playwright-core";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync, promises as fsPromises } from "node:fs";
import { randomUUID } from "node:crypto";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";
import { promisify } from "node:util";

const gzip = promisify(zlib.gzip);

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
  "save",
];

const BB_BASE = "https://api.browserbase.com";
const VERSION = "0.5.0";

// --- Recording config -------------------------------------------------------
//
// Feature is off unless WB_RECORDING_UPLOAD_URL is set. When enabled, every
// session gets rrweb DOM-event capture and/or a CDP screencast video; both
// artifacts are POSTed to the upload URL at session close.
//
// URL template supports `{run_id}` and `{kind}` placeholders, e.g.
//   https://host/api/runs/{run_id}/recording/{kind}
// kind ∈ {"rrweb", "video"}. Auth: `Authorization: Bearer <SECRET>`.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const RRWEB_VENDOR_PATH = path.join(
  __dirname,
  "..",
  "vendor",
  "rrweb-record.min.js",
);

function checkFfmpeg() {
  try {
    const res = spawnSync("ffmpeg", ["-version"], { stdio: "ignore" });
    return res.status === 0;
  } catch {
    return false;
  }
}

function loadRecordingConfig() {
  const uploadUrl = (process.env.WB_RECORDING_UPLOAD_URL || "").trim();
  if (!uploadUrl) return { enabled: false, reason: "no-upload-url" };
  const secret = (process.env.WB_RECORDING_UPLOAD_SECRET || "").trim();
  if (!secret) {
    log(
      "[recording] WB_RECORDING_UPLOAD_URL is set but WB_RECORDING_UPLOAD_SECRET is empty — refusing to upload unauthenticated. Recording disabled.",
    );
    return { enabled: false, reason: "no-secret" };
  }

  const runId =
    (process.env.WB_RECORDING_RUN_ID || "").trim() ||
    (process.env.TRIGGER_RUN_ID || "").trim() ||
    `wb-${randomUUID()}`;

  const fps =
    Number.parseInt(process.env.WB_RECORDING_SCREENCAST_FPS || "", 10) || 5;
  const quality =
    Number.parseInt(process.env.WB_RECORDING_SCREENCAST_QUALITY || "", 10) ||
    60;

  const rrwebRequested = process.env.WB_RECORDING_RRWEB !== "0";
  const videoRequested = process.env.WB_RECORDING_VIDEO !== "0";

  let rrwebSource = null;
  if (rrwebRequested) {
    if (!existsSync(RRWEB_VENDOR_PATH)) {
      log(
        `[recording] rrweb vendor file missing at ${RRWEB_VENDOR_PATH} — disabling rrweb capture`,
      );
    } else {
      rrwebSource = readFileSync(RRWEB_VENDOR_PATH, "utf8");
    }
  }

  const hasFfmpeg = videoRequested ? checkFfmpeg() : false;
  if (videoRequested && !hasFfmpeg) {
    log(
      "[recording] ffmpeg not found on $PATH — disabling video capture (rrweb will continue if enabled)",
    );
  }

  const kinds = {
    rrweb: rrwebRequested && !!rrwebSource,
    video: videoRequested && hasFfmpeg,
  };

  if (!kinds.rrweb && !kinds.video) {
    log("[recording] no usable kinds — recording disabled");
    return { enabled: false, reason: "all-kinds-disabled" };
  }

  return {
    enabled: true,
    uploadUrl,
    secret,
    runId,
    fps,
    quality,
    kinds,
    rrwebSource,
  };
}

const RECORDING = loadRecordingConfig();
if (RECORDING.enabled) {
  const activeKinds = Object.entries(RECORDING.kinds)
    .filter(([, v]) => v)
    .map(([k]) => k)
    .join(",");
  log(
    `[recording] enabled run_id=${RECORDING.runId} kinds=${activeKinds} fps=${RECORDING.fps} quality=${RECORDING.quality}`,
  );
}

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

  // Both flags opt-in per session. advancedStealth is Scale-plan-gated on
  // Browserbase's side; proxies adds residential-IP cost. Default off so a
  // misconfigured plan doesn't break unrelated runs (HN, Google Sheets, etc.);
  // flip per vendor when the target sits behind Cloudflare / similar bot
  // detection (e.g., Airbase).
  const envBool = (v) => v === "1" || (typeof v === "string" && v.toLowerCase() === "true");
  const advancedStealth = envBool(process.env.BROWSERBASE_ADVANCED_STEALTH);
  const proxies = envBool(process.env.BROWSERBASE_PROXIES);

  // keepAlive:false — slice lifetime is tied to wb process; on shutdown
  // we explicitly REQUEST_RELEASE so quota isn't burned by orphans.
  const body = { projectId, keepAlive: false };
  if (advancedStealth) {
    body.browserSettings = { advancedStealth: true };
  }
  if (proxies) {
    body.proxies = true;
  }

  log(`[bb] session create advancedStealth=${advancedStealth} proxies=${proxies}`);

  const res = await fetch(`${BB_BASE}/v1/sessions`, {
    method: "POST",
    headers: {
      "X-BB-API-Key": apiKey,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
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

const sessions = new Map(); // name -> { sid, browser, context, page, liveUrl, recording }

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
    recording: null,
  };
  sessions.set(name, info);

  send({
    type: "slice.session_started",
    session: name,
    session_id: created.id,
    live_url: liveUrl,
    started_at: new Date().toISOString(),
  });

  await startRecording(info, name);
  return info;
}

// --- Recording (rrweb + CDP screencast) ------------------------------------
//
// rrweb  — vendored record bundle injected via context.addInitScript. Events
//          are emitted to window.__wbRrwebBuffer and flushed every 500ms (and
//          on beforeunload) to a sidecar-side buffer via exposeBinding. This
//          survives cross-origin navigations because the init script reruns on
//          every new document.
// video  — per-page CDPSession.startScreencast streams JPEG frames; each frame
//          is piped into a long-lived `ffmpeg` subprocess that encodes to VP9
//          WebM on disk. At session end we close the stdin, wait for ffmpeg to
//          exit, and read the file.
//
// Both artifacts are POSTed with Bearer auth to the upload URL. Failure is
// soft — slice.recording.failed events are emitted but the run still succeeds.

async function startRecording(info, sessionName) {
  if (!RECORDING.enabled) return;
  info.recording = {
    kinds: { ...RECORDING.kinds },
    rrwebEvents: [],
    cdp: null,
    ffmpeg: null,
    ffmpegDone: null,
    videoPath: null,
  };
  const rec = info.recording;

  if (rec.kinds.rrweb) {
    try {
      await info.context.exposeBinding("__wbRrwebFlush", (_src, batch) => {
        if (Array.isArray(batch)) {
          for (const e of batch) rec.rrwebEvents.push(e);
        }
      });
      const bootstrap = `
;(function(){
  if (window.__wbRrwebActive) return;
  window.__wbRrwebActive = true;
  window.__wbRrwebBuffer = [];
  try {
    rrwebRecord({
      emit: function(event){ window.__wbRrwebBuffer.push(event); },
      sampling: { scroll: 150, media: 800, input: 'last' },
      maskAllInputs: true
    });
  } catch (e) { /* rrweb unavailable on this page (e.g. chrome://) */ }
  var flush = function(){
    var buf = window.__wbRrwebBuffer;
    if (buf && buf.length && typeof window.__wbRrwebFlush === 'function') {
      window.__wbRrwebBuffer = [];
      try { window.__wbRrwebFlush(buf); } catch (e) {}
    }
  };
  setInterval(flush, 500);
  window.addEventListener('beforeunload', flush);
})();
`;
      await info.context.addInitScript({
        content: RECORDING.rrwebSource + "\n" + bootstrap,
      });
    } catch (e) {
      log(`[recording] rrweb setup failed: ${e.message}`);
      rec.kinds.rrweb = false;
    }
  }

  if (rec.kinds.video) {
    try {
      const outPath = path.join(
        os.tmpdir(),
        `wb-video-${sanitize(sessionName)}-${Date.now()}-${process.pid}.webm`,
      );
      rec.videoPath = outPath;
      const ff = spawn(
        "ffmpeg",
        [
          "-hide_banner",
          "-loglevel",
          "warning",
          "-y",
          "-f",
          "image2pipe",
          "-vcodec",
          "mjpeg",
          "-framerate",
          String(RECORDING.fps),
          "-i",
          "pipe:0",
          "-c:v",
          "libvpx-vp9",
          "-b:v",
          "1M",
          "-deadline",
          "realtime",
          "-pix_fmt",
          "yuv420p",
          outPath,
        ],
        { stdio: ["pipe", "ignore", "pipe"] },
      );
      ff.stderr.on("data", (d) => {
        const s = d.toString().trim();
        if (s) log(`[ffmpeg] ${s.slice(0, 240)}`);
      });
      // Broken pipe on shutdown is normal — swallow it so it doesn't crash the
      // node process via the default 'error' handler.
      ff.stdin.on("error", (e) => {
        if (e.code !== "EPIPE") log(`[ffmpeg stdin] ${e.message}`);
      });
      rec.ffmpeg = ff;
      rec.ffmpegDone = new Promise((resolve) => {
        ff.on("close", (code) => resolve(code));
      });

      const cdp = await info.context.newCDPSession(info.page);
      rec.cdp = cdp;
      cdp.on("Page.screencastFrame", async (frame) => {
        try {
          if (ff.stdin.writable && !ff.killed) {
            ff.stdin.write(Buffer.from(frame.data, "base64"));
          }
          // Must ack each frame or Chrome stops streaming.
          await cdp.send("Page.screencastFrameAck", {
            sessionId: frame.sessionId,
          });
        } catch {
          // Session tearing down — safe to ignore.
        }
      });
      await cdp.send("Page.startScreencast", {
        format: "jpeg",
        quality: RECORDING.quality,
        everyNthFrame: 1,
      });
    } catch (e) {
      log(`[recording] video setup failed: ${e.message}`);
      rec.kinds.video = false;
      if (rec.ffmpeg) {
        try {
          rec.ffmpeg.kill();
        } catch {}
      }
    }
  }

  const active = Object.entries(rec.kinds)
    .filter(([, v]) => v)
    .map(([k]) => k);
  if (active.length) {
    send({
      type: "slice.recording.started",
      session: sessionName,
      run_id: RECORDING.runId,
      kinds: active,
    });
  }
}

async function flushRecording(info, sessionName) {
  if (!info.recording) return;
  const rec = info.recording;

  let rrwebBody = null;
  if (rec.kinds.rrweb) {
    try {
      const tail = await info.page.evaluate(() => {
        if (!Array.isArray(window.__wbRrwebBuffer)) return [];
        const out = window.__wbRrwebBuffer;
        window.__wbRrwebBuffer = [];
        return out;
      });
      if (Array.isArray(tail)) {
        for (const e of tail) rec.rrwebEvents.push(e);
      }
    } catch (e) {
      log(`[recording] rrweb final drain failed: ${e.message}`);
    }
    if (rec.rrwebEvents.length > 0) {
      try {
        const json = JSON.stringify({
          run_id: RECORDING.runId,
          session: sessionName,
          event_count: rec.rrwebEvents.length,
          events: rec.rrwebEvents,
        });
        rrwebBody = await gzip(Buffer.from(json, "utf8"));
      } catch (e) {
        log(`[recording] rrweb gzip failed: ${e.message}`);
      }
    }
  }

  let videoBody = null;
  if (rec.kinds.video && rec.cdp && rec.ffmpeg) {
    try {
      await rec.cdp.send("Page.stopScreencast");
    } catch {
      // Browser may already be tearing down.
    }
    try {
      rec.ffmpeg.stdin.end();
      const settled = await Promise.race([
        rec.ffmpegDone,
        new Promise((r) => setTimeout(() => r("timeout"), 15_000)),
      ]);
      if (settled === "timeout") {
        log("[recording] ffmpeg did not exit within 15s; killing");
        try {
          rec.ffmpeg.kill("SIGKILL");
        } catch {}
      }
      if (rec.videoPath && existsSync(rec.videoPath)) {
        videoBody = await fsPromises.readFile(rec.videoPath);
        try {
          await fsPromises.unlink(rec.videoPath);
        } catch {}
      }
    } catch (e) {
      log(`[recording] video finalize failed: ${e.message}`);
    }
  }

  const uploads = [];
  if (rrwebBody) {
    uploads.push(
      uploadArtifact(
        "rrweb",
        rrwebBody,
        "application/json+gzip",
        sessionName,
        { event_count: rec.rrwebEvents.length },
      ),
    );
  }
  if (videoBody) {
    uploads.push(
      uploadArtifact("video", videoBody, "video/webm", sessionName, {
        fps: RECORDING.fps,
      }),
    );
  }
  await Promise.allSettled(uploads);
}

async function uploadArtifact(kind, body, contentType, sessionName, extra) {
  const url = RECORDING.uploadUrl
    .replace("{run_id}", encodeURIComponent(RECORDING.runId))
    .replace("{kind}", encodeURIComponent(kind));
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 30_000);
  try {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${RECORDING.secret}`,
        "Content-Type": contentType,
        "X-WB-Run-Id": RECORDING.runId,
        "X-WB-Recording-Kind": kind,
        "X-WB-Session": sessionName,
      },
      body,
      signal: controller.signal,
    });
    if (!res.ok) {
      send({
        type: "slice.recording.failed",
        session: sessionName,
        run_id: RECORDING.runId,
        kind,
        status: res.status,
        reason: (await safeText(res)) || res.statusText || "upload rejected",
      });
      return;
    }
    send({
      type: "slice.recording.uploaded",
      session: sessionName,
      run_id: RECORDING.runId,
      kind,
      bytes: body.length,
      ...(extra || {}),
    });
  } catch (e) {
    send({
      type: "slice.recording.failed",
      session: sessionName,
      run_id: RECORDING.runId,
      kind,
      reason: e.name === "AbortError" ? "timeout" : e.message,
    });
  } finally {
    clearTimeout(timer);
  }
}

function sanitize(s) {
  return String(s || "default").replace(/[^A-Za-z0-9_-]+/g, "_");
}

// --- {{ env.X }} / {{ artifacts.X }} substitution --------------------------

const ENV_RE = /\{\{\s*env\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;
const ARTIFACT_RE = /\{\{\s*artifacts\.([A-Za-z_][A-Za-z0-9_.-]*)\s*\}\}/g;

function readArtifact(name) {
  const dir = (process.env.WB_ARTIFACTS_DIR || "").trim();
  if (!dir) {
    log(`[warn] artifacts.${name} referenced but WB_ARTIFACTS_DIR is not set`);
    return "";
  }
  // Per-verb read (no cache) so a bash cell that writes the artifact between
  // slices is always picked up by the next browser verb.
  for (const p of [path.join(dir, `${name}.txt`), path.join(dir, name)]) {
    try {
      return readFileSync(p, "utf8").trimEnd();
    } catch {
      // try next candidate
    }
  }
  log(`[warn] artifact ${name} not found in ${dir}; leaving placeholder`);
  return "";
}

function expand(value) {
  if (typeof value === "string") {
    return value
      .replace(ENV_RE, (_, name) => {
        const v = process.env[name];
        if (v === undefined) {
          // Leave the placeholder visible so failures surface in stderr
          // summaries instead of silently turning into empty strings.
          log(`[warn] env var ${name} is not set; leaving placeholder`);
          return "";
        }
        return v;
      })
      .replace(ARTIFACT_RE, (_, name) => readArtifact(name));
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

async function runVerb(page, verb, index, ctx) {
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
      if (ctx) ctx.lastResult = items;
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
      if (ctx) ctx.lastResult = result;
      return `script ran`;
    }
    case "save": {
      // Persist a JSON artifact into $WB_ARTIFACTS_DIR so later cells can read
      // it and wb can upload it. Captures the previous verb's output unless
      // the author provides an explicit `value:`.
      const artifactsDir = (process.env.WB_ARTIFACTS_DIR || "").trim();
      if (!artifactsDir) {
        throw new Error(
          "save: $WB_ARTIFACTS_DIR is not set — run this workbook via `wb run` (wb exports the dir for you)",
        );
      }
      const explicitValue = a.value !== undefined;
      const payload = explicitValue ? a.value : ctx?.lastResult;
      if (payload === undefined) {
        throw new Error(
          "save: no value provided and no prior extract/eval result to capture",
        );
      }
      const name =
        typeof a.name === "string" && a.name.trim().length > 0
          ? sanitizeArtifactName(a.name)
          : autoArtifactName(ctx?.blockIndex ?? index);
      const filename = name.endsWith(".json") ? name : `${name}.json`;
      const full = path.join(artifactsDir, filename);
      await fsPromises.mkdir(artifactsDir, { recursive: true });
      await fsPromises.writeFile(
        full,
        JSON.stringify(payload, null, 2),
        "utf8",
      );
      send({
        type: "slice.artifact_saved",
        filename,
        path: full,
        bytes: Buffer.byteLength(JSON.stringify(payload)),
      });
      return `→ ${filename}`;
    }
    default:
      throw new Error(`unsupported verb: ${name}`);
  }
}

function sanitizeArtifactName(s) {
  // Keep author-chosen names readable but safe as filenames. Drop anything
  // that could escape the artifacts dir (slashes, NULs, etc.).
  return String(s).replace(/[^A-Za-z0-9_.-]+/g, "_").slice(0, 200);
}

function autoArtifactName(blockIndex) {
  const rand = randomUUID().replace(/-/g, "").slice(0, 8);
  const n = Number.isFinite(blockIndex) ? blockIndex : 0;
  return `cell-${n}-${rand}`;
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
    case "save":
      return "name";
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
  const blockIndex =
    typeof msg.block_index === "number" ? msg.block_index : null;

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

  // Per-slice scratch so `save:` can capture the prior verb's JSON output.
  const sliceCtx = { lastResult: undefined, blockIndex };

  for (let i = startAt; i < verbs.length; i++) {
    const v = verbs[i];
    const name = verbName(v);
    try {
      const summary = await runVerb(session.page, v, i, sliceCtx);
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
  // Recordings must flush BEFORE browser.close() — rrweb tail drain needs a
  // live page.evaluate() and CDP screencast needs a live CDPSession.
  for (const [name, info] of sessions) {
    try {
      await flushRecording(info, name);
    } catch (e) {
      log(`[shutdown] flush recording ${name}: ${e.message}`);
    }
  }
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

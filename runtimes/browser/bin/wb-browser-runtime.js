#!/usr/bin/env node
// wb-browser-runtime — CDP + Playwright sidecar for `wb`.
//
// Speaks wb's line-framed JSON protocol on stdio (see ../README.md). Each
// `browser` fenced block in a workbook arrives as one `slice` message; this
// sidecar dispatches its verbs against a Playwright `Page` connected to a
// vendor-provided CDP endpoint.
//
// The vendor (Browserbase, browser-use, ...) is selected by WB_BROWSER_VENDOR
// and lives behind a provider in ../lib/providers/. Verbs, recording, session
// cache, and substitutions are all vendor-agnostic — they run against a
// Playwright Page regardless of whose chromium is on the other end.
//
// Sessions are cached by `session:` name across slices for the lifetime of
// this process, so a runbook with multiple browser blocks against the same
// vendor reuses one session (and one logged-in browser context).
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
import { readFileSync } from "node:fs";
import { send, log } from "../lib/io.js";
import { resolveInside } from "../lib/util.js";
import { SessionManager } from "../lib/session-manager.js";
import {
  RecordingManager,
  loadRecordingConfig,
} from "../lib/recording-manager.js";
import { getProvider } from "../lib/providers/index.js";
import { SUPPORTS, runVerb, verbName } from "../verbs/index.js";

const VERSION = "0.7.0";
const provider = getProvider();
log(`[provider] ${provider.name}`);

// --- Recording --------------------------------------------------------------
//
// Feature is off unless WB_RECORDING_UPLOAD_URL is set. See
// runtimes/browser/lib/recording-manager.js for the full lifecycle.

const recording = new RecordingManager(loadRecordingConfig());
if (recording.enabled) {
  log(
    `[recording] enabled run_id=${recording.runId} kinds=${recording.activeKinds.join(",")} fps=${recording.fps} quality=${recording.quality}`,
  );
}

// --- Session cache ----------------------------------------------------------

const sessions = new SessionManager();

async function ensureSession(name, { profile } = {}) {
  return sessions.ensure(name, async () => {
    // Vendors charge for the session the moment allocate() returns; if
    // anything after this point throws (getLiveUrl, CDP connect, newContext,
    // recording setup) we must release it explicitly or quota leaks until
    // the vendor's idle timeout. SessionManager only caches a successful
    // return, so on throw there's no half-populated entry to clean up here.
    //
    // Lifecycle timings attached to `slice.session_started` tell operators
    // which step dominated when startup feels slow — usually connectOverCDP
    // against a cold vendor region, but the live-URL fetch and
    // newContext/newPage can each stall independently.
    const t0 = Date.now();
    const allocated = await provider.allocate({ profile, sessionName: name });
    const tAllocated = Date.now();
    let browser = null;
    try {
      const liveUrl = await provider.getLiveUrl(allocated);
      browser = await chromium.connectOverCDP(allocated.cdpUrl);
      const tConnected = Date.now();
      const context = browser.contexts()[0] ?? (await browser.newContext());
      const page = context.pages()[0] ?? (await context.newPage());
      const tPageReady = Date.now();

      const info = {
        sid: allocated.sid,
        browser,
        context,
        page,
        liveUrl,
        recording: null,
      };

      send({
        type: "slice.session_started",
        session: name,
        session_id: allocated.sid,
        live_url: liveUrl,
        vendor: provider.name,
        started_at: new Date().toISOString(),
        timings: {
          allocate_ms: tAllocated - t0,
          connect_ms: tConnected - tAllocated,
          page_ready_ms: tPageReady - tConnected,
          total_ms: tPageReady - t0,
        },
      });

      await recording.start(info, name);
      return info;
    } catch (e) {
      if (browser) {
        try {
          await browser.close();
        } catch {}
      }
      await provider.release(allocated.sid);
      throw e;
    }
  });
}
// --- {{ env.X }} / {{ artifacts.X }} substitution --------------------------

const ENV_RE = /\{\{\s*env\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;
// Artifact names are bare identifiers — no dots, no slashes. Anything more
// exotic would invite path traversal once composed with WB_ARTIFACTS_DIR.
const ARTIFACT_RE = /\{\{\s*artifacts\.([A-Za-z_][A-Za-z0-9_-]*)\s*\}\}/g;

// Resolved once at module load. `warn` matches historical behavior
// (log + empty string, runbook continues). `error` throws so a missing OTP
// or env var fails the slice instead of silently sending an empty value
// into a Playwright action. `empty` is the silent variant.
const ON_MISSING = (() => {
  const raw = (process.env.WB_SUBSTITUTION_ON_MISSING || "warn")
    .trim()
    .toLowerCase();
  if (raw === "error" || raw === "empty" || raw === "warn") return raw;
  log(
    `[warn] WB_SUBSTITUTION_ON_MISSING=${raw} is not valid (warn|error|empty); defaulting to warn`,
  );
  return "warn";
})();

function handleMissingSubstitution(kind, name) {
  const msg = `${kind}.${name} is not set`;
  if (ON_MISSING === "error") {
    throw new Error(`substitution: ${msg}`);
  }
  if (ON_MISSING === "warn") {
    log(`[warn] ${msg}; substituting empty string`);
  }
  return "";
}

function readArtifactRaw(name) {
  const dir = (process.env.WB_ARTIFACTS_DIR || "").trim();
  if (!dir) {
    log(`[warn] artifacts.${name} referenced but WB_ARTIFACTS_DIR is not set`);
    return null;
  }
  for (const candidate of [`${name}.txt`, name]) {
    const full = resolveInside(dir, candidate);
    if (!full) continue;
    try {
      return readFileSync(full, "utf8").trimEnd();
    } catch {
      // try next candidate
    }
  }
  return null;
}

function readArtifact(name, cache) {
  if (cache && cache.has(name)) {
    const hit = cache.get(name);
    if (hit === null) return handleMissingSubstitution("artifacts", name);
    return hit;
  }
  const v = readArtifactRaw(name);
  if (cache) cache.set(name, v);
  if (v === null) return handleMissingSubstitution("artifacts", name);
  return v;
}

function expand(value, collected, artifactCache) {
  if (typeof value === "string") {
    return value
      .replace(ENV_RE, (_, name) => {
        const v = process.env[name];
        if (v === undefined) return handleMissingSubstitution("env", name);
        if (collected && v.length >= 3) collected.add(v);
        return v;
      })
      .replace(ARTIFACT_RE, (_, name) => {
        const v = readArtifact(name, artifactCache);
        if (collected && v && v.length >= 3) collected.add(v);
        return v;
      });
  }
  if (Array.isArray(value))
    return value.map((v) => expand(v, collected, artifactCache));
  if (value && typeof value === "object") {
    const out = {};
    for (const [k, v] of Object.entries(value))
      out[k] = expand(v, collected, artifactCache);
    return out;
  }
  return value;
}

// Scrub any values that came from {{ env.X }} / {{ artifacts.X }} expansion
// out of error messages before they cross the stdio boundary — Playwright and
// fetch errors sometimes echo their inputs (URLs, script bodies, assertion
// text) and those inputs may contain credentials.
function scrubSecrets(msg, secrets) {
  let out = String(msg == null ? "" : msg);
  if (!secrets) return out;
  for (const s of secrets) {
    if (!s) continue;
    out = out.split(s).join("«***»");
  }
  return out;
}

// --- Slice handler ----------------------------------------------------------

async function handleSlice(msg) {
  // Declared outside the inner try so the outer catch can scrub error
  // messages using whatever secrets were collected before the throw.
  const sliceCtx = {
    lastResult: undefined,
    blockIndex:
      typeof msg?.block_index === "number" ? msg.block_index : null,
    secrets: new Set(),
    // Per-slice cache so `{{ artifacts.otp }}` referenced from 5 verbs
    // hits disk once instead of 5× and doesn't block the event loop
    // per-verb. Freshness across slices is preserved because the cache is
    // scoped to one slice — a bash cell that rewrites the file between
    // slices is seen on the next slice's first read.
    artifactCache: new Map(),
  };
  // Per-slice wall-clock cap. Rust's SLICE_EVENT_TIMEOUT is per-event (resets
  // on every verb.complete), so a chain of 25 × 15s wait_fors that each emit
  // a frame never trips it — the sidecar just runs for 375s while the Rust
  // parent assumes progress. Cap aggregate slice time so we terminate cleanly
  // instead. Default 120s; operators who legitimately need longer can bump
  // via WB_SLICE_DEADLINE_MS.
  const sliceDeadlineMs =
    Number.parseInt(process.env.WB_SLICE_DEADLINE_MS || "", 10) || 120_000;
  const sliceDeadline = Date.now() + sliceDeadlineMs;
  // Top-level guard: any unhandled error must emit slice.failed so the Rust
  // side sees a terminal frame instead of waiting out SLICE_EVENT_TIMEOUT.
  try {
    const verbs = Array.isArray(msg.verbs) ? msg.verbs : [];
    const sessionName = msg.session || "default";
    const restore = msg.restore || null;

    let session;
    try {
      session = await ensureSession(sessionName, { profile: msg.profile });
    } catch (e) {
      send({
        type: "slice.failed",
        error: `session start failed: ${scrubSecrets(e.message, sliceCtx.secrets)}`,
      });
      return;
    }

    // Restore-from-pause is not implemented yet (no pause verb wired here).
    // The sidecar protocol leaves room for it; when wait_for_mfa lands, this
    // is where we'd jump to verbs[restore.state.verb_index].
    const startAt = restore?.state?.verb_index ?? 0;

    for (let i = startAt; i < verbs.length; i++) {
      if (Date.now() >= sliceDeadline) {
        send({
          type: "slice.failed",
          error: `slice exceeded deadline (${sliceDeadlineMs}ms); aborted before verb index ${i} of ${verbs.length}`,
        });
        return;
      }
      const v = verbs[i];
      const name = verbName(v);
      const verbStart = Date.now();
      try {
        const summary = await runVerb(session.page, v, i, sliceCtx, expand);
        send({
          type: "verb.complete",
          verb: name,
          verb_index: i,
          summary,
          duration_ms: Date.now() - verbStart,
        });
      } catch (e) {
        const duration_ms = Date.now() - verbStart;
        const clean = scrubSecrets(e.message, sliceCtx.secrets);
        send({
          type: "verb.failed",
          verb: name,
          verb_index: i,
          error: clean,
          duration_ms,
        });
        send({
          type: "slice.failed",
          error: `verb ${name} (index ${i}): ${clean}`,
        });
        return;
      }
    }
    send({ type: "slice.complete" });
  } catch (e) {
    log(`[slice] unhandled: ${e.stack || e.message}`);
    try {
      send({
        type: "slice.failed",
        error: `sidecar error: ${scrubSecrets(e.message, sliceCtx.secrets)}`,
      });
    } catch {}
  }
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
      await recording.flush(info, name);
    } catch (e) {
      log(`[shutdown] flush recording ${name}: ${e.message}`);
      // Unhandled flush error → consumer would otherwise see neither an
      // uploaded nor a failed event and have to infer loss from absence.
      try {
        send({
          type: "slice.recording.failed",
          session: name,
          run_id: recording.runId,
          reason: `finalize_error: ${e.message}`,
        });
      } catch {}
    }
  }
  for (const [name, info] of sessions) {
    try {
      await info.browser.close();
    } catch (e) {
      log(`[shutdown] close ${name}: ${e.message}`);
    }
  }
  // Ask the vendor to release sessions explicitly so quota isn't held by
  // orphans waiting for their idle timeout.
  await Promise.all(
    Array.from(sessions.values()).map((s) => provider.release(s.sid)),
  );
  process.exit(0);
}

// --- Main loop --------------------------------------------------------------

const rl = readline.createInterface({ input: process.stdin, terminal: false });

// Per-session dispatch: slices against the same session name serialize
// (shared Playwright page), slices against different names run in parallel.
// SessionManager owns the chain map + the in-flight-create dedup that makes
// this safe — two concurrent slices for "vendor-a" share one provider.allocate
// instead of racing to create two vendor sessions.
function dispatchSlice(msg) {
  const sessionName = msg.session || "default";
  return sessions
    .enqueueOn(sessionName, () => handleSlice(msg))
    .catch((e) => {
      // handleSlice has its own top-level guard that emits slice.failed;
      // this is the last-resort net for a bug that throws past that guard,
      // so the Rust parent never strands waiting on SLICE_EVENT_TIMEOUT.
      log(`[loop] ${e.stack || e.message}`);
      try {
        send({ type: "slice.failed", error: `sidecar loop error: ${e.message}` });
      } catch {}
    });
}

// Shutdown drains all pending per-session work, then tears down. Guarded
// against repeat entries via `shuttingDown` inside shutdown() itself.
async function drainAndShutdown() {
  try {
    await sessions.drainAll();
  } catch (e) {
    log(`[shutdown] drain failed: ${e.message}`);
  }
  await shutdown();
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
      dispatchSlice(msg);
      break;
    case "shutdown":
      drainAndShutdown();
      break;
    default:
      log(`[warn] unknown message type: ${msg.type}`);
  }
});

rl.on("close", () => {
  // stdin closed — drain pending work then exit.
  drainAndShutdown();
});

// If the Rust parent SIGTERMs us (timeout, abort, crash), Node's default is
// to exit without running shutdown() — which leaves ffmpeg processes and
// Browserbase sessions orphaned. Route signals through the same drain path.
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) {
  process.on(sig, () => {
    log(`[shutdown] received ${sig}`);
    drainAndShutdown();
  });
}

// Log unhandled rejections so a dropped promise doesn't exit the process
// silently between slices. The top-level guards in handleSlice / enqueue
// cover the hot paths; this catches background work (recording uploads, etc).
process.on("unhandledRejection", (reason) => {
  log(`[unhandledRejection] ${reason?.stack || reason}`);
});

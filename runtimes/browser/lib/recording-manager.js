// Recording lifecycle — rrweb DOM capture + CDP screencast video, both
// uploaded to a consumer endpoint at session close. Feature is off unless
// `WB_RECORDING_UPLOAD_URL` is set (validated in `loadRecordingConfig`).
//
// The manager has two public methods:
//
//   start(info, sessionName)  — installs rrweb via context.addInitScript +
//                               exposeBinding, and spawns `ffmpeg` piped
//                               from a per-page CDP screencast. Mutates
//                               `info.recording` with the per-session state
//                               (events buffer, ffmpeg handle, video path).
//
//   flush(info, sessionName)  — final rrweb drain + gzip, stop screencast,
//                               wait for ffmpeg to exit, upload both
//                               artifacts, then clean up the .webm on disk.
//                               Safe to call regardless of whether start()
//                               succeeded — returns immediately if there's
//                               no `info.recording`.
//
// Per-session state lives on `info.recording` (opaque to the main file) so
// SessionManager's cache stays a plain name -> SessionInfo map. The config
// (runId, kinds, fps, etc.) is constructor-scoped and shared across
// sessions — that's intentional, since a single wb process = a single
// recording stream.

import { spawn, spawnSync } from "node:child_process";
import {
  createReadStream,
  existsSync,
  readFileSync,
  promises as fsPromises,
} from "node:fs";
import { randomUUID } from "node:crypto";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";
import { promisify } from "node:util";
import { send, log } from "./io.js";
import { retryableFetch, safeText } from "./http.js";

const gzip = promisify(zlib.gzip);

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

function sanitize(s) {
  return String(s || "default").replace(/[^A-Za-z0-9_-]+/g, "_");
}

// Resolve recording config from environment at boot. Returns either
// `{ enabled: false, reason }` or a fully populated enabled config. Split
// out of the class so tests can inspect config without constructing the
// manager, and so main.js can log the boot banner on enabled without
// poking the manager's internals.
export function loadRecordingConfig() {
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

  // Clamp to ranges ffmpeg/libvpx-vp9 actually handles. Requesting fps=120
  // silently blew up memory; quality=0 produced unwatchable garbage. Clamp
  // + log so operators see the effective value.
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const rawFps =
    Number.parseInt(process.env.WB_RECORDING_SCREENCAST_FPS || "", 10) || 5;
  const rawQuality =
    Number.parseInt(process.env.WB_RECORDING_SCREENCAST_QUALITY || "", 10) ||
    60;
  const fps = clamp(rawFps, 1, 30);
  const quality = clamp(rawQuality, 10, 95);
  if (fps !== rawFps) {
    log(`[recording] fps=${rawFps} clamped to ${fps} (valid range 1..30)`);
  }
  if (quality !== rawQuality) {
    log(
      `[recording] quality=${rawQuality} clamped to ${quality} (valid range 10..95)`,
    );
  }

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

  const rrwebMaxEvents =
    Number.parseInt(process.env.WB_RECORDING_RRWEB_MAX_EVENTS || "", 10) ||
    50_000;

  return {
    enabled: true,
    uploadUrl,
    secret,
    runId,
    fps,
    quality,
    kinds,
    rrwebSource,
    rrwebMaxEvents,
  };
}

export class RecordingManager {
  constructor(config) {
    this._config = config;
  }

  get enabled() {
    return !!this._config.enabled;
  }

  get runId() {
    return this._config.runId ?? null;
  }

  get activeKinds() {
    if (!this.enabled) return [];
    return Object.entries(this._config.kinds)
      .filter(([, v]) => v)
      .map(([k]) => k);
  }

  get fps() {
    return this._config.fps ?? null;
  }

  get quality() {
    return this._config.quality ?? null;
  }

  // Per-page setup: inject rrweb, spawn ffmpeg, wire the CDP screencast.
  // No-op when disabled.
  async start(info, sessionName) {
    if (!this.enabled) return;
    const cfg = this._config;
    info.recording = {
      kinds: { ...cfg.kinds },
      rrwebEvents: [],
      rrwebDropped: 0,
      rrwebOverflowLogged: false,
      cdp: null,
      ffmpeg: null,
      ffmpegDone: null,
      videoPath: null,
    };
    const rec = info.recording;

    // Drop oldest events once the buffer exceeds the cap — keeps the tail
    // of a long run (usually the interesting bit) rather than failing the
    // upload or OOMing the sidecar. One warning per session so ops can
    // spot it.
    const pushRrweb = (e) => {
      if (rec.rrwebEvents.length >= cfg.rrwebMaxEvents) {
        rec.rrwebEvents.shift();
        rec.rrwebDropped++;
        if (!rec.rrwebOverflowLogged) {
          rec.rrwebOverflowLogged = true;
          log(
            `[recording] rrweb buffer hit cap (${cfg.rrwebMaxEvents}); dropping oldest events`,
          );
        }
      }
      rec.rrwebEvents.push(e);
    };

    if (rec.kinds.rrweb) {
      try {
        await info.context.exposeBinding("__wbRrwebFlush", (_src, batch) => {
          if (Array.isArray(batch)) {
            for (const e of batch) pushRrweb(e);
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
          content: cfg.rrwebSource + "\n" + bootstrap,
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
            String(cfg.fps),
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
        // Broken pipe on shutdown is normal — swallow it so it doesn't
        // crash the node process via the default 'error' handler.
        ff.stdin.on("error", (e) => {
          if (e.code !== "EPIPE") log(`[ffmpeg stdin] ${e.message}`);
        });
        rec.ffmpeg = ff;
        rec.ffmpegDone = new Promise((resolve) => {
          ff.on("close", (code) => resolve(code));
        });

        const cdp = await info.context.newCDPSession(info.page);
        rec.cdp = cdp;
        // Dedup identical consecutive frames. CDP emits repeats when
        // nothing changed on screen; encoding them as distinct frames
        // bloats the WebM and mis-paces playback. Compare the base64
        // string directly — it's cheaper than hashing and equivalent for
        // exact equality.
        let lastFrameData = null;
        let dedupCount = 0;
        let dedupLogged = false;

        cdp.on("Page.screencastFrame", async (frame) => {
          try {
            if (ff.stdin.writable && !ff.killed) {
              if (frame.data === lastFrameData) {
                dedupCount++;
                if (!dedupLogged && dedupCount >= 100) {
                  dedupLogged = true;
                  log(
                    `[recording] dedup active (${dedupCount} duplicate frames skipped so far)`,
                  );
                }
                // Still ack — Chrome needs it to keep streaming.
                await cdp.send("Page.screencastFrameAck", {
                  sessionId: frame.sessionId,
                });
                return;
              }
              lastFrameData = frame.data;
              const buf = Buffer.from(frame.data, "base64");
              const ok = ff.stdin.write(buf);
              // Backpressure: if ffmpeg's stdin buffer is full, wait for
              // drain before acking so Chrome slows frame production
              // instead of piling JPEG frames in Node heap. 5s fail-open
              // so a wedged ffmpeg can't stall the protocol indefinitely.
              if (!ok) {
                await new Promise((resolve) => {
                  let fired = false;
                  const done = () => {
                    if (fired) return;
                    fired = true;
                    ff.stdin.off("drain", done);
                    ff.stdin.off("close", done);
                    ff.stdin.off("error", done);
                    clearTimeout(timer);
                    resolve();
                  };
                  const timer = setTimeout(done, 5000);
                  ff.stdin.once("drain", done);
                  ff.stdin.once("close", done);
                  ff.stdin.once("error", done);
                });
              }
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
          quality: cfg.quality,
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
        run_id: cfg.runId,
        kinds: active,
      });
    }
  }

  async flush(info, sessionName) {
    if (!info.recording) return;
    const cfg = this._config;
    const rec = info.recording;

    let rrwebBody = null;
    let rrwebFailure = null;
    if (rec.kinds.rrweb) {
      try {
        const tail = await info.page.evaluate(() => {
          if (!Array.isArray(window.__wbRrwebBuffer)) return [];
          const out = window.__wbRrwebBuffer;
          window.__wbRrwebBuffer = [];
          return out;
        });
        if (Array.isArray(tail)) {
          for (const e of tail) {
            if (rec.rrwebEvents.length >= cfg.rrwebMaxEvents) {
              rec.rrwebEvents.shift();
              rec.rrwebDropped++;
            }
            rec.rrwebEvents.push(e);
          }
        }
      } catch (e) {
        log(`[recording] rrweb final drain failed: ${e.message}`);
        rrwebFailure = `final_drain_error: ${e.message}`;
      }
      if (rec.rrwebEvents.length > 0) {
        try {
          const json = JSON.stringify({
            run_id: cfg.runId,
            session: sessionName,
            event_count: rec.rrwebEvents.length,
            dropped: rec.rrwebDropped,
            events: rec.rrwebEvents,
          });
          rrwebBody = await gzip(Buffer.from(json, "utf8"));
        } catch (e) {
          log(`[recording] rrweb gzip failed: ${e.message}`);
          rrwebFailure = `gzip_error: ${e.message}`;
        }
      }
    }

    let videoBody = null;
    let videoFailure = null;
    if (rec.kinds.video && rec.cdp && rec.ffmpeg) {
      try {
        await rec.cdp.send("Page.stopScreencast");
      } catch {
        // Browser may already be tearing down.
      }
      const timeoutMs =
        Number.parseInt(
          process.env.WB_RECORDING_FFMPEG_TIMEOUT_MS || "",
          10,
        ) || 30_000;
      try {
        rec.ffmpeg.stdin.end();
        const settled = await Promise.race([
          rec.ffmpegDone,
          new Promise((r) =>
            setTimeout(() => r({ __timeout: true }), timeoutMs),
          ),
        ]);
        if (settled && typeof settled === "object" && settled.__timeout) {
          log(
            `[recording] ffmpeg did not exit within ${timeoutMs}ms; killing`,
          );
          try {
            rec.ffmpeg.kill("SIGKILL");
          } catch {}
          videoFailure = `ffmpeg_timeout_${timeoutMs}ms`;
        } else if (typeof settled === "number" && settled !== 0) {
          // ff.on('close') resolves with the exit code — non-zero means
          // ffmpeg produced a corrupt/partial webm that we should not
          // upload.
          videoFailure = `ffmpeg_exit_code_${settled}`;
          log(`[recording] ffmpeg exited with code ${settled}`);
        }
        if (!videoFailure && rec.videoPath && existsSync(rec.videoPath)) {
          // Stream the WebM off disk instead of buffering — a long slice
          // can produce hundreds of MB and slurping it into RAM just to
          // fetch() is the largest memory hit in this process. The
          // factory produces a fresh ReadStream per retry attempt.
          // cleanup() is deferred to _uploadArtifact's finally so the file
          // survives until upload settles.
          const stat = await fsPromises.stat(rec.videoPath);
          const videoPath = rec.videoPath;
          videoBody = {
            factory: () => createReadStream(videoPath),
            bytes: stat.size,
            cleanup: () => fsPromises.unlink(videoPath).catch(() => {}),
          };
        } else if (rec.videoPath && existsSync(rec.videoPath)) {
          // No upload path (failure or skip) — clean up the file now so
          // we don't leak disk. Upload path cleans up in _uploadArtifact's
          // finally.
          try {
            await fsPromises.unlink(rec.videoPath);
          } catch {}
        }
      } catch (e) {
        videoFailure = `finalize_error: ${e.message}`;
        log(`[recording] video finalize failed: ${e.message}`);
      }
    }

    const uploads = [];
    if (rrwebBody) {
      uploads.push(
        this._uploadArtifact(
          "rrweb",
          rrwebBody,
          "application/json+gzip",
          sessionName,
          { event_count: rec.rrwebEvents.length },
        ),
      );
    } else if (rrwebFailure) {
      // Surface pre-upload failures so consumers can distinguish "rrweb
      // recorded nothing" from "rrweb recorded and we lost it".
      send({
        type: "slice.recording.failed",
        session: sessionName,
        run_id: cfg.runId,
        kind: "rrweb",
        reason: rrwebFailure,
      });
    }
    if (videoBody) {
      uploads.push(
        this._uploadArtifact("video", videoBody, "video/webm", sessionName, {
          fps: cfg.fps,
        }),
      );
    } else if (videoFailure) {
      // Surface a terminal recording failure to the callback stream so
      // the consumer knows the video was lost rather than silently
      // missing.
      send({
        type: "slice.recording.failed",
        session: sessionName,
        run_id: cfg.runId,
        kind: "video",
        reason: videoFailure,
      });
    }
    await Promise.allSettled(uploads);
  }

  // `body` is either:
  //   Buffer                                 — legacy; reused across retries
  //   { factory, bytes, cleanup? }           — streaming; factory() returns a
  //                                            fresh Readable per attempt so
  //                                            retries get a new stream instead
  //                                            of a drained one. cleanup() (if
  //                                            provided) runs after the upload
  //                                            settles, success or failure.
  async _uploadArtifact(kind, body, contentType, sessionName, extra) {
    const cfg = this._config;
    const isStream =
      !Buffer.isBuffer(body) && typeof body?.factory === "function";
    const bytes = isStream ? body.bytes : body.length;
    const cleanup = isStream ? body.cleanup : null;
    const url = cfg.uploadUrl
      .replace("{run_id}", encodeURIComponent(cfg.runId))
      .replace("{kind}", encodeURIComponent(kind));
    try {
      const res = await retryableFetch(
        url,
        {
          method: "POST",
          headers: {
            Authorization: `Bearer ${cfg.secret}`,
            "Content-Type": contentType,
            "X-WB-Run-Id": cfg.runId,
            "X-WB-Recording-Kind": kind,
            "X-WB-Session": sessionName,
            ...(isStream ? { "Content-Length": String(bytes) } : {}),
          },
          body: isStream ? undefined : body,
        },
        `upload.${kind}`,
        {
          timeoutMs: 30_000,
          bodyFactory: isStream ? body.factory : null,
        },
      );
      if (!res.ok) {
        send({
          type: "slice.recording.failed",
          session: sessionName,
          run_id: cfg.runId,
          kind,
          status: res.status,
          reason: (await safeText(res)) || res.statusText || "upload rejected",
        });
        return;
      }
      send({
        type: "slice.recording.uploaded",
        session: sessionName,
        run_id: cfg.runId,
        kind,
        bytes,
        ...(extra || {}),
      });
    } catch (e) {
      send({
        type: "slice.recording.failed",
        session: sessionName,
        run_id: cfg.runId,
        kind,
        reason: e.name === "AbortError" ? "timeout" : e.message,
      });
    } finally {
      if (cleanup) {
        try {
          await cleanup();
        } catch {}
      }
    }
  }
}

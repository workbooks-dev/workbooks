// wait_for_drop — wait for files to land in a Google Drive folder.
//
// Unlike `pause_for_human`, which hands control back to the Rust side via the
// `__pause` sentinel + `slice.paused` + exit 42, `wait_for_drop` is *in-slice*:
// it keeps the sidecar alive through a poll loop, never triggering the
// pause-and-resume lifecycle. The verb returns a summary when the folder's
// contents satisfy the predicate, throws on timeout.
//
// Trade-off: the run's `wb` process must stay up for the whole wait. No
// resume-after-crash, no operator-click resume (a follow-up feature). What we
// get in exchange: simple UX for the runbook author, no out-of-band plumbing.
//
// The operator sees progress because we emit a `slice.drop_poll` event on
// each poll cycle — wb forwards these as `step.drop_poll` callbacks, and the
// per-event watchdog (SLICE_EVENT_TIMEOUT, resets on every emitted frame)
// stays alive through the wait.
//
// The Paracord relay's Google Drive connector handles auth + transport. We
// never touch Google's APIs directly — the bearer token is for the relay,
// not Google. When the relay is down we get a 502 `provider_not_connected`
// response, which we surface verbatim so the operator sees the actual
// failure mode.

import { send } from "../lib/io.js";
import { writeFileSync, mkdirSync } from "node:fs";
import { resolveInside } from "../lib/util.js";

// Poll-budget guardrails. The spec worst case (10s poll × 30min timeout =
// 180 relay calls per run) is inside typical per-key quotas; bumping timeout
// past an hour without extending poll_every would start to matter. These are
// hard caps: runbooks that exceed them should re-think the approach.
const MAX_TIMEOUT_MS = 4 * 60 * 60 * 1000; // 4h
const MIN_POLL_MS = 2_000; // 2s — relay rate-limit breathing room

const FOLDER_URL_RE =
  /https:\/\/drive\.google\.com\/drive\/(?:u\/\d+\/)?folders\/([A-Za-z0-9_-]+)/;

function extractFolderId(folderUrl) {
  if (!folderUrl || typeof folderUrl !== "string") {
    throw new Error("wait_for_drop: folder_url is required");
  }
  const m = folderUrl.match(FOLDER_URL_RE);
  if (!m) {
    throw new Error(
      `wait_for_drop: folder_url does not look like a Drive folder URL: ${folderUrl}`,
    );
  }
  return m[1];
}

// "30s" | "5m" | "1h" | bare integer (seconds). Mirrors wb's Rust-side parser
// closely enough that authors don't need to context-switch between the two.
function parseDurationMs(s, fallbackMs) {
  if (s == null) return fallbackMs;
  if (typeof s === "number") return s * 1000;
  const m = String(s).trim().match(/^(\d+)\s*([smh]?)$/i);
  if (!m) {
    throw new Error(`wait_for_drop: invalid duration "${s}" (use 30s, 5m, 1h)`);
  }
  const n = Number.parseInt(m[1], 10);
  const unit = (m[2] || "s").toLowerCase();
  const mult = unit === "h" ? 3600 : unit === "m" ? 60 : 1;
  return n * mult * 1000;
}

// Shell-style glob → RegExp. Only `*` + `?` are supported; anything fancier
// is a smell in a filename pattern.
function globToRegex(pattern) {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  const re = escaped.replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp(`^${re}$`);
}

function predicateMatches(files, expect, pattern) {
  if (expect === "at_least_one_file") {
    return files.length > 0;
  }
  if (expect === "filename_matches") {
    if (!pattern) {
      throw new Error(
        "wait_for_drop: filename_matches requires filename_pattern",
      );
    }
    const re = globToRegex(pattern);
    return files.some((f) => re.test(f.name || ""));
  }
  throw new Error(
    `wait_for_drop: expect must be "at_least_one_file" or "filename_matches", got "${expect}"`,
  );
}

async function pollFolder(relayBase, apiKey, folderId) {
  const q = encodeURIComponent(`'${folderId}' in parents and trashed = false`);
  const url = `${relayBase.replace(/\/$/, "")}/google_drive/drive/v3/files?q=${q}&fields=files(id,name,mimeType,modifiedTime,size)`;
  const resp = await fetch(url, {
    headers: { Authorization: `Bearer ${apiKey}`, Accept: "application/json" },
  });
  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(
      `wait_for_drop: relay returned ${resp.status} — ${body.slice(0, 200)}`,
    );
  }
  const data = await resp.json().catch(() => ({}));
  return Array.isArray(data.files) ? data.files : [];
}

function writeBindArtifact(name, files) {
  const dir = (process.env.WB_ARTIFACTS_DIR || "").trim();
  if (!dir) {
    // Non-fatal; downstream cells might not need the list. Matches the
    // warn-and-continue posture of other artifact-writing verbs.
    console.log(
      `[wait_for_drop] WB_ARTIFACTS_DIR not set; skipping bind_artifact write`,
    );
    return null;
  }
  mkdirSync(dir, { recursive: true });
  const target = resolveInside(dir, `${name}.json`);
  if (!target) {
    throw new Error(
      `wait_for_drop: invalid bind_artifact name "${name}" (resolves outside artifacts dir)`,
    );
  }
  writeFileSync(target, JSON.stringify({ files }, null, 2), "utf8");
  return target;
}

export default {
  name: "wait_for_drop",
  primaryKey: "folder_url",
  async execute(_page, args, ctx) {
    const apiKey = (process.env.PARACORD_RELAY_API_KEY || "").trim();
    if (!apiKey) {
      throw new Error(
        "wait_for_drop: PARACORD_RELAY_API_KEY is required (Google Drive connector access)",
      );
    }
    const relayBase = (process.env.PARACORD_RELAY_URL || "").trim();
    if (!relayBase) {
      throw new Error(
        "wait_for_drop: PARACORD_RELAY_URL is required (base URL of the Paracord relay)",
      );
    }

    const folderId = extractFolderId(args.folder_url);
    const expect = args.expect || "at_least_one_file";
    const pattern = args.filename_pattern || null;
    if (expect !== "at_least_one_file" && expect !== "filename_matches") {
      throw new Error(
        `wait_for_drop: expect must be "at_least_one_file" or "filename_matches", got "${expect}"`,
      );
    }
    if (expect === "filename_matches" && !pattern) {
      throw new Error(
        "wait_for_drop: filename_matches requires filename_pattern (e.g. '*.pdf')",
      );
    }
    const pollMs = Math.max(
      MIN_POLL_MS,
      parseDurationMs(args.poll_every, 10_000),
    );
    const timeoutMs = Math.min(
      MAX_TIMEOUT_MS,
      parseDurationMs(args.timeout, 30 * 60 * 1000),
    );
    const bindName = args.bind_artifact || "dropped_files";
    const message =
      args.message || "Waiting for files to land in the Drive folder";

    const startedAt = Date.now();
    const deadline = startedAt + timeoutMs;

    // Announce the wait — gives the run page a frame to pin the widget on,
    // distinct from the per-poll heartbeats below.
    send({
      type: "slice.drop_waiting",
      verb: "wait_for_drop",
      verb_index: ctx.index,
      folder_url: args.folder_url,
      message,
      expect,
      filename_pattern: pattern,
      poll_every_ms: pollMs,
      timeout_ms: timeoutMs,
    });

    let pollCount = 0;
    while (Date.now() < deadline) {
      pollCount++;
      let files;
      try {
        files = await pollFolder(relayBase, apiKey, folderId);
      } catch (e) {
        // Surface poll errors as a heartbeat with `error:` set, then keep
        // polling. A transient relay blip shouldn't abort a 30-min wait,
        // but the operator should see the dashboard flag the degradation.
        send({
          type: "slice.drop_poll",
          verb_index: ctx.index,
          poll: pollCount,
          error: String(e.message || e),
        });
        await sleep(Math.min(pollMs, Math.max(0, deadline - Date.now())));
        continue;
      }

      const matched = predicateMatches(files, expect, pattern);
      send({
        type: "slice.drop_poll",
        verb_index: ctx.index,
        poll: pollCount,
        file_count: files.length,
        matched,
      });

      if (matched) {
        const written = writeBindArtifact(bindName, files);
        return `wait_for_drop: matched after ${pollCount} poll(s) (${files.length} file${files.length === 1 ? "" : "s"})${written ? ` → ${bindName}.json` : ""}`;
      }

      const remaining = deadline - Date.now();
      if (remaining <= 0) break;
      await sleep(Math.min(pollMs, remaining));
    }

    throw new Error(
      `wait_for_drop: timed out after ${Math.round(timeoutMs / 1000)}s and ${pollCount} poll(s) — no files matching ${expect}${pattern ? ` (${pattern})` : ""} appeared`,
    );
  },
};

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

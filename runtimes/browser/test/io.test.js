// io.js log-level filtering. The level is resolved once at module load
// from WB_LOG_LEVEL, so we run each level scenario in a child process to
// get a clean module state — swapping env mid-process wouldn't retrigger
// the resolve.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const IO_PATH = path.resolve(__dirname, "../lib/io.js");

// Run a tiny inline script that imports io.js and invokes each log level.
// stderr contains whatever survived the WB_LOG_LEVEL filter.
function runAtLevel(level) {
  const script = `
import { logTrace, logDebug, log, logWarn, logError } from ${JSON.stringify(IO_PATH)};
logTrace("TRACE");
logDebug("DEBUG");
log("INFO");
logWarn("WARN");
logError("ERROR");
`;
  const res = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
    env: { ...process.env, WB_LOG_LEVEL: level ?? "" },
    encoding: "utf8",
  });
  return res.stderr;
}

test("default level (info) shows info/warn/error, hides trace/debug", () => {
  const err = runAtLevel(undefined);
  assert.match(err, /INFO/);
  assert.match(err, /WARN/);
  assert.match(err, /ERROR/);
  assert.ok(!err.includes("TRACE"));
  assert.ok(!err.includes("DEBUG"));
});

test("debug level also shows debug but not trace", () => {
  const err = runAtLevel("debug");
  assert.match(err, /DEBUG/);
  assert.match(err, /INFO/);
  assert.match(err, /WARN/);
  assert.match(err, /ERROR/);
  assert.ok(!err.includes("TRACE"));
});

test("trace level shows everything", () => {
  const err = runAtLevel("trace");
  for (const line of ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]) {
    assert.match(err, new RegExp(line));
  }
});

test("warn level suppresses info/debug/trace", () => {
  const err = runAtLevel("warn");
  assert.match(err, /WARN/);
  assert.match(err, /ERROR/);
  assert.ok(!err.includes("INFO"));
  assert.ok(!err.includes("DEBUG"));
  assert.ok(!err.includes("TRACE"));
});

test("error level shows only error", () => {
  const err = runAtLevel("error");
  assert.match(err, /ERROR/);
  assert.ok(!err.includes("WARN"));
  assert.ok(!err.includes("INFO"));
});

test("invalid level falls back to info with a one-shot warning", () => {
  const err = runAtLevel("bogus");
  assert.match(err, /WB_LOG_LEVEL=bogus is not valid/);
  // Still behaves as info-level.
  assert.match(err, /INFO/);
  assert.ok(!err.includes("DEBUG"));
});

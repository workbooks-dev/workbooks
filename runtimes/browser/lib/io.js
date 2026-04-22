// Line-framed JSON protocol I/O. `send` writes a single frame to stdout;
// `log*` helpers write diagnostic output to stderr, filtered by
// `WB_LOG_LEVEL` (trace|debug|info|warn|error, default info).
//
// `log()` (unqualified) is info-level for back-compat — existing call
// sites don't need to be reclassified. New verbose output should use
// `logDebug` / `logTrace` so it can be silenced by default. Warn/error
// helpers exist so a single grep finds all the paths that will always
// surface.

const LOG_LEVELS = { trace: 0, debug: 1, info: 2, warn: 3, error: 4 };

function resolveLevel() {
  const raw = (process.env.WB_LOG_LEVEL || "info").trim().toLowerCase();
  const level = LOG_LEVELS[raw];
  if (level === undefined) {
    process.stderr.write(
      `[warn] WB_LOG_LEVEL=${raw} is not valid (trace|debug|info|warn|error); defaulting to info\n`,
    );
    return LOG_LEVELS.info;
  }
  return level;
}

// Resolved once at module load — sidecar boots, runs, exits. If we ever
// need live reconfiguration, swap this for a getter that re-reads env.
const CURRENT_LEVEL = resolveLevel();

export function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function emit(level, args) {
  if (LOG_LEVELS[level] < CURRENT_LEVEL) return;
  process.stderr.write(args.join(" ") + "\n");
}

export function logTrace(...args) {
  emit("trace", args);
}

export function logDebug(...args) {
  emit("debug", args);
}

export function log(...args) {
  emit("info", args);
}

export function logWarn(...args) {
  emit("warn", args);
}

export function logError(...args) {
  emit("error", args);
}

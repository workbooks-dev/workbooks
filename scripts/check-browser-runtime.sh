#!/usr/bin/env bash
# Verify wb-browser-runtime is installed and answers the wb-sidecar/1
# handshake correctly. Used as a post-install smoke test from install.sh
# and as a standalone diagnostic when browser workbooks misbehave.
#
# Usage:
#   scripts/check-browser-runtime.sh            # uses wb-browser-runtime on $PATH
#   WB_BROWSER_RUNTIME=/custom/path scripts/check-browser-runtime.sh
#
# Exit codes:
#   0  runtime installed, correct protocol, ready frame OK
#   1  runtime binary not found on $PATH and $WB_BROWSER_RUNTIME unset/missing
#   2  runtime spawned but emitted no ready frame within the probe window
#   3  runtime emitted a frame of the wrong type
#   4  runtime speaks a non-wb-sidecar/1 protocol
#   5  validator (node) not available to parse the ready frame

set -u

fail() {
  local code="$1"; shift
  printf "error: %s\n" "$*" >&2
  exit "$code"
}

# Resolve the runtime binary. Matches the Rust sidecar lookup order in
# src/sidecar.rs: WB_BROWSER_RUNTIME env var first, then $PATH.
runtime_bin=""
if [ -n "${WB_BROWSER_RUNTIME:-}" ]; then
  if [ -x "$WB_BROWSER_RUNTIME" ] || command -v "$WB_BROWSER_RUNTIME" >/dev/null 2>&1; then
    runtime_bin="$WB_BROWSER_RUNTIME"
  else
    fail 1 "WB_BROWSER_RUNTIME points at '$WB_BROWSER_RUNTIME' but it's not executable or on \$PATH"
  fi
elif command -v wb-browser-runtime >/dev/null 2>&1; then
  runtime_bin="wb-browser-runtime"
else
  {
    echo "wb-browser-runtime not found on \$PATH"
    echo "  install: npm i -g wb-browser-runtime"
    if command -v npm >/dev/null 2>&1; then
      npm_bin=$(npm bin -g 2>/dev/null || npm prefix -g 2>/dev/null)
      [ -n "$npm_bin" ] && echo "  npm global bin: $npm_bin (ensure it's on \$PATH)"
    else
      echo "  (npm not found — install Node.js >=24 first: https://nodejs.org)"
    fi
  } >&2
  exit 1
fi

# Send hello + shutdown over stdin, capture only the first stdout line
# (the ready frame). The sleep gives the runtime a beat to answer before
# we close stdin. Any slower and there's something genuinely broken.
ready=$(
  {
    printf '{"type":"hello"}\n'
    sleep 1
    printf '{"type":"shutdown"}\n'
  } | "$runtime_bin" 2>/dev/null | head -n 1
) || true

[ -n "$ready" ] || fail 2 "no ready frame from '$runtime_bin' (check that Node >=24 is installed: node --version)"

# Parse + validate via node (the runtime requires Node, so if the binary
# ran at all, node is available). Using node avoids adding jq as a hard
# dependency of this script.
command -v node >/dev/null 2>&1 || fail 5 "node not found — required to parse the ready frame"

summary=$(
  printf '%s' "$ready" | node -e '
    let buf = "";
    process.stdin.on("data", d => buf += d);
    process.stdin.on("end", () => {
      let f;
      try { f = JSON.parse(buf); }
      catch (e) { console.error("parse error:", e.message); process.exit(3); }
      if (f.type !== "ready") { console.error("unexpected frame type:", f.type); process.exit(3); }
      if (f.protocol !== "wb-sidecar/1") { console.error("protocol:", f.protocol); process.exit(4); }
      const n = Array.isArray(f.supports) ? f.supports.length : 0;
      console.log(`ok: ${f.runtime} ${f.version} (${f.protocol}) — ${n} verbs`);
    });
  '
) || {
  rc=$?
  case "$rc" in
    3) fail 3 "runtime emitted an unexpected first frame: $ready" ;;
    4) fail 4 "runtime speaks a different protocol than wb-sidecar/1: $ready" ;;
    *) fail "$rc" "validation failed (exit $rc): $ready" ;;
  esac
}

echo "$summary"

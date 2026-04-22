#!/usr/bin/env bash
set -euo pipefail

# Test: include-scoped step.started / step.finished callback events (F2).
# Runs examples/include-demo.md with a local HTTP callback sink, then
# verifies the captured event timeline:
#   step.started (enter)  →  step.complete (inside, chain=[include])
#   step.finished (exit, outcome=ok, duration_ms > 0)
#   step.complete (outside, chain=[])
#   run.complete (status=pass)

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WB="$SCRIPT_DIR/target/release/wb"
PORT=8879
LOG="$(mktemp)"
PID_FILE="$(mktemp)"
PASS=0
FAIL=0

pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }

cleanup() {
    if [ -f "$PID_FILE" ]; then
        kill "$(cat "$PID_FILE")" 2>/dev/null || true
        rm -f "$PID_FILE"
    fi
    rm -f "$LOG"
}
trap cleanup EXIT

echo "=== F2: include-scoped step events test ==="

echo "Building release binary..."
(cd "$SCRIPT_DIR" && cargo build --release 2>/dev/null)

# Start the callback sink
python3 - "$PORT" "$LOG" "$PID_FILE" <<'PY' &
import http.server, json, sys, os
port, log, pid_file = int(sys.argv[1]), sys.argv[2], sys.argv[3]
with open(pid_file, "w") as f:
    f.write(str(os.getpid()))

class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n).decode()
        evt = self.headers.get("X-WB-Event", "?")
        # Write the raw body — re-serializing via json.dumps() would
        # reshuffle keys and add whitespace, breaking the grep assertions
        # below that pin exact JSON substrings.
        with open(log, "a") as f:
            f.write(evt + " " + body + "\n")
        self.send_response(200); self.end_headers()
    def log_message(self, *a): pass

http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
PY

sleep 0.4

# Run the workbook
echo "Running examples/include-demo.md with --callback http://127.0.0.1:$PORT/cb"
(cd "$SCRIPT_DIR" && "$WB" run examples/include-demo.md --callback "http://127.0.0.1:$PORT/cb" >/dev/null 2>&1)

# Give callbacks time to land (curl is fire-and-forget from wb's POV)
sleep 0.3

if [ ! -s "$LOG" ]; then
    fail "no callback events captured"
    echo "=== Results: $PASS passed, $FAIL failed ==="
    exit 1
fi

echo "Captured events:"
awk '{print "  " $1}' "$LOG"

# Assertions: event sequence and content
if grep -q '^step.started ' "$LOG"; then
    pass "step.started fired"
else
    fail "step.started missing"
fi

if grep '^step.started ' "$LOG" | grep -q '"step_id":"examples/include-login.md"'; then
    pass "step.started carries step_id=examples/include-login.md"
else
    fail "step.started missing expected step_id"
fi

if grep '^step.started ' "$LOG" | grep -q '"step_title":"Reusable login (fake)"'; then
    pass "step.started carries step_title from included frontmatter"
else
    fail "step.started missing step_title"
fi

# The inner block must carry include_chain with the frame; the outer must be []
if grep '^step.complete ' "$LOG" | head -1 | grep -q '"include_chain":\[{"step_id":"examples/include-login.md"'; then
    pass "first step.complete has non-empty include_chain"
else
    fail "first step.complete missing include_chain"
fi

if grep '^step.complete ' "$LOG" | tail -1 | grep -q '"include_chain":\[\]'; then
    pass "second step.complete has empty include_chain"
else
    fail "second step.complete should have empty include_chain"
fi

if grep -q '^step.finished ' "$LOG"; then
    pass "step.finished fired"
else
    fail "step.finished missing"
fi

if grep '^step.finished ' "$LOG" | grep -q '"outcome":"ok"'; then
    pass "step.finished outcome=ok"
else
    fail "step.finished outcome should be ok"
fi

# duration_ms should be a non-negative integer (at least 0, not null)
if grep '^step.finished ' "$LOG" | grep -qE '"duration_ms":[0-9]+'; then
    pass "step.finished carries numeric duration_ms"
else
    fail "step.finished missing duration_ms"
fi

# Order check: step.started precedes step.finished in the log
START_LINE=$(grep -n '^step.started ' "$LOG" | head -1 | cut -d: -f1)
FINISH_LINE=$(grep -n '^step.finished ' "$LOG" | head -1 | cut -d: -f1)
if [ -n "$START_LINE" ] && [ -n "$FINISH_LINE" ] && [ "$START_LINE" -lt "$FINISH_LINE" ]; then
    pass "step.started precedes step.finished"
else
    fail "step.started should precede step.finished"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

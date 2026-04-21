#!/usr/bin/env bash
set -euo pipefail

# Test: wait-demo.md end-to-end
# Verifies the full pause/resume cycle using the wait primitive.

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WB="$SCRIPT_DIR/target/release/wb"
CHECKPOINT_ID="test-wait-demo-e2e"
CHECKPOINT_DIR="$HOME/.wb/checkpoints"
PASS=0
FAIL=0

cleanup() {
    rm -f "$CHECKPOINT_DIR/${CHECKPOINT_ID}.json"
    rm -f "$CHECKPOINT_DIR/${CHECKPOINT_ID}.pending.json"
}

pass() {
    echo "  PASS: $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "  FAIL: $1"
    FAIL=$((FAIL + 1))
}

# Always clean up on exit
trap cleanup EXIT

echo "=== wait-demo.md end-to-end test ==="
echo ""

# 0. Build
echo "Building release binary..."
(cd "$SCRIPT_DIR" && cargo build --release 2>/dev/null)
echo ""

# Clean up any leftover state from previous runs
cleanup

# --- Step 1: Run the workbook, expect exit 42 (paused) ---
echo "Step 1: Run wait-demo.md (expect pause at wait block)"
set +e
(cd "$SCRIPT_DIR" && "$WB" run examples/wait-demo.md --checkpoint "$CHECKPOINT_ID" 2>/dev/null)
EXIT_CODE=$?
set -e

if [ "$EXIT_CODE" -eq 42 ]; then
    pass "exit code is 42 (paused)"
else
    fail "expected exit code 42, got $EXIT_CODE"
fi

# --- Step 2: Check pending descriptor exists ---
echo "Step 2: Check pending descriptor file exists"
if [ -f "$CHECKPOINT_DIR/${CHECKPOINT_ID}.pending.json" ]; then
    pass "pending descriptor exists at $CHECKPOINT_DIR/${CHECKPOINT_ID}.pending.json"
else
    fail "pending descriptor not found"
fi

# --- Step 3: Check checkpoint file exists ---
echo "Step 3: Check checkpoint file exists"
if [ -f "$CHECKPOINT_DIR/${CHECKPOINT_ID}.json" ]; then
    pass "checkpoint file exists"
else
    fail "checkpoint file not found"
fi

# --- Step 4: wb pending lists the paused workbook ---
echo "Step 4: Run 'wb pending' and check listing"
set +e
PENDING_OUTPUT=$("$WB" pending 2>/dev/null)
PENDING_EXIT=$?
set -e

if echo "$PENDING_OUTPUT" | grep -q "$CHECKPOINT_ID"; then
    pass "'wb pending' lists the paused workbook"
else
    fail "'wb pending' did not list the checkpoint (output: $PENDING_OUTPUT)"
fi

# --- Step 5: wb pending --format json outputs valid JSON ---
echo "Step 5: Run 'wb pending --format json' and validate JSON"
set +e
JSON_OUTPUT=$("$WB" pending --format json 2>/dev/null)
JSON_EXIT=$?
set -e

if echo "$JSON_OUTPUT" | python3 -m json.tool >/dev/null 2>&1; then
    pass "'wb pending --format json' outputs valid JSON"
else
    fail "'wb pending --format json' did not output valid JSON (output: $JSON_OUTPUT)"
fi

# Check JSON contains the checkpoint id
if echo "$JSON_OUTPUT" | grep -q "$CHECKPOINT_ID"; then
    pass "JSON output contains checkpoint id"
else
    fail "JSON output missing checkpoint id"
fi

# --- Step 6: Resume with --value ---
echo "Step 6: Resume workbook with --value 998877"
set +e
RESUME_OUTPUT=$("$WB" resume "$CHECKPOINT_ID" --value 998877 2>&1)
RESUME_EXIT=$?
set -e

if [ "$RESUME_EXIT" -eq 0 ]; then
    pass "resume exit code is 0"
else
    fail "resume exit code was $RESUME_EXIT (expected 0)"
    echo "    resume output: $RESUME_OUTPUT"
fi

# Check that the resumed block used the bound value
if echo "$RESUME_OUTPUT" | grep -q "998877"; then
    pass "resumed block received the bound value (998877)"
else
    fail "bound value 998877 not found in resume output"
    echo "    resume output: $RESUME_OUTPUT"
fi

# --- Step 7: Check pending descriptor was cleaned up ---
echo "Step 7: Check pending descriptor was cleaned up after resume"
if [ ! -f "$CHECKPOINT_DIR/${CHECKPOINT_ID}.pending.json" ]; then
    pass "pending descriptor cleaned up after resume"
else
    fail "pending descriptor still exists after resume"
fi

# --- Summary ---
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

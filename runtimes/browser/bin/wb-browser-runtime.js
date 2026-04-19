#!/usr/bin/env node
// wb-browser-runtime — minimal sidecar skeleton.
//
// Speaks wb's line-framed JSON protocol on stdio. See README.md.
//
// This skeleton echoes each verb as verb.complete. Demo-only behavior:
//   - verb `wait_for_mfa:` / `wait_for_email_otp:`       → emits slice.paused
//     with an opaque sidecar_state blob so wb can persist + resume.
//   - verb `act: ...` (the "AI recovery" verb)          → emits slice.recovered
//     before verb.complete so the callback flow is exercised.
//
// The real runtime (Playwright/Stagehand/Browserbase) will replace these echo
// paths but the protocol stays the same.

import readline from "node:readline";

const SUPPORTS = [
  "goto",
  "click",
  "fill",
  "upload",
  "download",
  "assert",
  "screenshot",
  "act",
  "wait_for_mfa",
  "wait_for_email_otp",
];

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function log(...args) {
  process.stderr.write(args.join(" ") + "\n");
}

function verbName(verb) {
  if (!verb || typeof verb !== "object") return String(verb);
  const keys = Object.keys(verb);
  return keys[0] || "verb";
}

function verbSummary(verb) {
  if (!verb || typeof verb !== "object") return "";
  const k = verbName(verb);
  const v = verb[k];
  if (typeof v === "string") return v.slice(0, 60);
  if (v && typeof v === "object") {
    return Object.entries(v)
      .map(([kk, vv]) => `${kk}=${typeof vv === "string" ? vv.slice(0, 20) : JSON.stringify(vv).slice(0, 20)}`)
      .join(" ");
  }
  return "";
}

function handleSlice(msg) {
  const verbs = Array.isArray(msg.verbs) ? msg.verbs : [];
  const session = msg.session || "-";
  const restore = msg.restore || null;

  if (restore) {
    const resumedAt = (restore.state && restore.state.verb_index) ?? 0;
    log(`[restore] state=${JSON.stringify(restore.state || {})} signal=${JSON.stringify(restore.signal || null)}`);
    // Jump straight past whatever pause was recorded.
    for (let i = resumedAt; i < verbs.length; i++) {
      if (!runVerb(verbs[i], i, true)) return;
    }
    send({ type: "slice.complete" });
    return;
  }

  log(`[slice] session=${session} verbs=${verbs.length}`);
  for (let i = 0; i < verbs.length; i++) {
    if (!runVerb(verbs[i], i, false)) return;
  }
  send({ type: "slice.complete" });
}

function runVerb(verb, index, isResume) {
  const name = verbName(verb);
  if (!SUPPORTS.includes(name)) {
    send({ type: "verb.failed", verb: name, error: `unsupported verb: ${name}` });
    send({ type: "slice.failed", error: `unsupported verb: ${name}` });
    return false;
  }

  // Demo-only: treat any pause-like verb as a human-in-the-loop pause.
  if ((name === "wait_for_mfa" || name === "wait_for_email_otp") && !isResume) {
    const reason =
      name === "wait_for_mfa" ? "totp_required" : "email_otp_required";
    send({
      type: "slice.paused",
      verb: name,
      verb_index: index,
      reason,
      resume_url: "https://browserbase.example/live/demo-session",
      sidecar_state: {
        verb_index: index,
        nav: "/login",
        cookie_state: "pre-mfa",
      },
    });
    return false; // stop processing; wb will save pending + exit 42
  }

  // Demo-only: emit slice.recovered for `act:` verbs to exercise the event
  // bridge. A real runtime would only fire this when AI recovery rescued a
  // missed selector.
  if (name === "act") {
    send({
      type: "slice.recovered",
      verb: name,
      verb_index: index,
      original_selector: "button.approve",
      recovered_selector: "[data-testid=approve]",
      recovered_strategy: "stagehand_act",
    });
  }

  send({
    type: "verb.complete",
    verb: name,
    verb_index: index,
    summary: verbSummary(verb),
  });
  return true;
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch (e) {
    log(`[warn] ignoring non-JSON input: ${trimmed.slice(0, 80)}`);
    return;
  }

  switch (msg.type) {
    case "hello":
      send({
        type: "ready",
        runtime: "wb-browser-runtime",
        version: "0.1.0",
        protocol: "wb-sidecar/1",
        supports: SUPPORTS,
      });
      break;
    case "slice":
      handleSlice(msg);
      break;
    case "shutdown":
      log("[shutdown] closing");
      process.exit(0);
      break;
    default:
      log(`[warn] unknown message type: ${msg.type}`);
  }
});

rl.on("close", () => {
  process.exit(0);
});

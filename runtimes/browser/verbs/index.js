// Verb registry. Each verb module exports a default { name, primaryKey,
// execute(page, args, ctx) } object. The registry is the single source of
// truth for the SUPPORTS list (shipped in the ready frame), the default-key
// lookup used by the bare-string arg form, and the dispatch table consumed
// by runVerb.
//
// Adding a verb: drop a new file next to these, import it here, append to
// VERBS. SUPPORTS/DEFAULT_KEYS/VERB_REGISTRY all derive automatically — no
// third list to keep in sync.

import gotoVerb from "./goto.js";
import fillVerb from "./fill.js";
import clickVerb from "./click.js";
import pressVerb from "./press.js";
import waitForVerb from "./wait_for.js";
import screenshotVerb from "./screenshot.js";
import extractVerb from "./extract.js";
import assertVerb from "./assert.js";
import evalVerb from "./eval.js";
import saveVerb from "./save.js";

const VERBS = [
  gotoVerb,
  fillVerb,
  clickVerb,
  pressVerb,
  waitForVerb,
  screenshotVerb,
  extractVerb,
  assertVerb,
  evalVerb,
  saveVerb,
];

export const VERB_REGISTRY = Object.fromEntries(VERBS.map((v) => [v.name, v]));
export const SUPPORTS = VERBS.map((v) => v.name);

export function verbName(verb) {
  if (!verb || typeof verb !== "object") return String(verb);
  return Object.keys(verb)[0] || "verb";
}

export function defaultKey(name) {
  return VERB_REGISTRY[name]?.primaryKey ?? "value";
}

// Most verbs accept either a bare string ("goto: https://...") or a
// structured object ("goto: { url: ..., wait_until: ... }"). This pulls the
// canonical field out of either shape.
export function arg(value, primaryKey) {
  if (typeof value === "string") return { [primaryKey]: value };
  if (value && typeof value === "object") return value;
  return {};
}

// Dispatch a single verb. `expand` is injected by the caller so the
// substitution/secrets machinery stays in the entry point (where env policy
// and the artifact cache live) instead of leaking into this module.
export async function runVerb(page, verb, index, ctx, expand) {
  const name = verbName(verb);
  const handler = VERB_REGISTRY[name];
  if (!handler) throw new Error(`unsupported verb: ${name}`);
  const raw = verb[name];
  const args = expand(
    arg(raw, handler.primaryKey),
    ctx?.secrets,
    ctx?.artifactCache,
  );
  return handler.execute(page, args, { ...ctx, index });
}

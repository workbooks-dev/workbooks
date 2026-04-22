// Verb-registry tests. Runs under Node's built-in test runner
// (`node --test` or `npm test`). No Browserbase credentials, no network.
//
// Pattern: construct a stub Page, invoke `VERB_REGISTRY[name].execute(page,
// args, ctx)`, then assert on the stub's recorded calls + the returned
// summary. Verbs that touch the filesystem (screenshot, save) use a real
// tmpdir and clean up via t.after.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  VERB_REGISTRY,
  SUPPORTS,
  defaultKey,
  verbName,
  arg,
} from "../verbs/index.js";
import { createStubPage, captureSendFrames } from "../lib/stub-page.js";

// --- registry shape ---------------------------------------------------------

test("SUPPORTS lists all 10 verbs in expected order", () => {
  assert.deepEqual(SUPPORTS, [
    "goto",
    "fill",
    "click",
    "press",
    "wait_for",
    "screenshot",
    "extract",
    "assert",
    "eval",
    "save",
  ]);
});

test("every verb module exports { name, primaryKey, execute }", () => {
  for (const name of SUPPORTS) {
    const v = VERB_REGISTRY[name];
    assert.equal(v.name, name, `${name}.name mismatch`);
    assert.equal(typeof v.primaryKey, "string", `${name}.primaryKey`);
    assert.equal(typeof v.execute, "function", `${name}.execute`);
  }
});

test("defaultKey() matches each verb's primaryKey", () => {
  for (const name of SUPPORTS) {
    assert.equal(defaultKey(name), VERB_REGISTRY[name].primaryKey);
  }
});

test("defaultKey() falls back to 'value' for unknown verbs", () => {
  assert.equal(defaultKey("not-a-verb"), "value");
});

test("verbName() extracts the key from { verb: args } shapes", () => {
  assert.equal(verbName({ goto: "https://x" }), "goto");
  assert.equal(verbName({ click: { selector: ".x" } }), "click");
});

test("arg() wraps bare strings into { primaryKey: value }", () => {
  assert.deepEqual(arg("https://x", "url"), { url: "https://x" });
  assert.deepEqual(arg({ url: "y", timeout: 5 }, "url"), {
    url: "y",
    timeout: 5,
  });
  assert.deepEqual(arg(null, "url"), {});
});

// --- goto -------------------------------------------------------------------

test("goto navigates + returns summary", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.goto.execute(page, {
    url: "https://example.com",
  });
  assert.equal(page.calls.length, 1);
  assert.equal(page.calls[0].verb, "goto");
  assert.equal(page.calls[0].url, "https://example.com");
  assert.equal(page.calls[0].options.waitUntil, "domcontentloaded");
  assert.equal(page.calls[0].options.timeout, 30_000);
  assert.equal(summary, "→ https://example.com");
});

test("goto honors custom wait_until + timeout", async () => {
  const page = createStubPage();
  await VERB_REGISTRY.goto.execute(page, {
    url: "https://x",
    wait_until: "networkidle",
    timeout: 5000,
  });
  assert.equal(page.calls[0].options.waitUntil, "networkidle");
  assert.equal(page.calls[0].options.timeout, 5000);
});

// --- fill -------------------------------------------------------------------

test("fill submits value + redacts in summary", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.fill.execute(page, {
    selector: "#email",
    value: "secret@domain.com",
  });
  assert.equal(page.calls[0].verb, "fill");
  assert.equal(page.calls[0].value, "secret@domain.com");
  // Value must not appear in the summary under any circumstance.
  assert.ok(!summary.includes("secret@domain.com"));
  assert.equal(summary, "#email = «se***»");
});

test("fill redacts short values completely", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.fill.execute(page, {
    selector: "#x",
    value: "abc",
  });
  assert.equal(summary, "#x = «***»");
});

test("fill coerces non-string values to strings", async () => {
  const page = createStubPage();
  await VERB_REGISTRY.fill.execute(page, { selector: "#x", value: 42 });
  assert.equal(page.calls[0].value, "42");
});

// --- click ------------------------------------------------------------------

test("click clicks selector with default timeout", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.click.execute(page, {
    selector: ".btn",
  });
  assert.equal(page.calls[0].verb, "click");
  assert.equal(page.calls[0].options.timeout, 10_000);
  assert.equal(summary, ".btn");
});

// --- press ------------------------------------------------------------------

test("press defaults to body selector", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.press.execute(page, { key: "Enter" });
  assert.equal(page.calls[0].selector, "body");
  assert.equal(page.calls[0].key, "Enter");
  assert.match(summary, /body ⌨ Enter/);
});

test("press honors custom selector", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.press.execute(page, {
    selector: "#search",
    key: "Enter",
  });
  assert.equal(page.calls[0].selector, "#search");
  assert.match(summary, /#search ⌨ Enter/);
});

// --- wait_for ---------------------------------------------------------------

test("wait_for defaults to 'visible' state + 15s timeout", async () => {
  const page = createStubPage();
  const summary = await VERB_REGISTRY.wait_for.execute(page, {
    selector: ".done",
  });
  assert.equal(page.calls[0].options.state, "visible");
  assert.equal(page.calls[0].options.timeout, 15_000);
  assert.equal(summary, ".done (visible)");
});

test("wait_for honors custom state + timeout", async () => {
  const page = createStubPage();
  await VERB_REGISTRY.wait_for.execute(page, {
    selector: ".gone",
    state: "detached",
    timeout: 2000,
  });
  assert.equal(page.calls[0].options.state, "detached");
  assert.equal(page.calls[0].options.timeout, 2000);
});

// --- extract ----------------------------------------------------------------

test("extract returns row-count summary + stores rows in ctx.lastResult", async () => {
  const rows = [{ name: "a" }, { name: "b" }, { name: "c" }];
  const page = createStubPage({ extractResult: rows });
  const ctx = {};
  const summary = await VERB_REGISTRY.extract.execute(
    page,
    { selector: ".row", fields: { name: ".name" } },
    ctx,
  );
  assert.equal(ctx.lastResult, rows);
  assert.match(summary, /\.row → 3 rows/);
});

// --- assert -----------------------------------------------------------------

test("assert throws when selector not found", async () => {
  const page = createStubPage({ handles: {} });
  await assert.rejects(
    () => VERB_REGISTRY.assert.execute(page, { selector: "#nope" }),
    /selector not found/,
  );
});

test("assert passes when selector exists + text matches", async () => {
  const page = createStubPage({
    handles: { "#ok": { textContent: "welcome home" } },
  });
  const summary = await VERB_REGISTRY.assert.execute(page, {
    selector: "#ok",
    text_contains: "welcome",
  });
  assert.equal(summary, "#ok");
});

test("assert throws when text does not contain expected substring", async () => {
  const page = createStubPage({
    handles: { "#ok": { textContent: "goodbye" } },
  });
  await assert.rejects(
    () =>
      VERB_REGISTRY.assert.execute(page, {
        selector: "#ok",
        text_contains: "welcome",
      }),
    /text "welcome" not in/,
  );
});

test("assert url_contains passes when url matches", async () => {
  const page = createStubPage({
    initialUrl: "https://app.example.com/dashboard",
    handles: { body: { textContent: "" } },
  });
  const summary = await VERB_REGISTRY.assert.execute(page, {
    selector: "body",
    url_contains: "dashboard",
  });
  assert.equal(summary, "body");
});

test("assert url_contains throws when url does not match", async () => {
  const page = createStubPage({
    initialUrl: "https://wrong.example.com",
    handles: { body: { textContent: "" } },
  });
  await assert.rejects(
    () =>
      VERB_REGISTRY.assert.execute(page, {
        selector: "body",
        url_contains: "expected.com/foo",
      }),
    /url does not contain/,
  );
});

// --- eval -------------------------------------------------------------------

test("eval sets ctx.lastResult to page.evaluate's return", async () => {
  const page = createStubPage({ evalResult: { x: 42 } });
  const ctx = {};
  const summary = await VERB_REGISTRY.eval.execute(
    page,
    { script: "1+1" },
    ctx,
  );
  assert.deepEqual(ctx.lastResult, { x: 42 });
  assert.equal(summary, "script ran");
});

// --- screenshot (filesystem) -----------------------------------------------

async function withArtifactsDir(t) {
  const dir = await mkdtemp(path.join(tmpdir(), "wb-test-"));
  const prev = process.env.WB_ARTIFACTS_DIR;
  process.env.WB_ARTIFACTS_DIR = dir;
  t.after(() => {
    if (prev === undefined) delete process.env.WB_ARTIFACTS_DIR;
    else process.env.WB_ARTIFACTS_DIR = prev;
  });
  return dir;
}

test("screenshot writes PNG atomically into WB_ARTIFACTS_DIR", async (t) => {
  const dir = await withArtifactsDir(t);
  const page = createStubPage();
  const summary = await VERB_REGISTRY.screenshot.execute(page, {
    path: "shot.png",
  });
  const file = await readFile(path.join(dir, "shot.png"));
  assert.ok(file.length > 0, "screenshot file should be non-empty");
  assert.equal(summary, "→ shot.png");
  // No leftover .tmp files.
  await assert.rejects(() =>
    access(path.join(dir, "shot.png.tmp")),
  );
});

test("screenshot creates nested subdirs + honors fullPage flag", async (t) => {
  const dir = await withArtifactsDir(t);
  const page = createStubPage();
  await VERB_REGISTRY.screenshot.execute(page, {
    path: "nested/deeper/shot.png",
    full_page: true,
  });
  const file = await readFile(path.join(dir, "nested/deeper/shot.png"));
  assert.ok(file.length > 0);
  assert.equal(page.calls[0].options.fullPage, true);
});

test("screenshot rejects absolute paths", async () => {
  const page = createStubPage();
  await assert.rejects(
    () => VERB_REGISTRY.screenshot.execute(page, { path: "/etc/passwd" }),
    /absolute paths are not allowed/,
  );
});

test("screenshot rejects traversal out of artifacts dir", async (t) => {
  await withArtifactsDir(t);
  const page = createStubPage();
  await assert.rejects(
    () =>
      VERB_REGISTRY.screenshot.execute(page, {
        path: "../../etc/passwd",
      }),
    /escapes artifacts dir/,
  );
});

test("screenshot picks jpeg type from .jpg extension", async (t) => {
  await withArtifactsDir(t);
  const page = createStubPage();
  await VERB_REGISTRY.screenshot.execute(page, { path: "photo.jpg" });
  assert.equal(page.calls[0].options.type, "jpeg");
});

// --- save (filesystem + send frame) ----------------------------------------

test("save writes JSON atomically + emits slice.artifact_saved", async (t) => {
  const dir = await withArtifactsDir(t);
  const capture = captureSendFrames();
  t.after(capture.dispose);

  const ctx = { lastResult: { hello: "world", n: 3 } };
  const summary = await VERB_REGISTRY.save.execute(
    null,
    { name: "greeting" },
    ctx,
  );

  const written = await readFile(path.join(dir, "greeting.json"), "utf8");
  assert.deepEqual(JSON.parse(written), { hello: "world", n: 3 });
  assert.equal(summary, "→ greeting.json");

  const savedFrame = capture.frames.find(
    (f) => f.type === "slice.artifact_saved",
  );
  assert.ok(savedFrame, "slice.artifact_saved should be emitted");
  assert.equal(savedFrame.filename, "greeting.json");
  assert.ok(savedFrame.bytes > 0);
});

test("save captures explicit value: over ctx.lastResult", async (t) => {
  const dir = await withArtifactsDir(t);
  const capture = captureSendFrames();
  t.after(capture.dispose);

  const ctx = { lastResult: "should-be-ignored" };
  await VERB_REGISTRY.save.execute(null, { name: "x", value: { a: 1 } }, ctx);

  const written = await readFile(path.join(dir, "x.json"), "utf8");
  assert.deepEqual(JSON.parse(written), { a: 1 });
});

test("save auto-names when name missing + ctx.blockIndex present", async (t) => {
  const dir = await withArtifactsDir(t);
  const capture = captureSendFrames();
  t.after(capture.dispose);

  await VERB_REGISTRY.save.execute(
    null,
    { value: { n: 1 } },
    { blockIndex: 7 },
  );
  const saved = capture.frames.find((f) => f.type === "slice.artifact_saved");
  assert.match(saved.filename, /^cell-7-[0-9a-f]{8}\.json$/);
});

test("save throws when no payload available", async (t) => {
  await withArtifactsDir(t);
  await assert.rejects(
    () => VERB_REGISTRY.save.execute(null, {}, {}),
    /no value provided/,
  );
});

test("save throws when WB_ARTIFACTS_DIR is not set", async (t) => {
  const prev = process.env.WB_ARTIFACTS_DIR;
  delete process.env.WB_ARTIFACTS_DIR;
  t.after(() => {
    if (prev !== undefined) process.env.WB_ARTIFACTS_DIR = prev;
  });

  await assert.rejects(
    () =>
      VERB_REGISTRY.save.execute(
        null,
        { name: "x" },
        { lastResult: "y" },
      ),
    /WB_ARTIFACTS_DIR is not set/,
  );
});

test("save sanitizes path separators + whitespace in names", async (t) => {
  const dir = await withArtifactsDir(t);
  const capture = captureSendFrames();
  t.after(capture.dispose);

  await VERB_REGISTRY.save.execute(
    null,
    { name: "../escape/name with spaces" },
    { lastResult: { ok: true } },
  );
  const saved = capture.frames.find((f) => f.type === "slice.artifact_saved");
  // Slashes + whitespace must not survive — that's what prevents the name
  // from ever being a multi-component path. `..` as a filename prefix is
  // harmless once `/` is gone, since path.join glues it onto artifactsDir
  // as one component (so no traversal is possible).
  assert.ok(!saved.filename.includes("/"));
  assert.ok(!saved.filename.includes(" "));
  // And the written file must actually live inside artifactsDir.
  const resolved = path.resolve(saved.path);
  assert.ok(resolved.startsWith(path.resolve(dir) + path.sep));
});

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

test("SUPPORTS lists all 12 verbs in expected order", () => {
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
    "pause_for_human",
    "wait_for_drop",
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
    { script: "return 1+1" },
    ctx,
  );
  assert.deepEqual(ctx.lastResult, { x: 42 });
  assert.equal(summary, "script ran");
});

test("eval wraps script in async IIFE so top-level return + await work", async () => {
  const page = createStubPage({ evalResult: null });
  await VERB_REGISTRY.eval.execute(
    page,
    { script: "console.log('seeded'); return 'ok';" },
    {},
  );
  const call = page.calls.find((c) => c.verb === "evaluate");
  assert.ok(
    /^\(async \(\) => \{ .* \}\)\(\)$/.test(call.script),
    `expected async IIFE wrap, got: ${call.script}`,
  );
  assert.ok(
    call.script.includes("return 'ok';"),
    "wrapped script must contain the original return statement",
  );
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

// --- pause_for_human --------------------------------------------------------

test("pause_for_human returns __pause sentinel with operator_click default", async () => {
  const result = await VERB_REGISTRY.pause_for_human.execute(
    null,
    { message: "Complete MFA in the open browser" },
    { index: 2 },
  );
  assert.ok(result.__pause, "should return __pause sentinel");
  const p = result.__pause;
  assert.equal(p.reason, "pause_for_human");
  assert.equal(p.message, "Complete MFA in the open browser");
  assert.equal(p.context_url, null);
  assert.equal(p.resume_on, "operator_click");
  assert.deepEqual(p.actions, [{ label: "Resume", value: null }]);
  assert.equal(p.timeout, null);
});

test("pause_for_human forwards context_url + resume_on + timeout", async () => {
  const result = await VERB_REGISTRY.pause_for_human.execute(
    null,
    {
      message: "Drop receipts in the folder below",
      context_url: "https://drive.google.com/drive/folders/abc",
      resume_on: "timeout",
      timeout: "1h",
    },
    { index: 0 },
  );
  const p = result.__pause;
  assert.equal(p.context_url, "https://drive.google.com/drive/folders/abc");
  assert.equal(p.resume_on, "timeout");
  assert.equal(p.timeout, "1h");
});

test("pause_for_human preserves custom actions list", async () => {
  const result = await VERB_REGISTRY.pause_for_human.execute(
    null,
    {
      message: "Approve?",
      actions: [
        { label: "Approved", value: "approved" },
        { label: "Denied", value: "denied" },
      ],
    },
    { index: 0 },
  );
  const p = result.__pause;
  assert.equal(p.actions.length, 2);
  assert.equal(p.actions[0].value, "approved");
  assert.equal(p.actions[1].value, "denied");
});

test("pause_for_human rejects invalid resume_on", async () => {
  await assert.rejects(
    VERB_REGISTRY.pause_for_human.execute(
      null,
      { message: "x", resume_on: "whenever" },
      { index: 0 },
    ),
    /resume_on must be one of/,
  );
});

test("pause_for_human rejects malformed action entries", async () => {
  await assert.rejects(
    VERB_REGISTRY.pause_for_human.execute(
      null,
      { message: "x", actions: [{ value: "no-label" }] },
      { index: 0 },
    ),
    /each action must be/,
  );
});

// --- wait_for_drop (argument validation only; network tests live elsewhere) -

async function callWaitForDrop(args, envOverrides = {}) {
  const savedEnv = {};
  for (const [k, v] of Object.entries(envOverrides)) {
    savedEnv[k] = process.env[k];
    if (v === null) delete process.env[k];
    else process.env[k] = v;
  }
  try {
    return await VERB_REGISTRY.wait_for_drop.execute(null, args, { index: 0 });
  } finally {
    for (const [k, v] of Object.entries(savedEnv)) {
      if (v === undefined) delete process.env[k];
      else process.env[k] = v;
    }
  }
}

test("wait_for_drop rejects missing PARACORD_RELAY_API_KEY", async () => {
  await assert.rejects(
    callWaitForDrop(
      { folder_url: "https://drive.google.com/drive/folders/abc" },
      { PARACORD_RELAY_API_KEY: null, PARACORD_RELAY_URL: "https://r" },
    ),
    /PARACORD_RELAY_API_KEY is required/,
  );
});

test("wait_for_drop rejects missing PARACORD_RELAY_URL", async () => {
  await assert.rejects(
    callWaitForDrop(
      { folder_url: "https://drive.google.com/drive/folders/abc" },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: null },
    ),
    /PARACORD_RELAY_URL is required/,
  );
});

test("wait_for_drop rejects malformed folder_url", async () => {
  await assert.rejects(
    callWaitForDrop(
      { folder_url: "https://not-drive.example.com/path" },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://r" },
    ),
    /does not look like a Drive folder URL/,
  );
});

test("wait_for_drop rejects invalid expect value", async () => {
  await assert.rejects(
    callWaitForDrop(
      {
        folder_url: "https://drive.google.com/drive/folders/abc",
        expect: "something_else",
      },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://r" },
    ),
    /expect must be/,
  );
});

test("wait_for_drop rejects filename_matches without pattern", async () => {
  await assert.rejects(
    callWaitForDrop(
      {
        folder_url: "https://drive.google.com/drive/folders/abc",
        expect: "filename_matches",
      },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://r" },
    ),
    /filename_matches requires filename_pattern/,
  );
});

test("wait_for_drop rejects bogus duration string", async () => {
  await assert.rejects(
    callWaitForDrop(
      {
        folder_url: "https://drive.google.com/drive/folders/abc",
        poll_every: "sometime",
      },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://r" },
    ),
    /invalid duration/,
  );
});

test("wait_for_drop accepts a user folder URL (with /u/0/)", async () => {
  // We stub fetch to return one file so the verb returns quickly. This also
  // proves the extractFolderId regex handles the `/u/<n>/` personal-drive
  // prefix that Google uses when the operator is signed into multiple accs.
  const savedFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return { files: [{ id: "f1", name: "x.pdf" }] };
    },
  });
  const savedDir = process.env.WB_ARTIFACTS_DIR;
  const tmp = await mkdtemp(path.join(tmpdir(), "wb-drop-"));
  process.env.WB_ARTIFACTS_DIR = tmp;
  try {
    const summary = await callWaitForDrop(
      { folder_url: "https://drive.google.com/drive/u/0/folders/ABC123_xyz" },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://relay" },
    );
    assert.match(summary, /matched/);
  } finally {
    globalThis.fetch = savedFetch;
    if (savedDir === undefined) delete process.env.WB_ARTIFACTS_DIR;
    else process.env.WB_ARTIFACTS_DIR = savedDir;
  }
});

test("wait_for_drop writes bind_artifact to WB_ARTIFACTS_DIR", async () => {
  const savedFetch = globalThis.fetch;
  const files = [
    { id: "a", name: "statement-2026-04.pdf" },
    { id: "b", name: "receipts.pdf" },
  ];
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return { files };
    },
  });
  const tmp = await mkdtemp(path.join(tmpdir(), "wb-drop-"));
  const savedDir = process.env.WB_ARTIFACTS_DIR;
  process.env.WB_ARTIFACTS_DIR = tmp;
  try {
    await callWaitForDrop(
      {
        folder_url: "https://drive.google.com/drive/folders/abc",
        bind_artifact: "uploaded",
      },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://relay" },
    );
    const content = await readFile(path.join(tmp, "uploaded.json"), "utf8");
    const parsed = JSON.parse(content);
    assert.equal(parsed.files.length, 2);
    assert.equal(parsed.files[0].name, "statement-2026-04.pdf");
  } finally {
    globalThis.fetch = savedFetch;
    if (savedDir === undefined) delete process.env.WB_ARTIFACTS_DIR;
    else process.env.WB_ARTIFACTS_DIR = savedDir;
  }
});

test("wait_for_drop filename_matches filters with glob", async () => {
  // Directory has two files; only one matches *.csv. The verb should match
  // on the first poll and return.
  const savedFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    async json() {
      return {
        files: [
          { id: "a", name: "notes.txt" },
          { id: "b", name: "data.csv" },
        ],
      };
    },
  });
  const tmp = await mkdtemp(path.join(tmpdir(), "wb-drop-"));
  const savedDir = process.env.WB_ARTIFACTS_DIR;
  process.env.WB_ARTIFACTS_DIR = tmp;
  try {
    const summary = await callWaitForDrop(
      {
        folder_url: "https://drive.google.com/drive/folders/abc",
        expect: "filename_matches",
        filename_pattern: "*.csv",
      },
      { PARACORD_RELAY_API_KEY: "k", PARACORD_RELAY_URL: "https://relay" },
    );
    assert.match(summary, /matched/);
  } finally {
    globalThis.fetch = savedFetch;
    if (savedDir === undefined) delete process.env.WB_ARTIFACTS_DIR;
    else process.env.WB_ARTIFACTS_DIR = savedDir;
  }
});

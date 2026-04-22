// SessionManager covers two behaviors that the entry point depends on:
//   1. `ensure(name, createFn)` dedups in-flight creates for the same name.
//      Without this, two concurrent slices for "vendor-x" would each call
//      bbCreateSession, burning two Browserbase sessions.
//   2. `enqueueOn(name, fn)` serializes work per name but runs distinct
//      names in parallel. `drainAll()` awaits everything queued.

import { test } from "node:test";
import assert from "node:assert/strict";
import { SessionManager } from "../lib/session-manager.js";

// Small helper — a deferred promise so tests can control timing.
function deferred() {
  let resolve, reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// --- ensure ----------------------------------------------------------------

test("ensure returns the created info and caches it", async () => {
  const mgr = new SessionManager();
  const info = await mgr.ensure("a", async () => ({ id: "session-a" }));
  assert.deepEqual(info, { id: "session-a" });
  assert.ok(mgr.has("a"));
  assert.equal(mgr.get("a"), info);
});

test("ensure hits the cache on repeat calls + skips createFn", async () => {
  const mgr = new SessionManager();
  let calls = 0;
  const createFn = async () => {
    calls++;
    return { n: calls };
  };
  const a = await mgr.ensure("x", createFn);
  const b = await mgr.ensure("x", createFn);
  assert.equal(calls, 1);
  assert.equal(a, b);
});

test("ensure dedups concurrent creates for the same name", async () => {
  const mgr = new SessionManager();
  const d = deferred();
  let calls = 0;
  const createFn = async () => {
    calls++;
    await d.promise;
    return { n: calls };
  };
  // Kick off two concurrent ensures — both should await the same promise.
  const p1 = mgr.ensure("same", createFn);
  const p2 = mgr.ensure("same", createFn);
  d.resolve();
  const [a, b] = await Promise.all([p1, p2]);
  assert.equal(calls, 1, "createFn should run exactly once");
  assert.equal(a, b);
});

test("ensure allows retry after a failed create (no poisoned entry)", async () => {
  const mgr = new SessionManager();
  let calls = 0;
  await assert.rejects(
    () =>
      mgr.ensure("flaky", async () => {
        calls++;
        throw new Error("boom");
      }),
    /boom/,
  );
  assert.ok(!mgr.has("flaky"), "failed create must not cache");
  // Second attempt with a passing createFn must actually run.
  const info = await mgr.ensure("flaky", async () => {
    calls++;
    return { ok: true };
  });
  assert.equal(calls, 2);
  assert.deepEqual(info, { ok: true });
});

test("ensure runs creates for distinct names in parallel", async () => {
  const mgr = new SessionManager();
  const dA = deferred();
  const dB = deferred();
  const pA = mgr.ensure("a", async () => {
    await dA.promise;
    return "A";
  });
  const pB = mgr.ensure("b", async () => {
    await dB.promise;
    return "B";
  });
  // Resolve B first — it must not be blocked by A.
  dB.resolve();
  const b = await pB;
  assert.equal(b, "B");
  dA.resolve();
  const a = await pA;
  assert.equal(a, "A");
});

// --- enqueueOn -------------------------------------------------------------

test("enqueueOn serializes work against the same name", async () => {
  const mgr = new SessionManager();
  const order = [];
  const p1 = mgr.enqueueOn("x", async () => {
    await new Promise((r) => setImmediate(r));
    order.push("first");
  });
  const p2 = mgr.enqueueOn("x", async () => {
    order.push("second");
  });
  await Promise.all([p1, p2]);
  assert.deepEqual(order, ["first", "second"]);
});

test("enqueueOn lets distinct names run concurrently", async () => {
  const mgr = new SessionManager();
  const dA = deferred();
  const order = [];
  const pA = mgr.enqueueOn("a", async () => {
    await dA.promise;
    order.push("a");
  });
  const pB = mgr.enqueueOn("b", async () => {
    order.push("b");
  });
  await pB;
  dA.resolve();
  await pA;
  // B ran while A was still waiting — parallel dispatch works.
  assert.deepEqual(order, ["b", "a"]);
});

test("enqueueOn does not poison subsequent work when a task throws", async () => {
  const mgr = new SessionManager();
  const failure = mgr.enqueueOn("y", async () => {
    throw new Error("nope");
  });
  await assert.rejects(() => failure, /nope/);
  // The next task on the same name must still run.
  let ran = false;
  await mgr.enqueueOn("y", async () => {
    ran = true;
  });
  assert.ok(ran);
});

// --- drainAll --------------------------------------------------------------

test("drainAll resolves after all queued work settles", async () => {
  const mgr = new SessionManager();
  const dA = deferred();
  const dB = deferred();
  let aDone = false;
  let bDone = false;
  mgr.enqueueOn("a", async () => {
    await dA.promise;
    aDone = true;
  });
  mgr.enqueueOn("b", async () => {
    await dB.promise;
    bDone = true;
  });
  dA.resolve();
  dB.resolve();
  await mgr.drainAll();
  assert.ok(aDone && bDone, "drainAll must await both chains");
});

test("drainAll swallows per-chain rejections (allSettled semantics)", async () => {
  const mgr = new SessionManager();
  mgr.enqueueOn("a", async () => {
    throw new Error("boom");
  }).catch(() => {}); // caller-level catch to avoid unhandled rejection
  mgr.enqueueOn("b", async () => {});
  // drainAll itself should never throw.
  await mgr.drainAll();
});

// --- iteration + Map-like API ---------------------------------------------

test("SessionManager iterates as [name, info] pairs like Map", async () => {
  const mgr = new SessionManager();
  await mgr.ensure("a", async () => ({ id: "A" }));
  await mgr.ensure("b", async () => ({ id: "B" }));
  const seen = [];
  for (const [name, info] of mgr) seen.push([name, info.id]);
  seen.sort();
  assert.deepEqual(seen, [
    ["a", "A"],
    ["b", "B"],
  ]);
});

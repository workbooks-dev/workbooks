// Per-name session cache with in-flight create dedup.
//
// Today the slice enqueue is global (one chain, all sessions serialized),
// so concurrent ensures for the same name never happen — the second caller
// always sees the cache populated. Once slice dispatch moves to per-session
// chains (see Phase 4), two concurrent slices for "vendor-x" would both
// race bbCreateSession and burn two Browserbase sessions. Deduping the
// in-flight promise here fixes that race up-front, so the per-session
// chain change in Phase 4 is a one-liner in main.js instead of a recursive
// bug hunt later.
//
// The manager is creation-logic-free on purpose: callers hand an async
// factory to `ensure()`, which is invoked at most once per name. The
// factory is responsible for its own cleanup on throw — on rejection the
// in-flight entry is dropped so a subsequent caller can retry.

export class SessionManager {
  constructor() {
    this._sessions = new Map();
    this._inFlight = new Map();
    this._chains = new Map();
  }

  get size() {
    return this._sessions.size;
  }

  has(name) {
    return this._sessions.has(name);
  }

  get(name) {
    return this._sessions.get(name);
  }

  delete(name) {
    return this._sessions.delete(name);
  }

  entries() {
    return this._sessions.entries();
  }

  values() {
    return this._sessions.values();
  }

  // Match Map's default iterator (yields [name, info] pairs) so callers can
  // write `for (const [name, info] of manager)` the same way they would
  // against the underlying Map.
  [Symbol.iterator]() {
    return this._sessions.entries();
  }

  // Serialize work against a single session name (so two slices against
  // the same Playwright page don't race), but let distinct names run in
  // parallel. Before this, the entry point held a single global promise
  // chain — two slices against "vendor-a" and "vendor-b" serialized even
  // though they touch disjoint browsers. The in-flight-create dedup in
  // `ensure()` is what makes per-session parallelism safe here: two
  // concurrent slices for the same name still share one bbCreateSession.
  //
  // `fn` should return a promise; errors propagate to the returned
  // promise and don't poison the next link in the chain.
  enqueueOn(name, fn) {
    const prev = this._chains.get(name) ?? Promise.resolve();
    const next = prev.catch(() => {}).then(fn);
    this._chains.set(name, next);
    return next;
  }

  // Resolve once every currently-queued chain has settled. Used by
  // shutdown to wait for in-flight slices before closing browsers and
  // releasing Browserbase sessions. Only observes chains that exist at
  // call time — later enqueues aren't awaited, which is the correct
  // behavior for shutdown (the main loop stops accepting messages first).
  async drainAll() {
    const chains = Array.from(this._chains.values());
    await Promise.allSettled(chains);
  }

  async ensure(name, createFn) {
    if (this._sessions.has(name)) return this._sessions.get(name);
    const inFlight = this._inFlight.get(name);
    if (inFlight) return inFlight;
    // Only set the cached entry after createFn returns successfully — a
    // failure inside createFn (e.g. startRecording throws) must not leave
    // a half-constructed SessionInfo visible to iterators like shutdown().
    const p = (async () => {
      const info = await createFn();
      this._sessions.set(name, info);
      return info;
    })();
    this._inFlight.set(name, p);
    try {
      return await p;
    } finally {
      this._inFlight.delete(name);
    }
  }
}

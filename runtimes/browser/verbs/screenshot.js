import path from "node:path";
import { promises as fsPromises } from "node:fs";
import { randomUUID } from "node:crypto";
import { resolveInside } from "../lib/util.js";

export default {
  name: "screenshot",
  primaryKey: "path",
  async execute(page, args) {
    // Always resolve inside $WB_ARTIFACTS_DIR (or cwd when unset). Absolute
    // paths and traversals are rejected — screenshots are controlled by
    // runbook authors whose content we don't want to grant arbitrary-write.
    const requested = args.path ?? `screenshot-${Date.now()}.png`;
    const artifactsDir = (process.env.WB_ARTIFACTS_DIR || "").trim() || ".";
    if (path.isAbsolute(requested)) {
      throw new Error(
        `screenshot: absolute paths are not allowed (got ${requested})`,
      );
    }
    const full = resolveInside(artifactsDir, requested);
    if (!full) {
      throw new Error(
        `screenshot: path escapes artifacts dir (got ${requested})`,
      );
    }
    await fsPromises.mkdir(path.dirname(full), { recursive: true });
    // Atomic write via tmp + rename so a crash mid-capture can't leave a
    // truncated PNG that's already been announced via slice.artifact_saved
    // and uploaded to R2. We capture to a Buffer (with `type` derived from
    // the requested extension) and write it ourselves — passing a `.tmp`
    // path directly to Playwright fails because it infers format from the
    // file extension and rejects unknown ones.
    const ext = path.extname(full).toLowerCase();
    const type = ext === ".jpg" || ext === ".jpeg" ? "jpeg" : "png";
    const tmp = `${full}.${process.pid}.${randomUUID().slice(0, 8)}.tmp`;
    try {
      const buf = await page.screenshot({ type, fullPage: !!args.full_page });
      await fsPromises.writeFile(tmp, buf);
      await fsPromises.rename(tmp, full);
    } catch (e) {
      try {
        await fsPromises.unlink(tmp);
      } catch {}
      throw e;
    }
    return `→ ${requested}`;
  },
};

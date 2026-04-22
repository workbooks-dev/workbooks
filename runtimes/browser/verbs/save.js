import path from "node:path";
import { promises as fsPromises } from "node:fs";
import { randomUUID } from "node:crypto";
import { send } from "../lib/io.js";
import { sanitizeArtifactName, autoArtifactName } from "../lib/util.js";

export default {
  name: "save",
  primaryKey: "name",
  async execute(_page, args, ctx) {
    // Persist a JSON artifact into $WB_ARTIFACTS_DIR so later cells can read
    // it and wb can upload it. Captures the previous verb's output unless
    // the author provides an explicit `value:`.
    const artifactsDir = (process.env.WB_ARTIFACTS_DIR || "").trim();
    if (!artifactsDir) {
      throw new Error(
        "save: $WB_ARTIFACTS_DIR is not set — run this workbook via `wb run` (wb exports the dir for you)",
      );
    }
    const explicitValue = args.value !== undefined;
    const payload = explicitValue ? args.value : ctx?.lastResult;
    if (payload === undefined) {
      throw new Error(
        "save: no value provided and no prior extract/eval result to capture",
      );
    }
    const name =
      typeof args.name === "string" && args.name.trim().length > 0
        ? sanitizeArtifactName(args.name)
        : autoArtifactName(ctx?.blockIndex ?? ctx?.index ?? 0);
    const filename = name.endsWith(".json") ? name : `${name}.json`;
    const full = path.join(artifactsDir, filename);
    await fsPromises.mkdir(artifactsDir, { recursive: true });
    // Atomic write: serialize to .tmp, then rename. Announce the artifact
    // AFTER rename so a partial write can never be seen by wb's uploader.
    const serialized = JSON.stringify(payload, null, 2);
    const tmp = `${full}.${process.pid}.${randomUUID().slice(0, 8)}.tmp`;
    try {
      await fsPromises.writeFile(tmp, serialized, "utf8");
      await fsPromises.rename(tmp, full);
    } catch (e) {
      try {
        await fsPromises.unlink(tmp);
      } catch {}
      throw e;
    }
    send({
      type: "slice.artifact_saved",
      filename,
      path: full,
      bytes: Buffer.byteLength(serialized),
    });
    return `→ ${filename}`;
  },
};

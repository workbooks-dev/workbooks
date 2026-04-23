import path from "node:path";
import { promises as fsPromises } from "node:fs";
import { randomUUID } from "node:crypto";
import { resolveInside } from "../lib/util.js";

// Ergonomic shorthand for attaching a human-readable label to an artifact
// already in (or about-to-be-in) $WB_ARTIFACTS_DIR. Writes a sidecar JSON
// at `<path>.meta.json`; the next Artifacts::sync() pass picks it up and
// attaches the label to the step.artifact_saved callback event. No upload,
// no network — just filesystem metadata.
//
// Usage:
//   - save:
//       path: statement.csv
//   - announce_artifact:
//       path: statement.csv
//       label: "April HSBC statement"
//       description: "Reconciled balance export"
//
// The target artifact need not exist yet — callers may pre-write the
// sidecar in an earlier block and drop the artifact later. The sidecar
// only takes effect once the artifact is visible to sync().
export default {
  name: "announce_artifact",
  primaryKey: "path",
  async execute(_page, args) {
    const rawPath = typeof args.path === "string" ? args.path.trim() : "";
    if (!rawPath) {
      throw new Error("announce_artifact: `path` is required");
    }
    const label = typeof args.label === "string" ? args.label : "";
    if (!label) {
      throw new Error("announce_artifact: `label` is required");
    }
    const description =
      typeof args.description === "string" ? args.description : undefined;

    const artifactsDir = (process.env.WB_ARTIFACTS_DIR || "").trim();
    if (!artifactsDir) {
      throw new Error(
        "announce_artifact: $WB_ARTIFACTS_DIR is not set — run this workbook via `wb run` (wb exports the dir for you)",
      );
    }
    if (path.isAbsolute(rawPath)) {
      throw new Error(
        `announce_artifact: absolute paths are not allowed (got ${rawPath})`,
      );
    }
    const full = resolveInside(artifactsDir, rawPath);
    if (!full) {
      throw new Error(
        `announce_artifact: path escapes artifacts dir (got ${rawPath})`,
      );
    }

    const payload = { label };
    if (description !== undefined) payload.description = description;
    const serialized = JSON.stringify(payload, null, 2);

    const sidecar = `${full}.meta.json`;
    await fsPromises.mkdir(path.dirname(sidecar), { recursive: true });
    const tmp = `${sidecar}.${process.pid}.${randomUUID().slice(0, 8)}.tmp`;
    try {
      await fsPromises.writeFile(tmp, serialized, "utf8");
      await fsPromises.rename(tmp, sidecar);
    } catch (e) {
      try {
        await fsPromises.unlink(tmp);
      } catch {}
      throw e;
    }
    return `→ ${rawPath} (labelled)`;
  },
};

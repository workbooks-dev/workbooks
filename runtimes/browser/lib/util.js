import path from "node:path";
import { randomUUID } from "node:crypto";

// Resolve `candidate` inside `dir`, rejecting traversal and absolute paths.
// Returns null when the resolved path escapes `dir` (or is `dir` itself).
// Used by the screenshot verb and substitution artifact reads — anywhere
// runbook-author-controlled strings could compose with a trusted directory
// into an arbitrary filesystem write.
export function resolveInside(dir, candidate) {
  const resolvedDir = path.resolve(dir);
  const resolved = path.resolve(resolvedDir, candidate);
  const rel = path.relative(resolvedDir, resolved);
  if (rel === "" || rel.startsWith("..") || path.isAbsolute(rel)) return null;
  return resolved;
}

export function sanitizeArtifactName(s) {
  // Keep author-chosen names readable but safe as filenames. Drop anything
  // that could escape the artifacts dir (slashes, NULs, etc.).
  return String(s).replace(/[^A-Za-z0-9_.-]+/g, "_").slice(0, 200);
}

export function autoArtifactName(blockIndex) {
  const rand = randomUUID().replace(/-/g, "").slice(0, 8);
  const n = Number.isFinite(blockIndex) ? blockIndex : 0;
  return `cell-${n}-${rand}`;
}

export function redact(value) {
  if (typeof value !== "string") return "";
  if (value.length <= 4) return "***";
  return `${value.slice(0, 2)}***`;
}

// pause_for_human — generalized operator-handoff pause.
//
// Unlike side-effecting verbs (click, fill, goto) which return a summary
// and let the slice loop continue, pause_for_human returns a sentinel
// `{ __pause: {...} }` shape. The dispatcher in ../bin/wb-browser-runtime.js
// inspects that shape and emits a `slice.paused` frame instead of a
// `verb.complete`, which flips the Rust side into its pause-and-exit-42 path
// (pending descriptor written, checkpoint marked paused, process exits).
//
// On `wb resume`, the slice re-enters at verb_index + 1 (the verb that
// paused is skipped — it has no post-resume work). If the pause carried
// `actions`, the operator's choice is written to
// `$WB_ARTIFACTS_DIR/pause_result.json` by the Rust side before the sidecar
// boots again, so downstream bash/python cells can branch on it via a
// plain file read.

const VALID_RESUME_MODES = ["operator_click", "poll", "timeout"];

export default {
  name: "pause_for_human",
  primaryKey: "message",
  async execute(_page, args, ctx) {
    const resumeOn = args.resume_on || "operator_click";
    if (!VALID_RESUME_MODES.includes(resumeOn)) {
      throw new Error(
        `pause_for_human: resume_on must be one of ${VALID_RESUME_MODES.join("|")}, got "${resumeOn}"`,
      );
    }
    // Default action: a single "Resume" button. Authors who want
    // branching on operator choice provide their own `actions` list.
    const actions =
      Array.isArray(args.actions) && args.actions.length > 0
        ? args.actions
        : [{ label: "Resume", value: null }];

    // Validate action entries early so malformed YAML doesn't reach the
    // run page as a broken button set.
    for (const a of actions) {
      if (!a || typeof a !== "object" || typeof a.label !== "string") {
        throw new Error(
          `pause_for_human: each action must be { label: string, value?: any }; got ${JSON.stringify(a)}`,
        );
      }
    }

    return {
      __pause: {
        reason: "pause_for_human",
        message: args.message || "",
        context_url: args.context_url || null,
        resume_on: resumeOn,
        timeout: args.timeout || null,
        actions,
        // Dispatcher wraps this with verb_index at emit time so `wb resume`
        // knows where to re-enter.
        sidecar_state: {},
      },
    };
  },
};

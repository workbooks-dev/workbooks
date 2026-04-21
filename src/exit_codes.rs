//! Process exit-code vocabulary.
//!
//! Agents driving `wb` programmatically need to distinguish *why* a run
//! failed — "the workbook had a bad block" is a retry loop, "the runtime
//! isn't installed" is a provisioning fix, "sandbox is disabled" is an
//! env-var change, "another process holds the checkpoint lock" is a
//! coordination signal.
//!
//! These constants are stable within a major version. When adding a new
//! category, pick the next unused low-number slot and document it here
//! and in the CLI help text. Avoid 126/127 (shell: command-not-executable
//! / not-found) and 128+N (shell: killed by signal N) so wb's codes don't
//! collide with what a child shell might surface.
//!
//! ## Current table
//! | Code | Constant                  | Meaning |
//! |------|---------------------------|---------|
//! | 0    | `EXIT_SUCCESS`            | Run completed, no failures. |
//! | 1    | `EXIT_BLOCK_FAILED`       | A workbook block exited non-zero (with `--bail`, or run-complete failure count > 0). |
//! | 2    | `EXIT_USAGE`              | Bad CLI args, unreadable workbook file, or no executable blocks. |
//! | 3    | `EXIT_WORKBOOK_INVALID`   | Workbook parsed but shape is wrong for this command (e.g. `resume` on a non-paused checkpoint). |
//! | 5    | `EXIT_SANDBOX_UNAVAILABLE`| `requires:` sandbox declared but Docker is missing or the image build failed. |
//! | 6    | `EXIT_CHECKPOINT_BUSY`    | Another `wb` process holds the session flock on the requested checkpoint id. |
//! | 7    | `EXIT_SIGNAL_TIMEOUT`     | A `wait` pause expired and `on_timeout: abort` (or equivalent) fired. |
//! | 42   | `EXIT_PAUSED`             | Run paused on a `wait` or browser-slice pause — not an error; an external resolver should eventually `wb resume`. |

/// Referenced for documentation symmetry; the process exits 0 implicitly on
/// normal completion, so this constant isn't on any hot path.
#[allow(dead_code)]
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_BLOCK_FAILED: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_WORKBOOK_INVALID: i32 = 3;
pub const EXIT_SANDBOX_UNAVAILABLE: i32 = 5;
pub const EXIT_CHECKPOINT_BUSY: i32 = 6;
pub const EXIT_SIGNAL_TIMEOUT: i32 = 7;
pub const EXIT_PAUSED: i32 = 42;

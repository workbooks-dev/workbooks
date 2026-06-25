//! Unified internal error type for `wb`.
//!
//! Before this, leaf modules returned `Result<_, String>` — a flat string that
//! `main.rs` had to re-classify by hand at every call site to pick an exit code.
//! `WbError` keeps the human-readable message but tags it with a *category*, so
//! the right exit code travels with the error: `From<WbError> for WbExit`
//! (below) maps each category to its documented exit code, and command
//! boundaries can `?`/`.into()` instead of hand-choosing a `WbExit` variant.
//!
//! Categories are intentionally coarse — they exist to drive the exit-code
//! contract (`src/exit_codes.rs`), not to enumerate every failure mode. The
//! message string carries the specifics. Genuinely-opaque pass-through blobs
//! (sidecar state, wait predicates) are *not* errors and stay as they are.
//!
//! ## What stays `Result<_, String>`
//!
//! A handful of `main.rs`-internal helpers keep `Result<_, String>` on purpose,
//! because they sit *at* a command boundary where the caller picks the exit
//! code contextually and a `WbError` category would be the wrong abstraction:
//!
//! - `resolve_selection` — an unknown `--only/--from/--until` id is a CLI usage
//!   error (`EXIT_USAGE`, 2). "Usage" is an argument-parsing concept owned by
//!   `WbExit`, not a leaf-error category, so routing it through `WbError`
//!   (whose categories map to 1/3/5) would change its exit code.
//! - `validate_pause_action_targets` — its `String` is *data* (the offending
//!   step id, interpolated into the caller's message), not a diagnostic string.
//! - `prepare_browser_spec` / `write_artifact_sidecar` / `run_setup` — these
//!   feed their message straight into a `BlockResult.stderr` (`String`) at the
//!   call site, so a `WbError` would only be `.to_string()`'d back immediately.

use crate::exit::WbExit;

/// Internal `Result` alias. Leaf modules return `WbResult<T>`; the message is
/// surfaced to the user and the category selects the process exit code.
pub type WbResult<T> = Result<T, WbError>;

#[derive(Debug, thiserror::Error)]
pub enum WbError {
    /// Filesystem / environment I/O failure (read, write, spawn, missing file
    /// that isn't a workbook-structure problem). Maps to exit 1.
    #[error("{0}")]
    Io(String),

    /// Malformed input we parsed ourselves: durations, TTLs, structured step
    /// outputs, signal payloads. Maps to exit 3 (workbook/usage invalid).
    #[error("{0}")]
    Parse(String),

    /// Secret-provider resolution failure (doppler/yard/command/dotenv/prompt).
    /// Maps to exit 1.
    #[error("{0}")]
    Secret(String),

    /// Workbook structure invalid: missing/cyclic includes, bad frontmatter
    /// referenced at load time. Maps to exit 3 (`EXIT_WORKBOOK_INVALID`).
    #[error("{0}")]
    Workbook(String),

    /// Sandbox/Docker unavailable or image build failed. Maps to exit 5
    /// (`EXIT_SANDBOX_UNAVAILABLE`).
    #[error("{0}")]
    Sandbox(String),

    /// Browser sidecar spawn/handshake/suspend failure. Maps to exit 1.
    #[error("{0}")]
    Sidecar(String),
}

impl WbError {
    /// Borrow the underlying message text. Useful where a caller still wants a
    /// `&str` (e.g. a String-returning function mid-migration).
    pub fn message(&self) -> &str {
        match self {
            WbError::Io(m)
            | WbError::Parse(m)
            | WbError::Secret(m)
            | WbError::Workbook(m)
            | WbError::Sandbox(m)
            | WbError::Sidecar(m) => m,
        }
    }
}

/// Category → exit code. This is the whole point of the type: the exit-code
/// decision lives here, once, instead of being re-derived at every call site.
impl From<WbError> for WbExit {
    fn from(e: WbError) -> WbExit {
        match e {
            WbError::Io(m) | WbError::Secret(m) | WbError::Sidecar(m) => WbExit::Io(m),
            WbError::Parse(m) | WbError::Workbook(m) => WbExit::WorkbookInvalid(m),
            WbError::Sandbox(m) => WbExit::SandboxUnavailable(m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit_codes;

    #[test]
    fn io_secret_sidecar_map_to_exit_1() {
        for e in [
            WbError::Io("x".into()),
            WbError::Secret("x".into()),
            WbError::Sidecar("x".into()),
        ] {
            assert_eq!(WbExit::from(e).code(), 1);
        }
    }

    #[test]
    fn parse_and_workbook_map_to_workbook_invalid() {
        assert_eq!(
            WbExit::from(WbError::Parse("x".into())).code(),
            exit_codes::EXIT_WORKBOOK_INVALID
        );
        assert_eq!(
            WbExit::from(WbError::Workbook("x".into())).code(),
            exit_codes::EXIT_WORKBOOK_INVALID
        );
    }

    #[test]
    fn sandbox_maps_to_sandbox_unavailable() {
        assert_eq!(
            WbExit::from(WbError::Sandbox("x".into())).code(),
            exit_codes::EXIT_SANDBOX_UNAVAILABLE
        );
    }

    #[test]
    fn message_round_trips_into_wbexit() {
        let e = WbError::Workbook("missing include: foo.md".into());
        assert_eq!(e.message(), "missing include: foo.md");
        assert_eq!(WbExit::from(e).message(), Some("missing include: foo.md"));
    }
}

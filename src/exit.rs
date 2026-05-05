use crate::exit_codes;

/// Typed result of a `wb` command. `main()` reads the code and optional
/// message, prints the message if set, then calls `std::process::exit`.
///
/// ## Exceptions — paths that still call `process::exit` directly
///
/// Three paths are intentionally left as `process::exit` rather than being
/// threaded through `WbExit`:
///
/// 1. **Sandbox re-entry** (src/main.rs, the docker re-exec path): forwards
///    the child container's exit status verbatim — the exit code is not a
///    wb-decided code, it's whatever the inner `wb run` returned.
///
/// 2. **`pause_for_signal`** (typed `-> !`): must `drop(session)` before
///    exiting so the browser sidecar gets a graceful shutdown. Threading
///    `WbExit::Paused` up through the entire run loop would require making
///    `run_single` fully `Result`-returning — deferred to a later wave.
///
/// 3. **`pause_browser_slice`** (typed `-> !`): same reason as above.
// Variants are part of the public exit-code contract. Most are not yet
// constructed in this wave; they'll be used as process::exit is migrated.
#[allow(dead_code)]
#[derive(Debug)]
pub enum WbExit {
    Success,
    BlockFailed,
    Usage(String),
    WorkbookInvalid(String),
    SandboxUnavailable(String),
    CheckpointBusy(String),
    SignalTimeout(String),
    Paused,
    /// Generic I/O or environment failure. Exits 1.
    Io(String),
}

impl WbExit {
    pub fn code(&self) -> i32 {
        match self {
            WbExit::Success => exit_codes::EXIT_SUCCESS,
            WbExit::BlockFailed => exit_codes::EXIT_BLOCK_FAILED,
            WbExit::Usage(_) => exit_codes::EXIT_USAGE,
            WbExit::WorkbookInvalid(_) => exit_codes::EXIT_WORKBOOK_INVALID,
            WbExit::SandboxUnavailable(_) => exit_codes::EXIT_SANDBOX_UNAVAILABLE,
            WbExit::CheckpointBusy(_) => exit_codes::EXIT_CHECKPOINT_BUSY,
            WbExit::SignalTimeout(_) => exit_codes::EXIT_SIGNAL_TIMEOUT,
            WbExit::Paused => exit_codes::EXIT_PAUSED,
            WbExit::Io(_) => 1,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            WbExit::Success | WbExit::Paused | WbExit::BlockFailed => None,
            WbExit::Usage(s)
            | WbExit::WorkbookInvalid(s)
            | WbExit::SandboxUnavailable(s)
            | WbExit::CheckpointBusy(s)
            | WbExit::SignalTimeout(s)
            | WbExit::Io(s) => {
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wb_exit_codes_match_documented() {
        assert_eq!(WbExit::Success.code(), exit_codes::EXIT_SUCCESS);
        assert_eq!(WbExit::BlockFailed.code(), exit_codes::EXIT_BLOCK_FAILED);
        assert_eq!(WbExit::Usage("x".into()).code(), exit_codes::EXIT_USAGE);
        assert_eq!(
            WbExit::WorkbookInvalid("x".into()).code(),
            exit_codes::EXIT_WORKBOOK_INVALID
        );
        assert_eq!(
            WbExit::SandboxUnavailable("x".into()).code(),
            exit_codes::EXIT_SANDBOX_UNAVAILABLE
        );
        assert_eq!(
            WbExit::CheckpointBusy("x".into()).code(),
            exit_codes::EXIT_CHECKPOINT_BUSY
        );
        assert_eq!(
            WbExit::SignalTimeout("x".into()).code(),
            exit_codes::EXIT_SIGNAL_TIMEOUT
        );
        assert_eq!(WbExit::Paused.code(), exit_codes::EXIT_PAUSED);
        assert_eq!(WbExit::Io("x".into()).code(), 1);
    }
}

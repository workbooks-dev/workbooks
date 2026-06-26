// Structured logging — a tiny, dependency-free severity layer over stderr.
//
// `wb` deliberately avoids a logging framework (zero runtime deps). This module
// is a process-global level plus a set of gate macros: `log_error!` / `log_warn!`
// / `log_info!` / `log_debug!`. Each forwards to `eprintln!` *only if* the
// message's severity is at or below the current level, so the message text is
// unchanged from the old bare `eprintln!` — these macros only add suppression.
//
// The level is set once in `main()` from `--log-level` / `$WB_LOG_LEVEL`
// (default `info`). Lowering it to `error` silences the noisy
// checkpoint/outputs/upload warnings that agents and CI don't want on stderr;
// `debug` is reserved for future traces.
//
// Levels are ordered most-severe → least: error(0) < warn(1) < info(2) < debug(3).
// `enabled(l)` is true when `l <= level`, i.e. a higher level shows more.

use std::sync::atomic::{AtomicU8, Ordering};

pub const LEVEL_ERROR: u8 = 0;
pub const LEVEL_WARN: u8 = 1;
pub const LEVEL_INFO: u8 = 2;
pub const LEVEL_DEBUG: u8 = 3;

static LEVEL: AtomicU8 = AtomicU8::new(LEVEL_INFO);

/// Set the global log level. Called once from `main()`.
pub fn set_level(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

/// Current global log level.
pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

/// True when a message of severity `l` should be emitted at the current level.
pub fn enabled(l: u8) -> bool {
    l <= level()
}

/// Parse a level name. Accepts `error|warn|warning|info|debug` (case-insensitive).
pub fn parse_level(s: &str) -> Result<u8, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "error" => Ok(LEVEL_ERROR),
        "warn" | "warning" => Ok(LEVEL_WARN),
        "info" => Ok(LEVEL_INFO),
        "debug" => Ok(LEVEL_DEBUG),
        other => Err(format!(
            "invalid log level '{other}' (expected error|warn|info|debug)"
        )),
    }
}

/// Emit at error severity (always shown — level can't go below error).
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        if $crate::logging::enabled($crate::logging::LEVEL_ERROR) {
            eprintln!($($arg)*);
        }
    };
}

/// Emit at warning severity (suppressed by `--log-level error`).
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::logging::enabled($crate::logging::LEVEL_WARN) {
            eprintln!($($arg)*);
        }
    };
}

/// Emit at info severity (suppressed by `--log-level warn` or lower).
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::logging::enabled($crate::logging::LEVEL_INFO) {
            eprintln!($($arg)*);
        }
    };
}

/// Emit at debug severity (only shown with `--log-level debug`).
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        if $crate::logging::enabled($crate::logging::LEVEL_DEBUG) {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_level_accepts_known_names() {
        assert_eq!(parse_level("error").unwrap(), LEVEL_ERROR);
        assert_eq!(parse_level("WARN").unwrap(), LEVEL_WARN);
        assert_eq!(parse_level("warning").unwrap(), LEVEL_WARN);
        assert_eq!(parse_level("info").unwrap(), LEVEL_INFO);
        assert_eq!(parse_level("Debug").unwrap(), LEVEL_DEBUG);
        assert!(parse_level("loud").is_err());
    }

    #[test]
    fn enabled_respects_ordering() {
        set_level(LEVEL_WARN);
        assert!(enabled(LEVEL_ERROR));
        assert!(enabled(LEVEL_WARN));
        assert!(!enabled(LEVEL_INFO));
        assert!(!enabled(LEVEL_DEBUG));
        // Restore default so other tests in the binary aren't affected.
        set_level(LEVEL_INFO);
    }
}

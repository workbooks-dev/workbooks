//! Inline assertions (#16/#31) — the `expect` / `assert` fence DSL.
//!
//! An `expect` (or `assert`) fence holds one assertion per line, evaluated at
//! run time against the result of the immediately preceding executable block.
//! In a workbook it is written as an `expect`-tagged fence; the body holds the
//! assertions:
//!
//! ```text
//! exit 0
//! stdout contains "deployed"
//! stderr empty
//! ```
//!
//! Grammar (one per line; `#` comments and blank lines ignored):
//!
//! - `exit <N>` / `exit-code <N>`       — exit code equals N
//! - `exit != <N>`                      — exit code does not equal N
//! - `stdout contains <text>`           — substring present
//! - `stdout not-contains <text>`       — substring absent
//! - `stdout equals <text>`             — exact match (trimmed)
//! - `stdout empty` / `stdout not-empty`
//! - `stderr …`                         — same operators against stderr
//!
//! `<text>` may be quoted (`"…"` or `'…'`) to include spaces. The DSL is
//! intentionally tiny and dependency-free: no regex, no shell. `wb validate`
//! reports malformed lines as `wb-expect-001`; at run time a malformed fence
//! fails the assertion rather than silently passing.

/// Which stream an assertion targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    fn label(self) -> &'static str {
        match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        }
    }
}

/// A single parsed assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    /// `exit <N>` (negate=false) or `exit != <N>` (negate=true).
    Exit {
        negate: bool,
        code: i32,
    },
    Contains {
        stream: Stream,
        needle: String,
    },
    NotContains {
        stream: Stream,
        needle: String,
    },
    Equals {
        stream: Stream,
        value: String,
    },
    Empty {
        stream: Stream,
    },
    NotEmpty {
        stream: Stream,
    },
}

/// Outcome of evaluating one assertion against a block result.
#[derive(Debug, Clone)]
pub struct AssertOutcome {
    pub ok: bool,
    /// The original assertion source line (for reporting).
    pub source: String,
    /// Human-readable detail, populated on failure.
    pub detail: String,
}

/// Parsed contents of an `expect` fence: the assertions plus any malformed
/// lines (each with a reason) so callers can surface `wb-expect-001`.
#[derive(Debug, Clone, Default)]
pub struct ParsedExpect {
    pub assertions: Vec<(String, Assertion)>,
    pub errors: Vec<String>,
}

/// Strip a single layer of matching quotes; otherwise return the input.
fn unquote(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn parse_stream(tok: &str) -> Option<Stream> {
    match tok {
        "stdout" => Some(Stream::Stdout),
        "stderr" => Some(Stream::Stderr),
        _ => None,
    }
}

/// Parse a single assertion line. Returns `Err(reason)` for malformed lines.
fn parse_line(line: &str) -> Result<Assertion, String> {
    let trimmed = line.trim();
    // Optional leading `expect`/`assert` keyword so both `exit 0` and
    // `expect exit 0` parse identically.
    let body = trimmed
        .strip_prefix("expect ")
        .or_else(|| trimmed.strip_prefix("assert "))
        .unwrap_or(trimmed)
        .trim();

    let mut parts = body.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();

    match head {
        "exit" | "exit-code" => {
            let (negate, num_str) = if let Some(n) = rest.strip_prefix("!=") {
                (true, n.trim())
            } else if let Some(n) = rest.strip_prefix("==") {
                (false, n.trim())
            } else {
                (false, rest)
            };
            let code = num_str
                .parse::<i32>()
                .map_err(|_| format!("expected an integer exit code, got '{num_str}'"))?;
            Ok(Assertion::Exit { negate, code })
        }
        "stdout" | "stderr" => {
            let stream = parse_stream(head).unwrap();
            let mut op_parts = rest.splitn(2, char::is_whitespace);
            let op = op_parts.next().unwrap_or("").trim();
            let arg = op_parts.next().unwrap_or("").trim();
            match op {
                "empty" => Ok(Assertion::Empty { stream }),
                "not-empty" | "notempty" => Ok(Assertion::NotEmpty { stream }),
                "contains" => {
                    if arg.is_empty() {
                        return Err(format!("{} contains needs a value", stream.label()));
                    }
                    Ok(Assertion::Contains {
                        stream,
                        needle: unquote(arg),
                    })
                }
                "not-contains" | "notcontains" => {
                    if arg.is_empty() {
                        return Err(format!("{} not-contains needs a value", stream.label()));
                    }
                    Ok(Assertion::NotContains {
                        stream,
                        needle: unquote(arg),
                    })
                }
                "equals" | "==" => {
                    if arg.is_empty() {
                        return Err(format!("{} equals needs a value", stream.label()));
                    }
                    Ok(Assertion::Equals {
                        stream,
                        value: unquote(arg),
                    })
                }
                "" => Err(format!(
                    "{} needs an operator (contains/equals/empty/…)",
                    stream.label()
                )),
                other => Err(format!("unknown {} operator '{other}'", stream.label())),
            }
        }
        "" => Err("empty assertion".to_string()),
        other => Err(format!(
            "unknown assertion '{other}' (expected exit/stdout/stderr)"
        )),
    }
}

/// Parse the body of an `expect` fence into assertions + errors.
pub fn parse(body: &str) -> ParsedExpect {
    let mut out = ParsedExpect::default();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line) {
            Ok(a) => out.assertions.push((line.to_string(), a)),
            Err(reason) => out.errors.push(format!("'{line}': {reason}")),
        }
    }
    out
}

/// Evaluate one assertion against a block's outputs.
pub fn evaluate(
    source: &str,
    assertion: &Assertion,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> AssertOutcome {
    let pick = |s: Stream| match s {
        Stream::Stdout => stdout,
        Stream::Stderr => stderr,
    };
    let (ok, detail) = match assertion {
        Assertion::Exit { negate, code } => {
            let eq = exit_code == *code;
            let ok = if *negate { !eq } else { eq };
            (
                ok,
                if *negate {
                    format!("exit code is {exit_code}, expected != {code}")
                } else {
                    format!("exit code is {exit_code}, expected {code}")
                },
            )
        }
        Assertion::Contains { stream, needle } => {
            let hay = pick(*stream);
            (
                hay.contains(needle.as_str()),
                format!("{} does not contain '{needle}'", stream.label()),
            )
        }
        Assertion::NotContains { stream, needle } => {
            let hay = pick(*stream);
            (
                !hay.contains(needle.as_str()),
                format!("{} unexpectedly contains '{needle}'", stream.label()),
            )
        }
        Assertion::Equals { stream, value } => {
            let hay = pick(*stream);
            (
                hay.trim() == value.as_str(),
                format!("{} does not equal '{value}'", stream.label()),
            )
        }
        Assertion::Empty { stream } => {
            let hay = pick(*stream);
            (
                hay.trim().is_empty(),
                format!("{} is not empty", stream.label()),
            )
        }
        Assertion::NotEmpty { stream } => {
            let hay = pick(*stream);
            (
                !hay.trim().is_empty(),
                format!("{} is empty", stream.label()),
            )
        }
    };
    AssertOutcome {
        ok,
        source: source.to_string(),
        detail: if ok { String::new() } else { detail },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exit() {
        assert_eq!(
            parse_line("exit 0").unwrap(),
            Assertion::Exit {
                negate: false,
                code: 0
            }
        );
        assert_eq!(
            parse_line("exit != 1").unwrap(),
            Assertion::Exit {
                negate: true,
                code: 1
            }
        );
    }

    #[test]
    fn parses_stream_ops() {
        assert_eq!(
            parse_line("stdout contains \"ok\"").unwrap(),
            Assertion::Contains {
                stream: Stream::Stdout,
                needle: "ok".to_string()
            }
        );
        assert_eq!(
            parse_line("stderr empty").unwrap(),
            Assertion::Empty {
                stream: Stream::Stderr
            }
        );
    }

    #[test]
    fn leading_keyword_optional() {
        assert!(parse_line("expect exit 0").is_ok());
        assert!(parse_line("assert stdout not-empty").is_ok());
    }

    #[test]
    fn malformed_lines_reported() {
        let p = parse("exit zero\nstdout\nbogus thing\n# comment\nexit 0\n");
        assert_eq!(p.assertions.len(), 1);
        assert_eq!(p.errors.len(), 3);
    }

    #[test]
    fn evaluate_exit() {
        let a = Assertion::Exit {
            negate: false,
            code: 0,
        };
        assert!(evaluate("exit 0", &a, 0, "", "").ok);
        assert!(!evaluate("exit 0", &a, 1, "", "").ok);
    }

    #[test]
    fn evaluate_contains_and_empty() {
        let c = Assertion::Contains {
            stream: Stream::Stdout,
            needle: "hi".to_string(),
        };
        assert!(evaluate("", &c, 0, "say hi there", "").ok);
        assert!(!evaluate("", &c, 0, "nope", "").ok);

        let e = Assertion::Empty {
            stream: Stream::Stderr,
        };
        assert!(evaluate("", &e, 0, "out", "   ").ok);
        assert!(!evaluate("", &e, 0, "", "boom").ok);
    }

    #[test]
    fn evaluate_equals_trims() {
        let eq = Assertion::Equals {
            stream: Stream::Stdout,
            value: "done".to_string(),
        };
        assert!(evaluate("", &eq, 0, "done\n", "").ok);
        assert!(!evaluate("", &eq, 0, "done now", "").ok);
    }
}

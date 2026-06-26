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

    #[test]
    fn parses_exit_aliases_and_eq_prefix() {
        assert_eq!(
            parse_line("exit-code 2").unwrap(),
            Assertion::Exit {
                negate: false,
                code: 2
            }
        );
        // `==` prefix on exit is accepted (negate=false).
        assert_eq!(
            parse_line("exit == 0").unwrap(),
            Assertion::Exit {
                negate: false,
                code: 0
            }
        );
    }

    #[test]
    fn parses_all_stream_operators_and_aliases() {
        assert_eq!(
            parse_line("stdout not-empty").unwrap(),
            Assertion::NotEmpty {
                stream: Stream::Stdout
            }
        );
        assert_eq!(
            parse_line("stderr notempty").unwrap(),
            Assertion::NotEmpty {
                stream: Stream::Stderr
            }
        );
        assert_eq!(
            parse_line("stdout not-contains 'x'").unwrap(),
            Assertion::NotContains {
                stream: Stream::Stdout,
                needle: "x".to_string()
            }
        );
        assert_eq!(
            parse_line("stdout notcontains y").unwrap(),
            Assertion::NotContains {
                stream: Stream::Stdout,
                needle: "y".to_string()
            }
        );
        // `==` is an alias for equals.
        assert_eq!(
            parse_line("stdout == done").unwrap(),
            Assertion::Equals {
                stream: Stream::Stdout,
                value: "done".to_string()
            }
        );
    }

    #[test]
    fn single_quoted_and_spaced_text_unquoted() {
        assert_eq!(
            parse_line("stdout contains 'hello world'").unwrap(),
            Assertion::Contains {
                stream: Stream::Stdout,
                needle: "hello world".to_string()
            }
        );
    }

    #[test]
    fn malformed_operator_argument_errors() {
        assert!(parse_line("stdout contains").is_err());
        assert!(parse_line("stderr not-contains").is_err());
        assert!(parse_line("stdout equals").is_err());
        // bare stream with no operator
        assert!(parse_line("stdout").is_err());
        // unknown operator
        let e = parse_line("stdout wibble x").unwrap_err();
        assert!(e.contains("unknown stdout operator"));
        // empty line / unknown head
        assert!(parse_line("").is_err());
        let e = parse_line("frobnicate").unwrap_err();
        assert!(e.contains("unknown assertion"));
        // non-integer exit code
        assert!(parse_line("exit nope").is_err());
    }

    #[test]
    fn evaluate_exit_negate() {
        let a = Assertion::Exit {
            negate: true,
            code: 1,
        };
        assert!(evaluate("exit != 1", &a, 0, "", "").ok); // 0 != 1 → pass
        let r = evaluate("exit != 1", &a, 1, "", "");
        assert!(!r.ok);
        assert!(r.detail.contains("expected != 1"));
    }

    #[test]
    fn evaluate_not_contains_and_not_empty() {
        let nc = Assertion::NotContains {
            stream: Stream::Stdout,
            needle: "bad".to_string(),
        };
        assert!(evaluate("", &nc, 0, "all good", "").ok);
        let r = evaluate("", &nc, 0, "this is bad", "");
        assert!(!r.ok);
        assert!(r.detail.contains("unexpectedly contains"));

        let ne = Assertion::NotEmpty {
            stream: Stream::Stderr,
        };
        assert!(evaluate("", &ne, 0, "", "boom").ok);
        let r = evaluate("", &ne, 0, "", "   ");
        assert!(!r.ok);
        assert!(r.detail.contains("stderr is empty"));
    }

    #[test]
    fn evaluate_failure_detail_for_each_kind() {
        // Contains fail detail.
        let c = Assertion::Contains {
            stream: Stream::Stdout,
            needle: "z".to_string(),
        };
        let r = evaluate("", &c, 0, "abc", "");
        assert!(!r.ok && r.detail.contains("does not contain"));
        // Equals fail detail.
        let eq = Assertion::Equals {
            stream: Stream::Stderr,
            value: "v".to_string(),
        };
        let r = evaluate("", &eq, 0, "", "other");
        assert!(!r.ok && r.detail.contains("does not equal"));
        // Empty fail detail.
        let em = Assertion::Empty {
            stream: Stream::Stdout,
        };
        let r = evaluate("", &em, 0, "x", "");
        assert!(!r.ok && r.detail.contains("is not empty"));
        // A passing assertion has empty detail.
        assert!(evaluate("", &em, 0, "", "").detail.is_empty());
    }

    #[test]
    fn parse_skips_blank_and_comment_lines() {
        let p = parse("\n   \n# just a comment\nexit 0\n");
        assert_eq!(p.assertions.len(), 1);
        assert!(p.errors.is_empty());
    }

    #[test]
    fn unquote_leaves_unmatched_quotes() {
        // Single short string / mismatched quotes pass through untouched.
        assert_eq!(unquote("\"only-left"), "\"only-left".to_string());
        assert_eq!(unquote("a"), "a".to_string());
    }
}

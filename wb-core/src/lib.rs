//! `wb-core` (#48) — the pure, dependency-light analysis core of `wb`, factored
//! out as an embeddable + WASM-buildable crate. It reuses the *exact* source
//! files of the main `wb` crate via `#[path]` (no duplication, no drift): the
//! parser, step IR, diagnostics, the assertion DSL, and typed parameters. None
//! of these touch process spawning, the filesystem at parse time (include
//! resolution is the one exception and is simply unused in a browser), or the
//! network — so they compile to `wasm32-unknown-unknown` for a client-side
//! preview on workbooks.dev.
//!
//! The heavy runtime (executor, sidecar, checkpoints, callbacks, CLI) stays in
//! the `wb` binary crate and is intentionally absent here.

#[path = "../../src/assertion.rs"]
pub mod assertion;
#[path = "../../src/diagnostic.rs"]
pub mod diagnostic;
#[path = "../../src/error.rs"]
pub mod error;
#[path = "../../src/exit.rs"]
pub mod exit;
#[path = "../../src/exit_codes.rs"]
pub mod exit_codes;
#[path = "../../src/logging.rs"]
pub mod logging;
#[path = "../../src/params.rs"]
pub mod params;
#[path = "../../src/parser.rs"]
pub mod parser;
#[path = "../../src/step_ir.rs"]
pub mod step_ir;

/// Parse a workbook and return a compact JSON summary (title, runtime, and the
/// resolved step list with ids/languages/line numbers). The embeddable entry
/// point for a client-side preview — pure, no I/O, no side effects.
pub fn parse_to_json(markdown: &str) -> String {
    let wb = parser::parse(markdown);
    let steps: Vec<_> = wb
        .build_steps()
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id.as_str(),
                "language": s.language,
                "line": s.span.line,
            })
        })
        .collect();
    serde_json::json!({
        "title": wb.frontmatter.title,
        "runtime": wb.frontmatter.runtime,
        "blocks": wb.code_block_count(),
        "steps": steps,
    })
    .to_string()
}

#[cfg(test)]
mod core_tests {
    use super::*;

    #[test]
    fn parse_to_json_returns_steps() {
        let md = "---\ntitle: Demo\nruntime: bash\n---\n```bash {#hi}\necho hi\n```\n";
        let json: serde_json::Value = serde_json::from_str(&parse_to_json(md)).unwrap();
        assert_eq!(json["title"], "Demo");
        assert_eq!(json["runtime"], "bash");
        assert_eq!(json["blocks"], 1);
        assert_eq!(json["steps"][0]["id"], "hi");
        assert_eq!(json["steps"][0]["language"], "bash");
    }
}

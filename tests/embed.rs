//! Proves the `wb` library crate (the wb-core embeddable surface, #48) exposes
//! the pure parser / step-IR / assertion / params API for embedding (e.g. a
//! client-side WASM preview). The binary is a thin shim over `wb::run`.

#[test]
fn embed_parse_and_build_steps() {
    let workbook =
        wb::parser::parse("---\nruntime: bash\n---\n# Title\n```bash {#hi}\necho hi\n```\n");
    assert_eq!(workbook.code_block_count(), 1);
    let steps = workbook.build_steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].language, "bash");
    assert_eq!(steps[0].id.as_str(), "hi");
}

#[test]
fn embed_assertion_dsl() {
    let parsed = wb::assertion::parse("exit 0\nstdout contains \"ok\"\nbogus line\n");
    assert_eq!(parsed.assertions.len(), 2);
    assert_eq!(parsed.errors.len(), 1);
}

#[test]
fn embed_params_resolution() {
    let specs: std::collections::HashMap<String, wb::params::ParamSpec> =
        serde_yaml::from_str("region: us-east-1\n").unwrap();
    let resolved = wb::params::resolve(Some(&specs), None, None, None, &[]).unwrap();
    assert_eq!(
        resolved.values.get("region").map(String::as_str),
        Some("us-east-1")
    );
}

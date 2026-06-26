//! Thin binary shim. All logic lives in the `wb` library crate (`src/lib.rs`)
//! so the parser / step-IR / diagnostic core can also be embedded (e.g. a
//! client-side WASM preview build). See `wb::run`.

fn main() -> std::process::ExitCode {
    wb::run()
}

//! The HTTP driving adapter, and a composition root.
//!
//! Dormant. Extraction is invoked from a terminal, so the first capability
//! lives in `cli`; this crate keeps its ring for the HTTP surface that
//! follows. When it arrives, this is the only place besides `cli` that names
//! a concrete adapter.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("web: no HTTP surface yet — use `fitness` for extraction");
    // `clippy::exit` is `forbid`, so returning `ExitCode` is not a style
    // preference: `std::process::exit` will not compile, and no attribute can
    // grant an exception (E0453).
    ExitCode::FAILURE
}

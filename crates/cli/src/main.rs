//! The operator's entry point, and a composition root.
//!
//! This is the only place besides `web` that names a concrete adapter: it
//! picks the implementations, injects them into the use cases, and translates
//! between the terminal and the driving ports.

use std::process::ExitCode;

fn main() -> ExitCode {
    // `clippy::exit` is `forbid`, so returning `ExitCode` is not a style
    // preference — `std::process::exit` will not compile, and no attribute can
    // grant an exception (E0453).
    ExitCode::SUCCESS
}

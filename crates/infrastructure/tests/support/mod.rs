//! Shared fixtures for the adapter suites.
//!
//! Each file in `tests/` is its own crate, so every one that says `mod support`
//! compiles the whole of this — and warns about the parts it does not use.
//! `dead_code` is allowed here for that reason and no other; it is not a
//! forbidden lint, and the alternative is a fixture split per suite with the
//! corpus loaded four times.
#![allow(dead_code, unused_macros, unused_imports)]

pub mod corpus;

/// Derive over the corpus, or fail the test saying which step broke.
///
/// A macro rather than a function so that the `panic!` expands inside the
/// `#[test]` body. `clippy.toml`'s `allow-panic-in-tests` covers a test
/// function; it does not cover a helper defined alongside one, and `panic` is
/// `forbid` — so a plain `fn derive() -> Produced` is a compile error.
macro_rules! derived {
    () => {
        derived!(false)
    };
    ($reversed:expr) => {
        match $crate::support::corpus::derive($reversed) {
            Ok(Ok(produced)) => produced,
            Ok(Err(error)) => panic!("the corpus derives: {error}"),
            Err(error) => panic!("the corpus fixture loads: {error}"),
        }
    };
    ($fixture:expr, $reversed:expr) => {
        match $crate::support::corpus::block_on($fixture.run($reversed)) {
            Ok(Ok(produced)) => produced,
            Ok(Err(error)) => panic!("the derivation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

pub(crate) use derived;

//! Shared fixtures for the adapter suites.

pub mod corpus;
pub mod layers;
pub mod mesocycle;
pub mod store;

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

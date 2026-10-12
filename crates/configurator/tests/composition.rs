//! What is true of the composition as a whole, rather than of any one value.

use std::path::Path;

/// **One place turns a Peloton login into a `PelotonAuth`** (#366).
///
/// `wiring::peloton_auth` is it, and whether the token file is wired is
/// therefore not a per-call-site decision. It was: `caching_in` was called in
/// `plan` and nowhere else, so every `fitness next` that collected cycling
/// performed two complete Auth0 password logins seconds apart while extraction
/// neither read nor wrote the token file — and the second of the two was
/// refused on 2026-10-05, landing no performance graph for anything ridden.
///
/// **Both frontends are read as well as this crate** (#398): a second driving
/// adapter is a second place an authenticator could be built without the cache.
///
/// A grep, because what is being asserted is about the composition rather than
/// about a value: there is no object to ask. The adapter's own constructor
/// stays public for the contract tests, which point it at a stub and want no
/// cache at all.
#[test]
fn only_the_configurator_constructs_a_peloton_authenticator() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the workspace's crates directory");
    let mut sites = Vec::new();

    for crate_name in ["configurator", "cli", "web"] {
        let source = crates.join(crate_name).join("src");
        for entry in std::fs::read_dir(&source).expect("each crate's sources are readable") {
            let path = entry.expect("each entry reads").path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file reads");
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .to_owned();
            for (number, line) in text.lines().enumerate() {
                if line.contains("PelotonAuth::new(") {
                    sites.push(format!("{crate_name}/{name}:{}", number + 1));
                }
            }
        }
    }

    assert_eq!(
        sites.len(),
        1,
        "every Peloton authenticator comes from wiring::peloton_auth, found {sites:?}"
    );
    assert!(
        sites
            .first()
            .is_some_and(|site| site.starts_with("configurator/wiring.rs")),
        "and that one place is the configurator's wiring, found {sites:?}"
    );
}

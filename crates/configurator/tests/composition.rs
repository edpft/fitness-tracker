//! What is true of the composition as a whole, rather than of any one value.

use std::path::Path;

/// Every line in the configurator and both frontends that calls `constructor`,
/// as `crate/file.rs:line`.
///
/// A grep, because what is being asserted is about the composition rather than
/// about a value: there is no object to ask. The adapters' own constructors
/// stay public for the contract tests, which point them at a stub and want no
/// cache at all.
///
/// **Both frontends are read as well as this crate** (#398): a second driving
/// adapter is a second place an authenticator could be built without the cache.
fn construction_sites(constructor: &str) -> std::io::Result<Vec<String>> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| std::io::Error::other("the crate sits in the crates directory"))?;
    let mut sites = Vec::new();

    for crate_name in ["configurator", "cli", "web"] {
        let source = crates.join(crate_name).join("src");
        for entry in std::fs::read_dir(&source)? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .to_owned();
            for (number, line) in text.lines().enumerate() {
                if line.contains(constructor) {
                    sites.push(format!("{crate_name}/{name}:{}", number + 1));
                }
            }
        }
    }
    Ok(sites)
}

/// **One place turns a Peloton login into a `PelotonAuth`** (#366).
///
/// `wiring::peloton_auth` is it, and whether the token file is wired is
/// therefore not a per-call-site decision. It was: `caching_in` was called in
/// `plan` and nowhere else, so every `fitness next` that collected cycling
/// performed two complete Auth0 password logins seconds apart while extraction
/// neither read nor wrote the token file — and the second of the two was
/// refused on 2026-10-05, landing no performance graph for anything ridden.
#[test]
fn only_the_configurator_constructs_a_peloton_authenticator() {
    let sites = construction_sites("PelotonAuth::new(").expect("the sources are readable");

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

/// **One place turns a Garmin login into a `GarminAuth`** (#377), for
/// Peloton's reason.
///
/// Three did, each with no token cache: `garmin.hrv`, `garmin.activities` and
/// `garmin.daily_heart_rate` each signed in for itself, so a run collecting two
/// of them went through Garmin's SSO widget twice — behind Cloudflare, where a
/// client that keeps signing in is a client that gets blocked. The credentials
/// wizard was a fourth, and threw away the token its check earned.
#[test]
fn only_the_configurator_constructs_a_garmin_authenticator() {
    let sites = construction_sites("GarminAuth::new(").expect("the sources are readable");

    assert_eq!(
        sites.len(),
        1,
        "every Garmin authenticator comes from wiring::garmin_auth, found {sites:?}"
    );
    assert!(
        sites
            .first()
            .is_some_and(|site| site.starts_with("configurator/wiring.rs")),
        "and that one place is the configurator's wiring, found {sites:?}"
    );
}

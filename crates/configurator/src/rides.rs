//! Where this build's rides are done.
//!
//! **A venue is an opaque reference everywhere below here**; that this build's
//! cycling comes from Peloton is a fact about the composition, and turning a
//! reference into a link is the one place it is allowed to show.

/// The page a class is started from.
#[must_use]
pub fn link(reference: &str) -> String {
    infrastructure::peloton::url_for(reference)
}

/// Whether the operator's account is known not to be able to start the class.
#[must_use]
pub fn known_unavailable(reference: &str) -> bool {
    infrastructure::peloton::is_known_unavailable(reference)
}

/// The standalone FTP test programme, as the authored record names it.
pub const TEST_PROGRAMME: &str = infrastructure::peloton::skeleton::POWER_ZONE_TEST;

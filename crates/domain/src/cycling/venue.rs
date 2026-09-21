//! Where a ride is done, named by whoever runs the place.
//!
//! **One vocabulary for both sides of § 11.** A [`RideVenue`] names a place an
//! authored ride is *to be* done and a place a performed ride *was* done, and
//! it is the same type deliberately: prescribed and performed are stored
//! separately and **joinably**, and "have I ridden this before?" is exactly
//! that join. Two types would have made the question a translation.
//!
//! **Nothing here interprets the reference.** It is a Peloton class id today
//! and this module says nothing about that (§ II.3) — it is an opaque handle
//! the destination issued, in the same position
//! [`DeliveryReference`](crate::prescription::DeliveryReference) holds.
//!
//! It lived in [`super::mesocycle`] until 2026-09-20, which was the authored
//! side alone. A venue is not an authoring concept: it is where riding happens.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidVenue {
    #[error("a ride has to be done somewhere, and an empty reference names nowhere")]
    EmptyReference,
    #[error("a destination that names a place calls it something")]
    EmptyName,
}

/// What a destination calls one place a ride is done.
///
/// Validated for emptiness and nothing else, exactly as
/// [`DeliveryReference`](crate::prescription::DeliveryReference) is: the
/// reference belongs to the system that issued it, and imposing a shape on it
/// would be this side inventing a rule the issuer never agreed to.
///
/// **The name is carried beside the reference** because it is what a
/// prescription prints. It identifies nothing — two Peloton classes really do
/// share a title — so nothing reads it but the report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RideVenue {
    reference: String,
    called: String,
}

impl RideVenue {
    /// # Errors
    ///
    /// [`InvalidVenue`] if either half is empty once trimmed.
    pub fn new(reference: &str, called: &str) -> Result<Self, InvalidVenue> {
        let reference = reference.trim().to_owned();
        let called = called.trim().to_owned();
        if reference.is_empty() {
            return Err(InvalidVenue::EmptyReference);
        }
        if called.is_empty() {
            return Err(InvalidVenue::EmptyName);
        }
        Ok(Self { reference, called })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn called(&self) -> &str {
        &self.called
    }
}

impl std::fmt::Display for RideVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.called)
    }
}

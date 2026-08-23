//! What a prescription becomes once it has been put somewhere the operator
//! trains from.
//!
//! **A destination is a renderer that returns a receipt.** Printing a session
//! to a terminal and putting it in a phone app are the same act — deriving what
//! to do, and rendering it — and neither is part of the domain's reasoning. The
//! one asymmetry is that a terminal forgets and an app does not: it keeps the
//! session as an object with an identity of its own, and that identity is the
//! only residue worth recording. Everything else about how it got there belongs
//! to the adapter that put it there.
//!
//! **The reference is opaque here on purpose.** § 8 makes our entity identity
//! ours rather than a source's, and a destination's identifier is a foreign key
//! into a system we do not own — so this type never interprets it, compares it
//! to anything but another of its own kind, or knows what shape it has. The
//! precedent is the resumption token on the extraction side, which the
//! application carries and only the adapter reads.
//!
//! Which destination a reference belongs to is not recorded on the reference.
//! It is a fact about the delivery, and putting it here would make the type
//! carry an answer that only the adapter that minted it can give.

use std::fmt;

use crate::newtype::string_name;

/// Why a delivery could not be recorded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidDelivery {
    #[error("a delivery reference must not be empty")]
    EmptyReference,
    #[error("a destination name must not be empty")]
    EmptyDestination,
    #[error("a destination name must not contain whitespace")]
    DestinationContainsWhitespace,
    #[error("a destination name must be lowercase")]
    DestinationNotLowercase,
    #[error("a session ordinal counts from one, and {value} does not")]
    OrdinalBelowOne { value: u32 },
}

/// What a destination called the session it was given.
///
/// Validated only for emptiness, exactly as [`crate::landing::SourceRecordId`]
/// is: the value belongs to the system that issued it, and imposing a shape on
/// it would be this side inventing a rule the issuer never agreed to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeliveryReference(String);

impl TryFrom<String> for DeliveryReference {
    type Error = InvalidDelivery;

    fn try_from(reference: String) -> Result<Self, Self::Error> {
        if reference.is_empty() {
            return Err(InvalidDelivery::EmptyReference);
        }
        Ok(Self(reference))
    }
}

string_name!(DeliveryReference, InvalidDelivery);

/// Which session of its programme this is, counting every programmed session
/// from the first.
///
/// **A property of the calendar, not of any destination.** What a renderer does
/// with it — pads it, prefixes a title with it, ignores it — is the renderer's
/// business; that there is a first, second and third session of a block is the
/// programme's. Counting sessions rather than weeks is what makes it a total
/// order over a folder: two sessions in one week are two numbers, and an
/// interrupted week contributes none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionOrdinal(u32);

impl SessionOrdinal {
    /// # Errors
    ///
    /// [`InvalidDelivery::OrdinalBelowOne`] for a zero, which would name the
    /// session before the block began.
    pub const fn new(value: u32) -> Result<Self, InvalidDelivery> {
        if value < 1 {
            return Err(InvalidDelivery::OrdinalBelowOne { value });
        }
        Ok(Self(value))
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SessionOrdinal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which destination a session was delivered to.
///
/// **Ours, not the destination's.** § 8 puts entity identity on this side, and
/// the name of a system we send to is no different: it keys the record of what
/// has already been delivered, so two spellings of one destination would deliver
/// a session twice and leave the operator two routines to choose between. The
/// rules are [`crate::landing::LandingStream`]'s, less the separator — a name
/// that reaches a command line and a stored key has to round-trip through both.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestinationName(String);

impl TryFrom<String> for DestinationName {
    type Error = InvalidDelivery;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            return Err(InvalidDelivery::EmptyDestination);
        }
        if name.chars().any(char::is_whitespace) {
            return Err(InvalidDelivery::DestinationContainsWhitespace);
        }
        if name.chars().any(char::is_uppercase) {
            return Err(InvalidDelivery::DestinationNotLowercase);
        }
        Ok(Self(name))
    }
}

string_name!(DestinationName, InvalidDelivery);

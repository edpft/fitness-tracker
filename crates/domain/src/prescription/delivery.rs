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

use sha2::{Digest, Sha256};

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
    #[error("a destination that answered said how it took the request")]
    EmptyStatus,
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

/// Which session of its macrocycle this is, counting every session the
/// macrocycle has prescribed for from the first.
///
/// **A count over the record of prescriptions, not over a calendar** (#312).
/// Until 2026-09-30 it was the session's position in its *mesocycle's*
/// calendar, and both halves of that were wrong. The folder it orders is the
/// macrocycle's, so the number restarted inside it; and a calendar is rebuilt
/// from a start, a duration and its interruptions, so re-authoring any of them
/// renumbers sessions already sitting on the operator's phone. The autumn's
/// first two deliveries were both `01` for exactly that reason — same
/// mesocycle, moved calendar.
///
/// So what it counts is days the macrocycle issued a prescription for. The
/// operator, 2026-09-30: *"it doesn't matter if a session was missed because of
/// illness, all that matters is was it prescribed ... so it's the prescription
/// number, relative to the macrocycle, at the time of prescription."* A day
/// nobody prescribed for takes no number; a day prescribed and then missed
/// keeps its own; a reissue for a day already numbered keeps that day's number,
/// because what it replaces is that day's routine.
///
/// **A property of the macrocycle, not of any destination.** What a renderer
/// does with it — pads it, prefixes a title with it, ignores it — is the
/// renderer's business.
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

/// A stored rendering digest that is not 32 bytes wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a rendering digest is 32 bytes, found {width}")]
pub struct WrongRenderingWidth {
    pub width: usize,
}

/// A SHA-256 over what a destination made of a session.
///
/// **The fingerprint of a routine's contents, so a stale one can be told from a
/// current one.** A [`DeliveryReference`] says where a session is; this says
/// what is there. Without it a delivery can only ask "has this prescription
/// been delivered?", and answers yes to a routine rendered by a build whose
/// rendering has since been corrected — which is a broken session on the
/// operator's phone and nothing that will replace it (#343).
///
/// **Opaque, and compared only for equality.** Nothing orders two of these,
/// reads a byte of one or infers what changed from the difference: the only
/// question it answers is whether the destination is holding what this build
/// renders. What goes into it is the destination's to decide, and the digest is
/// meaningless across destinations.
///
/// Deliberately not `Hash`, for the reason [`crate::landing::RawPayload`]'s
/// digest is not: this is written to the store and compared against rows
/// written by earlier versions of this program, and `Hash` guarantees neither
/// the algorithm nor stability across builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderingDigest([u8; 32]);

impl RenderingDigest {
    /// Digest the bytes a destination would send.
    pub fn of(rendered: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(rendered);
        Self(hasher.finalize().into())
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Rehydrate a digest an adapter previously persisted, exactly as
/// [`crate::landing::PayloadDigest`] is rehydrated: the bytes that produced it
/// are not read back merely to re-derive it.
impl From<[u8; 32]> for RenderingDigest {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A row holding anything other than 32 bytes means the file holds something
/// this program did not write.
impl TryFrom<&[u8]> for RenderingDigest {
    type Error = WrongRenderingWidth;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        <[u8; 32]>::try_from(bytes)
            .map(Self)
            .map_err(|_| WrongRenderingWidth { width: bytes.len() })
    }
}

impl AsRef<[u8]> for RenderingDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Where a prescription has got to.
///
/// **Three states, and which one it is decides what may be done to it.** The
/// operator settled this on 2026-08-25, and it is the answer to the question
/// § 12 left open: authored data keeps its history because "nothing regenerates
/// it if lost", and that premise is false for a prescription nobody has
/// performed.
///
/// **Derived, never stored.** Drafted is a prescription with no delivery;
/// published is one with a delivery nothing names; performed is one a workout
/// names. A status column would be a second source of truth for a fact the
/// relations already carry, and the two could disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrescriptionState {
    /// Issued and nowhere else. Nothing outside the store knows it exists, and
    /// it re-derives exactly from its programme, the record and the parameters
    /// — so deleting it loses nothing.
    Drafted,
    /// Delivered somewhere it can be performed, and fixed by the reference that
    /// destination gave it. Still cheap: withdrawing it means removing the
    /// session at the destination as well, rather than merely forgetting it
    /// here, or the clutter is what is left behind.
    Published { reference: DeliveryReference },
    /// A workout names its reference. Now it is not cheap: what it records
    /// happened, and the performance beside it would be left comparing against
    /// nothing.
    Performed { reference: DeliveryReference },
}

impl PrescriptionState {
    /// May this be thrown away?
    ///
    /// The whole of what the three states are for.
    pub const fn is_disposable(&self) -> bool {
        match self {
            Self::Drafted | Self::Published { .. } => true,
            Self::Performed { .. } => false,
        }
    }

    /// The reference the destination gave it, once it has one.
    pub const fn reference(&self) -> Option<&DeliveryReference> {
        match self {
            Self::Drafted => None,
            Self::Published { reference } | Self::Performed { reference } => Some(reference),
        }
    }
}

impl fmt::Display for PrescriptionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Drafted => f.write_str("drafted"),
            Self::Published { .. } => f.write_str("published"),
            Self::Performed { .. } => f.write_str("performed"),
        }
    }
}

/// How a destination said it took what it was given, in its own vocabulary.
///
/// **Opaque, exactly as [`DeliveryReference`] is.** `201 Created` and
/// `400 Bad Request` are Hevy's words for what it did; this side keeps them and
/// does not parse them. What it does record is the one bit that is not the
/// destination's to define: whether the act succeeded, which the adapter knows
/// and nothing downstream could recover from the text without learning the
/// destination's vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyStatus {
    stated: String,
    succeeded: bool,
}

impl ReplyStatus {
    /// # Errors
    ///
    /// [`InvalidDelivery::EmptyStatus`] if the destination said nothing about
    /// how it took the request. A destination that answered at all said
    /// something.
    pub fn new(stated: impl Into<String>, succeeded: bool) -> Result<Self, InvalidDelivery> {
        let stated = stated.into();
        if stated.is_empty() {
            return Err(InvalidDelivery::EmptyStatus);
        }
        Ok(Self { stated, succeeded })
    }

    pub fn as_str(&self) -> &str {
        &self.stated
    }

    pub const fn succeeded(&self) -> bool {
        self.succeeded
    }
}

impl fmt::Display for ReplyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.stated)
    }
}

/// What a destination answered, before anything interpreted it.
///
/// **The delivery side's [`crate::landing::RawPayload`]**, and it exists for the
/// reason that one does: bytes read once cannot be asked for again. We kept
/// every byte a source *served* and nothing a destination *answered*, so a reply
/// that would not parse left only serde's complaint about a column number, and a
/// reply that parsed left nothing at all — which is how a routine created
/// without its front squat became a question only the live API could answer
/// (#124).
///
/// Two things differ from a landing payload, and both follow from what a reply
/// is rather than from convenience.
///
/// **An empty body is legal here.** A source that served nothing served no
/// observation; a destination that refuses with a bare status has still
/// answered, and that it said nothing beyond the status is the fact worth
/// keeping.
///
/// **It is not observation data.** § II does not reach it: nothing here is a
/// record of what happened, and this acquires no normalised or canonical form.
/// It belongs to the delivery record — § 12 authored data, extended by what the
/// destination said back — and, like the reference beside it, nothing
/// regenerates it if lost.
#[derive(Clone, PartialEq, Eq)]
pub struct DestinationReply {
    status: ReplyStatus,
    body: Vec<u8>,
}

impl DestinationReply {
    /// Taken before anything interprets it, which is the whole point: an
    /// adapter that parses first has already decided what is worth keeping.
    pub const fn new(status: ReplyStatus, body: Vec<u8>) -> Self {
        Self { status, body }
    }

    pub const fn status(&self) -> &ReplyStatus {
        &self.status
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The body as something printable, for the error the operator reads.
    ///
    /// Lossy rather than fallible: a reply that is not valid UTF-8 is exactly
    /// the sort of surprise this exists to surface, and refusing to show it
    /// would reproduce the failure it was written to end. The bytes are kept
    /// intact whatever this renders.
    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

/// Prints the body's length rather than the body, for
/// [`crate::landing::RawPayload`]'s reason: a reply is a whole routine, and
/// dumping it into every log line helps nobody. The one place it is worth
/// reading in full asks for it with [`DestinationReply::text`].
impl fmt::Debug for DestinationReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DestinationReply({}, {} bytes)",
            self.status,
            self.body.len()
        )
    }
}

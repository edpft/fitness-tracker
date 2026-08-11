//! The bytes a source served, and the digest that decides whether they have
//! changed.

use std::fmt;

use sha2::{Digest, Sha256};

/// Why a payload could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidPayload {
    #[error("a payload must not be empty")]
    Empty,
}

/// One record's payload, exactly as the source served it.
///
/// Nothing here parses, validates, renames, defaults or interprets a field.
/// The bytes are the source's; our only claim about them is that there are
/// some.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RawPayload(Vec<u8>);

impl RawPayload {
    /// # Errors
    ///
    /// Returns [`InvalidPayload::Empty`] if there are no bytes. A source that
    /// served nothing served no observation.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, InvalidPayload> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(InvalidPayload::Empty);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn digest(&self) -> PayloadDigest {
        let mut hasher = Sha256::new();
        hasher.update(&self.0);
        PayloadDigest(hasher.finalize().into())
    }
}

/// Debug prints the length rather than the bytes: a payload is a whole workout
/// and dumping it into a log or a test failure helps nobody.
impl fmt::Debug for RawPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPayload({} bytes)", self.0.len())
    }
}

/// A SHA-256 over a payload's bytes, as received.
///
/// Deliberately not over a canonicalised form. Canonicalising is
/// interpretation, and it guards against a failure mode that is harmless
/// anyway: were the source to change its serialisation, the result would be
/// one extra landing record per record, once — and a later record superseding
/// an earlier one is how supersession already works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PayloadDigest([u8; 32]);

impl PayloadDigest {
    /// Rehydrate a digest an adapter previously persisted.
    ///
    /// The ordinary way to obtain one is [`RawPayload::digest`]. This exists
    /// for the persistence boundary alone, where the payload that produced the
    /// digest was written in the same transaction and is not read back merely
    /// to re-derive it. What the type still guarantees everywhere is width: a
    /// digest cannot be confused with arbitrary data.
    pub fn from_storage(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for PayloadDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

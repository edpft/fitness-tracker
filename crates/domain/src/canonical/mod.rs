//! What the canonical layer needs whatever entity it is deriving.
//!
//! § II.4: one entry per real-world event, whatever number of sources recorded
//! it, built from the normalised layer, deterministic matching and the match
//! overlay. The entities themselves belong to their discipline, as they do at
//! the layer below — a [`crate::gym::CanonicalGymSession`] is declared by
//! [`crate::gym`]. This is the vocabulary they are built with.
//!
//! Sibling of [`crate::normalised`], and deliberately smaller. That module
//! carries a run, its refusals and a declared time zone because a derivation
//! from raw can fail in ways this one cannot: matching reads entities that are
//! already in our terms, so there is nothing here to refuse and no zone to
//! apply.

pub mod session;
pub mod time;

pub use session::{NegativeNormalisedSessionId, NormalisedSessionId, SessionCount};
pub use time::Occurred;

//! Linear periodisation: a top-set ladder climbing to a test, and the programmes
//! written against it.
//!
//! **The right tool for a short or interrupted window**, where there are not
//! enough weeks to give a phase each to accumulating, intensifying and realising.
//! [`block`](super::block) is the other model and neither supersedes the other.
//! This was template `v1` until 2026-08-18; the rename is because the two are
//! models of periodisation rather than versions of one programme.
//!
//! **A template is never edited or removed, only added to.** A programme written
//! against this one keeps generating against it, so changing anything here
//! changes what an already-issued series would regenerate as — and § 7 requires
//! that regeneration to be faithful. A structural change is a new module beside
//! this one.
//!
//! The template is a builder rather than a value: there is no `Template` type
//! anywhere, because selecting a variant is selecting among programme types.

pub mod programme;
pub mod template;

pub use programme::{Linear, Primary};
pub use template::{Fill, Position, PrimaryPattern, STRETCHES, SlotContent, SlotFills, StaticFill};

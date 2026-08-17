//! Template `v1`, and the programmes written against it.
//!
//! **A variant is never edited or removed, only added.** A programme written
//! against `v1` keeps generating against `v1` after a `v2` exists, so changing
//! anything here changes what an already-issued series would regenerate as — and
//! § 7 requires that regeneration to be faithful. A structural change is a new
//! module beside this one.
//!
//! The template is a builder rather than a value: there is no `Template` type
//! anywhere, because selecting a variant is selecting among programme types.

pub mod programme;
pub mod template;

pub use programme::{InconsistentProgramme, Programme};
pub use template::{Fill, PrimaryPattern, SlotContent, SlotFills};

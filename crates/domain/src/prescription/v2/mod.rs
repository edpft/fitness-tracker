//! Block periodisation: the programme for a window long enough to hold one.
//!
//! **`v1` is not superseded by this and is not going anywhere.** The two answer
//! different questions about the same lift. A linear top-set ladder is the right
//! tool for a short or interrupted window — the weeks before Christmas, a block
//! broken up by travel — and periodisation is the right tool when the calendar
//! gives seven weeks or more. Which one a programme uses is a decision taken per
//! block, from the number of weeks available.

pub mod block;

pub use block::{Block, InvalidBlock, Phase, WeekPlan};

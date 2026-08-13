//! Pagination, which is an artefact of how this source answers.
//!
//! It lives here, in the adapter, and goes no further: a landing record
//! corresponds to one workout as served, page boundaries are preserved
//! nowhere, and a run has no use for a page number it cannot land. What
//! crosses the port is "there is more" and whatever this adapter needs to ask
//! for it — see [`application::WorkoutEventSource::Resume`].
//!
//! These types exist so a walk cannot acquire a stray `+ 1` or an off-by-one
//! at the last page.

use std::{fmt, num::NonZeroU32};

/// A page to ask for. One-based, because the source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageNumber(NonZeroU32);

impl PageNumber {
    pub const fn first() -> Self {
        Self(NonZeroU32::MIN)
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Pages are one-based at the source. A zero would be the source contradicting
/// its own contract; treating it as the first page is harmless, since the
/// number is only ever echoed back.
impl From<u32> for PageNumber {
    fn from(page: u32) -> Self {
        NonZeroU32::new(page).map_or_else(Self::first, Self)
    }
}

impl fmt::Display for PageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many pages the source says there are.
///
/// Asking beyond this is an error at the source rather than an empty page, so
/// a walk must stop exactly here rather than reading until it runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageCount(u32);

impl PageCount {
    /// Whether this page is one the source will serve.
    pub const fn contains(self, page: PageNumber) -> bool {
        page.get() <= self.0
    }
}

impl From<u32> for PageCount {
    fn from(count: u32) -> Self {
        Self(count)
    }
}

impl fmt::Display for PageCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

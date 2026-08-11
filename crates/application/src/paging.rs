//! Pagination, which is an artefact of the request rather than of the data.
//!
//! These types exist so a run can walk a source's pages without a stray `+ 1`
//! or an off-by-one at the last page. They are deliberately not in `domain`: a
//! landing record corresponds to one workout as served, and page boundaries
//! are not preserved anywhere.

use std::{fmt, num::NonZeroU32};

/// A page to ask for. One-based, because sources are.
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
    pub const fn new(count: u32) -> Self {
        Self(count)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Whether this page is one the source will serve.
    pub const fn contains(self, page: PageNumber) -> bool {
        page.get() <= self.0
    }
}

impl fmt::Display for PageCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

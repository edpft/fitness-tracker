//! Sequences that cannot be too short.
//!
//! Two of them, because the entity has two different minimums and they mean
//! different things. An exercise holds at least one set, since an exercise
//! nobody performed is not an observation. A superset holds at least two
//! members, because "performed back to back" is a relation and one thing is not
//! back to back with anything.
//!
//! Both store the mandatory elements as their own fields rather than validating
//! a `Vec`. That is § 24 read strictly: the minimum is not a rule the type
//! checks, it is the shape the type has. It also removes every panic path —
//! `first` returns a `&T` because there is a `T` to borrow, not because a
//! constructor promised there would be — which matters where `panic`, `unwrap`
//! and `unreachable` are all `forbid`.

use std::{fmt, iter};

/// Why a sequence was too short for the thing it was meant to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TooShort {
    #[error("expected at least one, found none")]
    Empty,
    #[error("expected at least two, found {found}")]
    FewerThanTwo { found: usize },
}

/// One or more, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<T> {
    /// The infallible constructor. Prefer it — a caller that already has the
    /// first element should not have to handle an error that cannot happen.
    pub const fn of(head: T, tail: Vec<T>) -> Self {
        Self { head, tail }
    }

    /// # Errors
    ///
    /// [`TooShort::Empty`] if there is nothing in it.
    pub fn new(items: Vec<T>) -> Result<Self, TooShort> {
        let mut items = items.into_iter();
        let head = items.next().ok_or(TooShort::Empty)?;
        Ok(Self::of(head, items.collect()))
    }

    /// The one that is always there.
    pub const fn first(&self) -> &T {
        &self.head
    }

    /// Named `count` rather than `len` because a length invites an `is_empty`
    /// beside it, and the answer here is always the same.
    pub const fn count(&self) -> usize {
        self.tail.len().saturating_add(1)
    }

    /// Map every element, keeping the guarantee.
    ///
    /// **Not `iter().map().collect()` back through `new`.** That round trip
    /// makes an infallible operation fallible: mapping cannot empty a sequence,
    /// so a caller should not be handed an error it would have to invent a
    /// response to. The index comes along because position is meaningful here —
    /// the last set of a group rests differently from the ones before it.
    pub fn map_indexed<U>(&self, mut f: impl FnMut(usize, &T) -> U) -> NonEmpty<U> {
        NonEmpty {
            head: f(0, &self.head),
            tail: self
                .tail
                .iter()
                .enumerate()
                .map(|(index, item)| f(index + 1, item))
                .collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        iter::once(&self.head).chain(self.tail.iter())
    }
}

/// Two or more, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtLeastTwo<T> {
    first: T,
    second: T,
    rest: Vec<T>,
}

impl<T> AtLeastTwo<T> {
    pub const fn of(first: T, second: T, rest: Vec<T>) -> Self {
        Self {
            first,
            second,
            rest,
        }
    }

    /// # Errors
    ///
    /// [`TooShort`] if there are fewer than two. Both shortfalls are
    /// distinguished, because "one member" and "no members" are different
    /// mistakes in a source's data and a refusal has to say which.
    pub fn new(items: Vec<T>) -> Result<Self, TooShort> {
        let found = items.len();
        let mut items = items.into_iter();
        let first = items.next().ok_or(TooShort::Empty)?;
        let second = items.next().ok_or(TooShort::FewerThanTwo { found })?;
        Ok(Self::of(first, second, items.collect()))
    }

    /// The two that are always there.
    ///
    /// Infallible, for the same reason [`NonEmpty::first`] is: a caller that has
    /// an `AtLeastTwo` should not have to handle the absence of an element the
    /// type exists to guarantee. Without these, code mapping a superset member by
    /// member has to reach through `iter()` and invent a branch for a case the
    /// constructor already ruled out.
    pub const fn first(&self) -> &T {
        &self.first
    }

    pub const fn second(&self) -> &T {
        &self.second
    }

    pub const fn count(&self) -> usize {
        self.rest.len().saturating_add(2)
    }

    /// Map every element, keeping the guarantee. [`NonEmpty::map_indexed`]'s
    /// reasoning: mapping cannot shorten a sequence, so nothing here can fail.
    pub fn map_indexed<U>(&self, mut f: impl FnMut(usize, &T) -> U) -> AtLeastTwo<U> {
        AtLeastTwo {
            first: f(0, &self.first),
            second: f(1, &self.second),
            rest: self
                .rest
                .iter()
                .enumerate()
                .map(|(index, item)| f(index.saturating_add(2), item))
                .collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        iter::once(&self.first)
            .chain(iter::once(&self.second))
            .chain(self.rest.iter())
    }
}

// `IntoIterator` on the reference, not on the value: these exist to be read,
// and consuming one back into a `Vec` discards the guarantee that made it worth
// building.

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<'a, T> IntoIterator for &'a AtLeastTwo<T> {
    type Item = &'a T;
    type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<T: fmt::Display> fmt::Display for NonEmpty<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.head)?;
        for item in &self.tail {
            write!(f, ", {item}")?;
        }
        Ok(())
    }
}

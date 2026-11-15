//! The store as a writer of a node nothing derives sees it, and where each
//! such node stands (#805).
//!
//! **A writer of one gets no pool**, only an [`Authoring`]: a write is a
//! [`Writing`] that names what it authors, whose commit moves the revision of
//! each of those nodes in the same transaction, and a read is a [`Reading`],
//! which is never committed. So such an adapter has no way to write that does
//! not say what it moves, and a page drawn from what it writes is never served
//! stale.
//!
//! **A write of what no page reads authors nothing**, and touches only tables
//! no page reads: a table holds what a page reads or bookkeeping about the
//! source it came from, never both.

use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use application::{Authors, Calendar, Generation, Node, Revision, Revisions, StoreError};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::store_error;

/// The store, for an adapter writing what a page reads.
#[derive(Debug, Clone)]
pub struct Authoring {
    pool: SqlitePool,
}

impl Authoring {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// A snapshot to read from.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if no connection is free.
    pub async fn reading(&self) -> Result<Reading, StoreError> {
        super::reading(&self.pool)
            .await
            .map(Reading)
            .map_err(|error| store_error(&error))
    }

    /// A write of what `W` authors, holding the store's write lock from its
    /// first statement.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if no connection is free or the lock is not given up.
    pub async fn writing<W: Authors>(&self) -> Result<Writing<W>, StoreError> {
        super::writing(&self.pool)
            .await
            .map(|transaction| Writing {
                transaction,
                author: PhantomData,
            })
            .map_err(|error| store_error(&error))
    }
}

/// A read: a snapshot, dropped rather than committed, so nothing written
/// through it is kept.
pub struct Reading(Transaction<'static, Sqlite>);

impl Deref for Reading {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.0
    }
}

impl DerefMut for Reading {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

/// A write of what `W` authors, kept only by [`Writing::commit`].
pub struct Writing<W> {
    transaction: Transaction<'static, Sqlite>,
    author: PhantomData<fn() -> W>,
}

impl<W> Deref for Writing<W> {
    type Target = Transaction<'static, Sqlite>;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}

impl<W> DerefMut for Writing<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.transaction
    }
}

impl<W: Authors> Writing<W> {
    /// Keep what was written, and move the revision of everything `W`
    /// authors with it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store refuses either.
    pub async fn commit(mut self) -> Result<(), StoreError> {
        for node in W::authors() {
            let Some(node) = stored(&node) else {
                return Err(StoreError::Corrupt {
                    detail: format!("{node:?} is authored but has no revision"),
                });
            };
            sqlx::query!(
                "INSERT INTO node_revision (node, revision) VALUES (?, 1)
                 ON CONFLICT (node) DO UPDATE SET revision = revision + 1",
                node
            )
            .execute(&mut *self.transaction)
            .await
            .map_err(|error| store_error(&error))?;
        }
        self.transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))
    }
}

/// The name a node's revision is kept under, for a node that has one.
fn stored(node: &Node) -> Option<&'static str> {
    [
        (Node::PLAN, "plan"),
        (Node::PRESCRIPTIONS, "prescriptions"),
        (Node::WORKOUTS, "workouts"),
        (Node::DIARY, "diary"),
        (Node::Generation(Generation::Equipment), "equipment"),
        (Node::Generation(Generation::Classes), "classes"),
        (
            Node::Generation(Generation::Calendar(Calendar::School)),
            "calendar school",
        ),
        (
            Node::Generation(Generation::Calendar(Calendar::Public)),
            "calendar public",
        ),
    ]
    .into_iter()
    .find_map(|(named, stored)| (&named == node).then_some(stored))
}

/// Where each node nothing derives stands, in the store.
#[derive(Debug, Clone)]
pub struct SqliteRevisions {
    pool: SqlitePool,
}

impl SqliteRevisions {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl Revisions for SqliteRevisions {
    async fn revision(&self, node: &Node) -> Result<Revision, StoreError> {
        let Some(node) = stored(node) else {
            return Ok(Revision::default());
        };
        let revision =
            sqlx::query_scalar!("SELECT revision FROM node_revision WHERE node = ?", node)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
        revision
            .map(|revision| {
                u64::try_from(revision).map_err(|_| StoreError::Corrupt {
                    detail: format!("a revision of {revision} is not a count"),
                })
            })
            .transpose()
            .map(|revision| Revision::from(revision.unwrap_or_default()))
    }
}

//! The store as a writer of a node nothing derives sees it, and where each
//! such node stands (#805).
//!
//! **A writer of one gets no pool**, only an [`Authoring`]: a write is a
//! [`Writing`], whose commit moves the revision of every node the writer
//! authors in the same transaction, and a read is a [`Reading`], which is
//! never committed. So such an adapter has no way to write that moves no
//! revision, and a page drawn from what it writes is never served stale.

use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use application::{
    Authors, Calendar, Generation, Node, Revision, Revisions, StoreError, graph::Authored,
};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::store_error;

/// The store, for an adapter writing what `W` authors.
pub struct Authoring<W> {
    pool: SqlitePool,
    author: PhantomData<fn() -> W>,
}

impl<W> Clone for Authoring<W> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            author: PhantomData,
        }
    }
}

impl<W> std::fmt::Debug for Authoring<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authoring").finish_non_exhaustive()
    }
}

impl<W: Authors> Authoring<W> {
    pub const fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            author: PhantomData,
        }
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

    /// A write, holding the store's write lock from its first statement.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if no connection is free or the lock is not given up.
    pub async fn writing(&self) -> Result<Writing<W>, StoreError> {
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
const fn stored(node: &Node) -> Option<&'static str> {
    Some(match node {
        Node::Authored(Authored::Plan) => "plan",
        Node::Authored(Authored::Prescriptions) => "prescriptions",
        Node::Authored(Authored::Workouts) => "workouts",
        Node::Authored(Authored::Diary) => "diary",
        Node::Generation(Generation::Equipment) => "equipment",
        Node::Generation(Generation::Classes) => "classes",
        Node::Generation(Generation::Calendar(Calendar::School)) => "calendar school",
        Node::Generation(Generation::Calendar(Calendar::Public)) => "calendar public",
        Node::Landing(_)
        | Node::Normalised(_)
        | Node::Canonical(_)
        | Node::Table(_)
        | Node::Edits(_)
        | Node::Interpretive(_)
        | Node::Date
        | Node::DayPart => return None,
    })
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

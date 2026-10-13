//! The devices the operator uses, and when each was in use (#427).
//!
//! **What the record shows, with what the operator stated applied.** The
//! register is `domain`'s ([`register`]); this reads the two things it is a
//! function of and keeps a statement once the register says it can be made.
//! Every frontend reaches the register through here, so the terminal and the
//! browser offer the same thing without either knowing how it is derived.

use domain::device::{
    AddedDevice, Contradiction, Device, DeviceStatement, Stated, Statement, check, register,
};
use jiff::Timestamp;

use crate::{
    error::StoreError,
    ports::{DeviceStatements, RecordedDevices},
};

/// Why a statement was not kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatementRefused {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Contradiction(#[from] Contradiction),
}

/// The register use case.
pub struct DeviceRegister<R, S> {
    recorded: R,
    statements: S,
}

impl<R, S> DeviceRegister<R, S>
where
    R: RecordedDevices + Sync,
    S: DeviceStatements + Sync,
{
    pub const fn new(recorded: R, statements: S) -> Self {
        Self {
            recorded,
            statements,
        }
    }

    /// Every device, ordered by first use.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if either store is unavailable.
    pub async fn devices(&self) -> Result<Vec<Device>, StoreError> {
        let sightings = self.recorded.sightings().await?;
        let statements = self.statements.statements().await?;
        Ok(register(&sightings, &statements))
    }

    /// Keep a statement about a device, and answer with the register it
    /// leaves.
    ///
    /// # Errors
    ///
    /// [`StatementRefused::Contradiction`] if the register would not apply it
    /// — it names no device, or puts a last use before a first — and
    /// [`StatementRefused::Store`] if either store is unavailable.
    pub async fn state(
        &self,
        at: Timestamp,
        statement: DeviceStatement,
    ) -> Result<Vec<Device>, StatementRefused> {
        let sightings = self.recorded.sightings().await?;
        let mut statements = self.statements.statements().await?;
        let stated = Stated {
            at,
            statement: Statement::About(statement.clone()),
        };
        check(&register(&sightings, &statements), &stated)?;
        self.statements.state(at, &statement).await?;
        statements.push(stated);
        Ok(register(&sightings, &statements))
    }

    /// Keep a device no recording names, and answer with the register it
    /// leaves.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if either store is unavailable.
    pub async fn add(&self, at: Timestamp, added: AddedDevice) -> Result<Vec<Device>, StoreError> {
        self.statements.add(at, &added).await?;
        self.devices().await
    }
}

//! Each translation's version, held to what it produces (#606).
//!
//! A layer is derived again when its translation's version changes, and not
//! when the build does, so a translator whose output moved under the same
//! version leaves every layer it derived standing as it was. Each translator's
//! suite translates its own accounts once more and hands the result here,
//! which compares a digest of it with the one pinned beside the version.
//!
//! **A row is added, never edited.** A changed output under the same version is
//! the mistake this exists to catch, so the fix is a raised `VERSION` and a new
//! row beside the old one.

use application::{
    StoreError, Translation,
    ports::{AccountReader, Reading, Translator},
};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

/// Each translator, a version it has had, and the digest of what its suite's
/// accounts translated to under that version.
const PINNED: &[(&str, u32, &str)] = &[
    (
        "BtwbTranslator",
        1,
        "0781c39268b75d0096b334fb4e43649e1d55ad75b81a05b94995be639c64b31b",
    ),
    (
        "GarminActivityTranslator",
        1,
        "51a5e3f5a0762d0e7f1bca36d2b916b9f67b1567678f94808fcf7990e0271643",
    ),
    (
        "GarminDailyHeartRateTranslator",
        1,
        "89bf67ba268f1a421ac7968131fa630e76fe1130047177170067d67fc536d2e7",
    ),
    (
        "GarminGymTranslator",
        1,
        "65d79c904623e8adcaba931636db09a45027390029d58b21e2ed5cdb0bc48e5a",
    ),
    (
        "GarminHrvTranslator",
        1,
        "74714cd8d4670812a126c6e301a18eabf68aed633b4b2b266825b2899c9986db",
    ),
    (
        "GarminRestingHeartRateTranslator",
        1,
        "d935e8f11910f2c92c07fc70a7ece414043c8d0f2ee2b6fe57e46daef447a88e",
    ),
    (
        "GarminSleepTranslator",
        1,
        "9bf8a3e96639ec3c1024aa3de4ae9a325f679826240a70ddba1a51734d9d126f",
    ),
    (
        "HevySessionTranslator",
        1,
        "25098948e67da12aee337caebef9cf856ce2b26d9948a232ffe31000d73f2108",
    ),
    (
        "MyFitnessPalWeighInTranslator",
        1,
        "19da92067ccd927b045f756623e11141a8ea9bdd468f37c0feb6241aa1ed8a66",
    ),
    (
        "PelotonSessionTranslator",
        1,
        "20f9edca248acd6953d56ac7235298ef6838cb7fc439d50e58b207c6f7367cba",
    ),
    (
        "SpreadsheetSessionTranslator",
        1,
        "bdb184fd92ea3d086798efde2b6919a9e0e90dfceee2313e0677c790b6e913f1",
    ),
    (
        "SpreadsheetTranslator",
        1,
        "30991de4c99948f8eeb6f6ded502612600b15cdc6802ab1aa1168d12de5d44e8",
    ),
    (
        "SpreadsheetWeighInTranslator",
        1,
        "d01983d53d449d3b7ffba91ef923a66fa73432df9866543e3be82aadaa7b0aa8",
    ),
    (
        "WithingsWeighInTranslator",
        1,
        "af34faf6bcd22058724d99efe1f12624fc796af013795eed394856b05aebedca",
    ),
];

/// A translator's output no longer matches what its version pinned.
#[derive(Debug, thiserror::Error)]
#[error(
    "{translator} {version} translates its suite's accounts to {digest}, which no row pins. \
     If its output was meant to change, raise its VERSION and add a row: \
     (\"{translator}\", {next}, \"{digest}\")"
)]
pub struct Unpinned {
    translator: &'static str,
    version: u32,
    next: u32,
    digest: String,
}

/// Whether what `T` produced from its suite's accounts is what its version
/// pinned.
///
/// # Errors
///
/// [`Unpinned`] where it is not, naming the row that would pin it.
pub fn pinned<T>(translations: &[Translation<T::Entity>]) -> Result<(), Unpinned>
where
    T: Translator,
    T::Entity: std::fmt::Debug,
{
    let translator = std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or_default();
    let digest =
        Sha256::digest(format!("{translations:#?}"))
            .iter()
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            });
    let latest = PINNED
        .iter()
        .filter(|(name, _, _)| *name == translator)
        .map(|(_, version, _)| *version)
        .max()
        .unwrap_or(0);
    if PINNED.contains(&(translator, T::VERSION, digest.as_str())) {
        Ok(())
    } else {
        Err(Unpinned {
            translator,
            version: T::VERSION,
            next: if T::VERSION > latest {
                T::VERSION
            } else {
                latest + 1
            },
            digest,
        })
    }
}

/// Every account a reader holds, for a suite that lands its records in a store
/// and so has no accounts in hand.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn accounts<R: AccountReader + Sync>(reader: &R) -> Result<Vec<R::Account>, StoreError> {
    Ok(reader.accounts(&Reading::Whole).await?.now)
}

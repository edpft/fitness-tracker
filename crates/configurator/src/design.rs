//! Designing each workout and each primary progression, over this build's store
//! (#192).

use application::{
    DesignError,
    design::{Designing, InForce},
};
use domain::{
    prescription::{LinearProgression, SbsProgression, Workout},
    schedule::Relative,
};

use crate::store::Store;

fn designing(
    store: &Store,
) -> Designing<impl application::WorkoutStore + application::ProgressionStore + Sync + use<>> {
    Designing::new(store.parameters())
}

/// The versions a plan made now would take.
///
/// # Errors
///
/// [`DesignError::Store`] if the store cannot be read.
pub async fn in_force(store: &Store) -> Result<InForce, DesignError> {
    designing(store).in_force(jiff::Timestamp::now()).await
}

/// # Errors
///
/// See [`Designing::workout`].
pub async fn workout(
    store: &Store,
    intensity: Relative,
    workout: &Workout,
) -> Result<(), DesignError> {
    designing(store)
        .workout(intensity, workout, jiff::Timestamp::now())
        .await
}

/// # Errors
///
/// See [`Designing::linear`].
pub async fn linear(store: &Store, linear: &LinearProgression) -> Result<(), DesignError> {
    designing(store)
        .linear(linear, jiff::Timestamp::now())
        .await
}

/// # Errors
///
/// See [`Designing::sbs`].
pub async fn sbs(store: &Store, sbs: &SbsProgression) -> Result<(), DesignError> {
    designing(store).sbs(sbs, jiff::Timestamp::now()).await
}

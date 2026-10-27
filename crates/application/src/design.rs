//! Designing each workout and each primary progression (#192).
//!
//! **On their own, and never as a side effect of planning** (§ 14.1). Each is
//! a dated version: authoring one puts it in force from now, and a plan already
//! made goes on reading the versions in force when it was planned, so a change
//! reaches only the plans made after it.

use domain::{
    prescription::{ByIntensity, LinearProgression, SbsProgression, Workout},
    schedule::Relative,
};
use jiff::Timestamp;

use crate::{DesignError, ProgressionStore, WorkoutStore};

/// The latest version of each, with when it was authored. `None` for one never
/// authored, which a store `init` has not seeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InForce {
    pub workouts: ByIntensity<Option<(Timestamp, Workout)>>,
    pub linear: Option<(Timestamp, LinearProgression)>,
    pub sbs: Option<(Timestamp, SbsProgression)>,
}

/// The use case, over the store that keeps the versions.
pub struct Designing<S> {
    store: S,
}

impl<S: WorkoutStore + ProgressionStore + Sync> Designing<S> {
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// The versions a plan made at `at` would take.
    ///
    /// # Errors
    ///
    /// [`DesignError::Store`] if the store cannot be read.
    pub async fn in_force(&self, at: Timestamp) -> Result<InForce, DesignError> {
        Ok(InForce {
            workouts: ByIntensity {
                higher: self.store.workout_at(Relative::Higher, at).await?,
                lower: self.store.workout_at(Relative::Lower, at).await?,
            },
            linear: self.store.linear_at(at).await?,
            sbs: self.store.sbs_at(at).await?,
        })
    }

    /// Author a version of the workout for sessions of `intensity`, in force
    /// from `at`. The other workout is untouched.
    ///
    /// # Errors
    ///
    /// [`DesignError::Unchanged`] for the version already in force: a second
    /// copy of it would date a change that was never made.
    pub async fn workout(
        &self,
        intensity: Relative,
        workout: &Workout,
        at: Timestamp,
    ) -> Result<(), DesignError> {
        let was = self.store.workout_at(intensity, at).await?;
        if was.map(|(_, was)| was).as_ref() == Some(workout) {
            return Err(DesignError::Unchanged {
                what: match intensity {
                    Relative::Higher => "higher-intensity workout",
                    Relative::Lower => "lower-intensity workout",
                },
            });
        }
        Ok(self.store.author_workout(intensity, at, workout).await?)
    }

    /// Author a version of linear progression, in force from `at`.
    ///
    /// # Errors
    ///
    /// [`DesignError::NotADrop`] for an entry drop or a reset that does not
    /// lower the load, and [`DesignError::Unchanged`] as for a workout.
    pub async fn linear(
        &self,
        linear: &LinearProgression,
        at: Timestamp,
    ) -> Result<(), DesignError> {
        for (what, drop) in [
            ("entry drop", linear.entry_drop),
            ("first reset's drop", linear.first_reset.drop),
            ("second reset's drop", linear.second_reset.drop),
        ] {
            if drop.as_basis_points() > 0 {
                return Err(DesignError::NotADrop {
                    what,
                    drop: drop.to_string(),
                });
            }
        }
        if self.store.linear_at(at).await?.map(|(_, was)| was).as_ref() == Some(linear) {
            return Err(DesignError::Unchanged {
                what: "linear progression",
            });
        }
        Ok(self.store.author_linear(at, linear).await?)
    }

    /// Author a version of SBS — its ramp and its rest — in force from `at`.
    ///
    /// # Errors
    ///
    /// [`DesignError::Unchanged`] as for a workout.
    pub async fn sbs(&self, sbs: &SbsProgression, at: Timestamp) -> Result<(), DesignError> {
        if self.store.sbs_at(at).await?.map(|(_, was)| was).as_ref() == Some(sbs) {
            return Err(DesignError::Unchanged { what: "SBS" });
        }
        Ok(self.store.author_sbs(at, sbs).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use domain::{
        measure::RepCount,
        prescription::{
            AccessoryScheme, LinearProgression, Percentage, SbsProgression, Section, Target,
            Workout, seed,
        },
        schedule::Relative,
    };
    use jiff::{Timestamp, ToSpan as _};

    use super::Designing;
    use crate::{DesignError, ProgressionStore, StoreError, WorkoutStore};

    /// Every version authored, in the order it was.
    #[derive(Default)]
    struct Versions {
        workouts: Mutex<Vec<(Relative, Timestamp, Workout)>>,
        linear: Mutex<Vec<(Timestamp, LinearProgression)>>,
        sbs: Mutex<Vec<(Timestamp, SbsProgression)>>,
    }

    fn at<T: Clone>(
        versions: &Mutex<Vec<(Timestamp, T)>>,
        at: Timestamp,
    ) -> Option<(Timestamp, T)> {
        versions
            .lock()
            .ok()?
            .iter()
            .filter(|(when, _)| *when <= at)
            .max_by_key(|(when, _)| *when)
            .cloned()
    }

    fn push<T>(versions: &Mutex<Vec<T>>, value: T) -> Result<(), StoreError> {
        versions
            .lock()
            .map_err(|_| StoreError::Unavailable {
                detail: "poisoned".to_owned(),
            })?
            .push(value);
        Ok(())
    }

    impl WorkoutStore for Versions {
        async fn workout_at(
            &self,
            intensity: Relative,
            when: Timestamp,
        ) -> Result<Option<(Timestamp, Workout)>, StoreError> {
            Ok(self.workouts.lock().ok().and_then(|all| {
                all.iter()
                    .filter(|(of, at, _)| *of == intensity && *at <= when)
                    .max_by_key(|(_, at, _)| *at)
                    .map(|(_, at, workout)| (*at, workout.clone()))
            }))
        }

        async fn author_workout(
            &self,
            intensity: Relative,
            when: Timestamp,
            workout: &Workout,
        ) -> Result<(), StoreError> {
            push(&self.workouts, (intensity, when, workout.clone()))
        }
    }

    impl ProgressionStore for Versions {
        async fn linear_at(
            &self,
            when: Timestamp,
        ) -> Result<Option<(Timestamp, LinearProgression)>, StoreError> {
            Ok(at(&self.linear, when))
        }

        async fn author_linear(
            &self,
            when: Timestamp,
            linear: &LinearProgression,
        ) -> Result<(), StoreError> {
            push(&self.linear, (when, linear.clone()))
        }

        async fn sbs_at(
            &self,
            when: Timestamp,
        ) -> Result<Option<(Timestamp, SbsProgression)>, StoreError> {
            Ok(at(&self.sbs, when))
        }

        async fn author_sbs(
            &self,
            when: Timestamp,
            sbs: &SbsProgression,
        ) -> Result<(), StoreError> {
            push(&self.sbs, (when, sbs.clone()))
        }
    }

    fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread().build()
    }

    /// **Each workout is edited on its own** (operator, 2026-10-27): changing
    /// Monday's leaves Friday's as it was, and neither changes before it is
    /// authored.
    #[test]
    fn a_workout_changes_on_its_own_and_from_when_it_is_authored() {
        runtime().expect("a runtime").block_on(async {
            let designing = Designing::new(Versions::default());
            let first = seed::workout().expect("the seed builds");
            let then = Timestamp::UNIX_EPOCH;
            let now = then + 1.hour();
            for intensity in [Relative::Higher, Relative::Lower] {
                designing
                    .workout(intensity, &first, then)
                    .await
                    .expect("authored");
            }

            let mut monday = first.clone();
            monday.reps.sections.insert(
                Section::Hypertrophy,
                AccessoryScheme {
                    reps: Target::between(
                        RepCount::new(6).expect("six"),
                        RepCount::new(8).expect("eight"),
                    )
                    .expect("a range"),
                    sets: RepCount::new(3).expect("three"),
                },
            );
            designing
                .workout(Relative::Lower, &monday, now)
                .await
                .expect("authored");

            let before = designing.in_force(then).await.expect("read");
            let after = designing.in_force(now).await.expect("read");
            assert_eq!(before.workouts.lower, Some((then, first.clone())));
            assert_eq!(after.workouts.lower, Some((now, monday)));
            assert_eq!(after.workouts.higher, Some((then, first)));
        });
    }

    /// **The version in force, authored again, is refused**: it would date a
    /// change nobody made.
    #[test]
    fn the_version_in_force_is_not_authored_twice() {
        runtime().expect("a runtime").block_on(async {
            let designing = Designing::new(Versions::default());
            let sbs = seed::sbs().expect("the seed builds");
            let then = Timestamp::UNIX_EPOCH;
            designing.sbs(&sbs, then).await.expect("authored");

            let again = designing.sbs(&sbs, then + 1.hour()).await;
            assert_eq!(again, Err(DesignError::Unchanged { what: "SBS" }));
        });
    }

    /// **A reset that adds load is not a reset**, so a drop typed without its
    /// sign is refused rather than stored as a climb.
    #[test]
    fn a_drop_that_raises_the_load_is_refused() {
        runtime().expect("a runtime").block_on(async {
            let designing = Designing::new(Versions::default());
            let mut linear = seed::linear().expect("the seed builds");
            linear.first_reset.drop = "10%".parse::<Percentage>().expect("a percentage");

            let refused = designing.linear(&linear, Timestamp::UNIX_EPOCH).await;
            assert_eq!(
                refused,
                Err(DesignError::NotADrop {
                    what: "first reset's drop",
                    drop: "10%".to_owned(),
                })
            );
        });
    }
}

//! Every derivation this build makes, and so what each page collects (#809).
//!
//! **Each derivation is named by the type `wiring` builds it as**, so what it
//! reads is what its ports declare: an arm that built it from another reader
//! would not compile against its name here. What a page collects is what it
//! reads and everything upstream of that, and nothing here lists either.

use std::sync::OnceLock;

use application::{
    Part,
    graph::{Derivation, Graph, Input, Node, Output, Reads},
    next::{CanonicalPorts, Scope},
    normalise::Normalisation,
};
use domain::landing::LandingStream;
use infrastructure::{
    BtwbExportAccountReader, BtwbExportLandingStore, BtwbTranslator, GarminActivityLandingStore,
    GarminActivityTranslator, GarminDailyHeartRateAccountReader, GarminDailyHeartRateLandingStore,
    GarminDailyHeartRateTranslator, GarminDailySummaryAccountReader,
    GarminDailySummaryLandingStore, GarminGymAccountReader, GarminHrvAccountReader,
    GarminHrvLandingStore, GarminHrvTranslator, GarminSleepAccountReader, GarminSleepLandingStore,
    GarminSleepTranslator, GarminStressTranslator, HevySessionAccountReader, HevySessionTranslator,
    HevyWorkoutLandingStore, MyFitnessPalExportAccountReader, MyFitnessPalExportLandingStore,
    MyFitnessPalWeighInTranslator, PelotonRideLandingStore, PelotonSessionAccountReader,
    SpreadsheetFileAccountReader, SpreadsheetFileLandingStore, SpreadsheetTranslator,
    SqliteBtwbStore, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCanonicalWeighInStore, SqliteCyclingSessionStore, SqliteDailyHeartRateStore,
    SqliteGarminActivityStore, SqliteGymSessionStore, SqliteMyFitnessPalStore,
    SqliteNormalisationRunLog, SqliteNormalisedCyclingSessionReader,
    SqliteNormalisedGymSessionReader, SqliteNormalisedWeighInReader, SqliteOvernightHrvStore,
    SqliteRefusalStore, SqliteSleepStore, SqliteSpreadsheetStore, SqliteStressStore,
    SqliteWeighInStore, WithingsMeasurementLandingStore, WithingsWeighInAccountReader,
    WithingsWeighInTranslator, peloton::PelotonSessionTranslator,
};

use crate::store::Spanned;
use crate::wiring::SystemClock;

/// A struct of what something reads through, which reads what each of them
/// does (#809).
///
/// **Its fields are its declaration**: a struct built as one has nothing to
/// read with but them, so what it reads and what it declares cannot differ.
macro_rules! reads {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($(#[$field_meta:meta])* $field:ident: $type:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $($(#[$field_meta])* $field: $type),+
        }

        impl application::graph::Input for $name {
            fn reads() -> application::graph::Reads {
                application::graph::Reads::default()
                    $(.and(<$type as application::graph::Input>::reads()))+
            }
        }
    };
}
pub(crate) use reads;

/// What a page reads, declared as [`reads!`] declares it, and each field
/// read only through [`read!`], which spans the read (#804).
///
/// **A page cannot read unrecorded**: its fields are
/// [`Read`](crate::telemetry::Read)s, which give up what they hold only as a
/// [`Reading`](crate::telemetry::Reading).
macro_rules! page {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($(#[$field_meta:meta])* $field:ident: $type:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $($(#[$field_meta])* $field: $crate::telemetry::Read<$type>),+
        }

        impl application::graph::Input for $name {
            fn reads() -> application::graph::Reads {
                application::graph::Reads::default()
                    $(.and(<$type as application::graph::Input>::reads()))+
            }
        }
    };
}
pub(crate) use page;

/// Read a page's field, as a `reading` step named for the field (#804):
/// `read!(self.field.method(args))`, or `read!(self.field, |field| …)` where
/// the field is an argument rather than the receiver.
macro_rules! read {
    ($page:ident . $field:ident . $method:ident ( $($argument:expr),* $(,)? )) => {
        $crate::telemetry::Reading::named(stringify!($field), &$page.$field)
            .read(|field| field.$method($($argument),*))
    };
    ($page:ident . $field:ident, $read:expr) => {
        $crate::telemetry::Reading::named(stringify!($field), &$page.$field).read($read)
    };
}
pub(crate) use read;

/// A stream's normalisation, over this build's run log and refusals.
type Normalising<R, T, W> = Normalisation<
    R,
    T,
    W,
    Spanned<SqliteRefusalStore>,
    Spanned<SqliteNormalisationRunLog>,
    SystemClock,
>;

pub(crate) type HevyWorkouts = Normalising<
    Spanned<HevySessionAccountReader>,
    HevySessionTranslator,
    Spanned<SqliteGymSessionStore>,
>;
pub(crate) type PelotonRides = Normalising<
    Spanned<PelotonSessionAccountReader>,
    PelotonSessionTranslator,
    Spanned<SqliteCyclingSessionStore>,
>;
pub(crate) type WithingsMeasurements = Normalising<
    Spanned<WithingsWeighInAccountReader>,
    WithingsWeighInTranslator,
    Spanned<SqliteWeighInStore>,
>;
pub(crate) type GarminHrv = Normalising<
    Spanned<GarminHrvAccountReader>,
    GarminHrvTranslator,
    Spanned<SqliteOvernightHrvStore>,
>;
pub(crate) type GarminActivities = Normalising<
    Spanned<GarminGymAccountReader>,
    GarminActivityTranslator,
    Spanned<SqliteGarminActivityStore>,
>;
pub(crate) type GarminDailyHeartRate = Normalising<
    Spanned<GarminDailyHeartRateAccountReader>,
    GarminDailyHeartRateTranslator,
    Spanned<SqliteDailyHeartRateStore>,
>;
pub(crate) type GarminSleep = Normalising<
    Spanned<GarminSleepAccountReader>,
    GarminSleepTranslator,
    Spanned<SqliteSleepStore>,
>;
pub(crate) type GarminDailySummary = Normalising<
    Spanned<GarminDailySummaryAccountReader>,
    GarminStressTranslator,
    Spanned<SqliteStressStore>,
>;
pub(crate) type Spreadsheets = Normalising<
    Spanned<SpreadsheetFileAccountReader>,
    SpreadsheetTranslator,
    Spanned<SqliteSpreadsheetStore>,
>;
pub(crate) type Btwb =
    Normalising<Spanned<BtwbExportAccountReader>, BtwbTranslator, Spanned<SqliteBtwbStore>>;
pub(crate) type MyFitnessPal = Normalising<
    Spanned<MyFitnessPalExportAccountReader>,
    MyFitnessPalWeighInTranslator,
    Spanned<SqliteMyFitnessPalStore>,
>;

/// Each canonical layer, and the reader of the normalised layer it is built
/// from.
pub(crate) type Canonical = CanonicalPorts<
    Spanned<SqliteNormalisedGymSessionReader>,
    Spanned<SqliteCanonicalGymSessionStore>,
    Spanned<SqliteNormalisedCyclingSessionReader>,
    Spanned<SqliteCanonicalCyclingSessionStore>,
    Spanned<SqliteNormalisedWeighInReader>,
    Spanned<SqliteCanonicalWeighInStore>,
>;

/// The normalised layer of the stream named `name`, derived by `D`, and each
/// table `D` writes, which is read wherever that table is (#809).
fn normalised<D: Input + Output>(name: &str) -> Vec<Derivation> {
    LandingStream::try_from(name)
        .map(|stream| Derivation::writing::<D>(Node::Normalised(stream)).collect())
        .unwrap_or_default()
}

fn derivations() -> impl Iterator<Item = Derivation> {
    [
        normalised::<HevyWorkouts>(HevyWorkoutLandingStore::STREAM),
        normalised::<PelotonRides>(PelotonRideLandingStore::STREAM),
        normalised::<WithingsMeasurements>(WithingsMeasurementLandingStore::STREAM),
        normalised::<GarminHrv>(GarminHrvLandingStore::STREAM),
        normalised::<GarminActivities>(GarminActivityLandingStore::STREAM),
        normalised::<GarminDailyHeartRate>(GarminDailyHeartRateLandingStore::STREAM),
        normalised::<GarminSleep>(GarminSleepLandingStore::STREAM),
        normalised::<GarminDailySummary>(GarminDailySummaryLandingStore::STREAM),
        normalised::<Spreadsheets>(SpreadsheetFileLandingStore::STREAM),
        normalised::<Btwb>(BtwbExportLandingStore::STREAM),
        normalised::<MyFitnessPal>(MyFitnessPalExportLandingStore::STREAM),
    ]
    .into_iter()
    .flatten()
    .chain(Canonical::derivations())
}

/// Every derivation this build makes.
pub fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| Graph::of(derivations()))
}

/// The parts a run collecting what `reads` reads takes.
///
/// **A stream read from a folder is not fetched**, so its landing is no part:
/// a run derives what the folder last landed.
#[must_use]
pub fn scope(reads: &Reads) -> Scope {
    let from_a_folder = |stream: &LandingStream| {
        crate::catalogue::lookup(&stream.to_string()).is_some_and(|known| known.system().is_none())
    };
    Scope::new(
        graph()
            .parts(reads)
            .into_iter()
            .filter(|part| !matches!(part, Part::Fetching(stream) if from_a_folder(stream))),
    )
}

/// What `I` reads, and the parts a run collecting it takes.
#[must_use]
pub fn collects<I: Input>() -> Scope {
    scope(&I::reads())
}

#[cfg(test)]
mod tests {
    use application::graph::Node;
    use domain::landing::LandingStream;

    use crate::{catalogue, wiring};

    /// **Every stream `next` derives is a derivation in the graph**, so a
    /// page reading it collects what it is derived from.
    #[test]
    fn every_derived_stream_is_in_the_graph() {
        for known in catalogue::KNOWN
            .iter()
            .filter(|known| wiring::derives(known))
        {
            let stream = LandingStream::try_from(known.name().as_str()).expect("a stream");
            assert!(
                super::graph().reads(&Node::Normalised(stream)).is_some(),
                "{} is derived but not in the graph",
                known.name()
            );
        }
    }
}

//! Informational telemetry (#631): where `telemetry.db` is, which build its
//! spans are stamped with, and the spans this crate opens around the use cases
//! it composes.

use std::path::Path;

use application::{
    CanonicalLayer, DerivedFrom, StoreError,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    next::Canonicalising,
};
use tracing::{Instrument as _, Span, field::Empty};
use tracing_subscriber::layer::SubscriberExt as _;

pub use ::telemetry::Recording;
use ::telemetry::{OUTCOME, Outcome};

/// The file, beside the store and never inside it: the store's migrations are
/// not its migrations, and losing it costs history and nothing else.
const FILE: &str = "telemetry.db";

/// Record this process's spans to the `telemetry.db` beside `store`, until the
/// [`Recording`] is dropped.
///
/// **A second call records nothing.** One subscriber serves a process, so
/// whichever store was named first is where the spans go.
pub fn record(store: &Path) -> Recording {
    let (recorder, recording) =
        ::telemetry::record(&store.with_file_name(FILE), crate::next::BUILD);
    let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(recorder));
    recording
}

/// Each rebuild as a `step` span: its layer, and what it read and wrote.
pub struct Traced<C>(pub C);

fn canonicalising(layer: CanonicalLayer) -> Span {
    tracing::info_span!(
        "step",
        step = "canonicalising",
        layer = layer.as_str(),
        read = Empty,
        written = Empty,
        outcome = Empty,
    )
}

fn ended<T>(span: &Span, done: &Result<T, StoreError>, counts: impl Fn(&T) -> (usize, usize)) {
    match done {
        Ok(done) => {
            let (read, written) = counts(done);
            span.record("read", read);
            span.record("written", written);
            span.record(OUTCOME, Outcome::Succeeded.as_str());
        }
        Err(_) => {
            span.record(OUTCOME, Outcome::Failed.as_str());
        }
    }
}

const fn counts(done: &Canonicalised) -> (usize, usize) {
    (done.read.as_usize(), done.written.as_usize())
}

impl<C: Canonicalising + Sync> Canonicalising for Traced<C> {
    async fn gym(&self) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Gym);
        let done = self.0.gym().instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn cycling(&self) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Cycling);
        let done = self.0.cycling().instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn weigh_ins(&self) -> Result<WeighInsCanonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::WeighIns);
        let done = self.0.weigh_ins().instrument(span.clone()).await;
        ended(&span, &done, |done| {
            (done.read.as_usize(), done.written.as_usize())
        });
        done
    }

    async fn inputs(&self, layer: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
        self.0.inputs(layer).await
    }
}

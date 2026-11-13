//! The school and public holidays, kept as their publishers last served them
//! (#809).
//!
//! **Asked by a run, read by everything else.** A run asks each publisher
//! whether its calendar has changed since it was kept, and one that has not
//! answers so without serving it again. A page, a plan or a walk through it
//! reads what is kept and asks nothing.

use std::future::Future;

use domain::schedule::Holidays;
use jiff::Timestamp;

use crate::{
    SourceError, StoreError,
    graph::Calendar,
    next::{Found, NotDue, Refetch, StepFailure},
    ports::{CalendarPublisher, CalendarStore, HolidayCalendar},
};

/// The calendars, kept and asked after.
pub struct Calendars<S, A, B> {
    store: S,
    school: A,
    public: B,
}

impl<S, A, B> Calendars<S, A, B> {
    pub const fn new(store: S, school: A, public: B) -> Self {
        Self {
            store,
            school,
            public,
        }
    }
}

/// **What is kept is what is read** (#809): the publishers are what a run
/// asks, and nothing reads them.
impl<S: crate::graph::Input, A, B> crate::graph::Input for Calendars<S, A, B> {
    fn reads() -> crate::graph::Reads {
        S::reads()
    }
}

/// Asking whether a calendar has changed, as a step of a run.
pub trait KeptCalendars {
    /// Ask `calendar`'s publisher at `at` whether it has changed since it was
    /// kept, and keep what it serves if it has. **Not asked again within
    /// `refetch`** of the last time, as a stream is not fetched again.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable. A publisher that cannot be
    /// reached is a failed step, and what was kept stands.
    fn ask(
        &self,
        calendar: Calendar,
        at: Timestamp,
        refetch: Refetch,
    ) -> impl Future<Output = Result<Result<Found, StepFailure>, StoreError>> + Send;

    /// Whether asking `calendar` at `at` would ask its publisher, rather than
    /// answer that it was asked within `refetch`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn due(
        &self,
        calendar: Calendar,
        at: Timestamp,
        refetch: Refetch,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;
}

impl<S, A, B> KeptCalendars for Calendars<S, A, B>
where
    S: CalendarStore + Sync,
    A: CalendarPublisher + Sync,
    B: CalendarPublisher + Sync,
{
    async fn ask(
        &self,
        calendar: Calendar,
        at: Timestamp,
        refetch: Refetch,
    ) -> Result<Result<Found, StepFailure>, StoreError> {
        let kept = self.store.kept(calendar).await?;
        if let (Refetch::After(within), Some(kept)) = (refetch, &kept)
            && asked_within(kept.asked, at, within)
        {
            return Ok(Ok(Found::NotDue(NotDue::Fetched { within })));
        }
        let since = kept.as_ref().map(|kept| &kept.served);
        let served = match calendar {
            Calendar::School => self.school.served_since(since).await,
            Calendar::Public => self.public.served_since(since).await,
        };
        Ok(match served {
            Err(error) => Err(StepFailure::Source(error)),
            Ok(None) => {
                self.store.unchanged(calendar, at).await?;
                Ok(Found::Unchanged)
            }
            Ok(Some(served)) => {
                self.store.keep(calendar, &served, at).await?;
                Ok(Found::Changed)
            }
        })
    }

    async fn due(
        &self,
        calendar: Calendar,
        at: Timestamp,
        refetch: Refetch,
    ) -> Result<bool, StoreError> {
        let kept = self.store.kept(calendar).await?;
        Ok(match (refetch, kept) {
            (Refetch::After(within), Some(kept)) => !asked_within(kept.asked, at, within),
            (Refetch::WhenDue, _) | (_, None) => true,
        })
    }
}

/// Whether a calendar last asked at `asked` was asked within `within` of `at`.
fn asked_within(asked: Timestamp, at: Timestamp, within: std::time::Duration) -> bool {
    at.duration_since(asked).unsigned_abs() < within
}

/// What is kept, read: the store says what the holidays are.
impl<S: HolidayCalendar + Sync, A: Sync, B: Sync> HolidayCalendar for Calendars<S, A, B> {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        self.store.holidays().await
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Mutex, time::Duration};

    use jiff::Timestamp;

    use super::{Calendars, KeptCalendars as _};
    use crate::{
        SourceError, StoreError,
        graph::Calendar,
        next::{Found, NotDue, Refetch},
        ports::{CalendarPublisher, CalendarStore, Kept, Served},
    };

    #[derive(Default)]
    struct Store(Mutex<Vec<(Calendar, Kept)>>);

    impl CalendarStore for Store {
        async fn kept(&self, calendar: Calendar) -> Result<Option<Kept>, StoreError> {
            Ok(self
                .0
                .lock()
                .expect("a lock")
                .iter()
                .find(|(kept, _)| *kept == calendar)
                .map(|(_, kept)| kept.clone()))
        }

        async fn keep(
            &self,
            calendar: Calendar,
            served: &Served,
            asked: Timestamp,
        ) -> Result<(), StoreError> {
            {
                let mut kept = self.0.lock().expect("a lock");
                kept.retain(|(kept, _)| *kept != calendar);
                kept.push((
                    calendar,
                    Kept {
                        served: served.clone(),
                        asked,
                    },
                ));
            }
            Ok(())
        }

        async fn unchanged(&self, calendar: Calendar, asked: Timestamp) -> Result<(), StoreError> {
            for (kept, held) in self.0.lock().expect("a lock").iter_mut() {
                if *kept == calendar {
                    held.asked = asked;
                }
            }
            Ok(())
        }
    }

    /// A publisher that serves `body` with an `ETag` of it, and answers
    /// unchanged to a question asked with that tag.
    struct Publisher(&'static str);

    impl CalendarPublisher for Publisher {
        async fn served_since(&self, kept: Option<&Served>) -> Result<Option<Served>, SourceError> {
            Ok(
                (kept.and_then(|kept| kept.etag.as_deref()) != Some(self.0)).then(|| Served {
                    body: self.0.to_owned(),
                    etag: Some(self.0.to_owned()),
                    last_modified: None,
                }),
            )
        }
    }

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).expect("a moment")
    }

    /// **Kept the first time, unchanged the next, and not asked within the
    /// window**: what a run, a second run and a page's run straight after find.
    #[test]
    fn a_calendar_is_kept_then_asked_whether_it_changed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let calendars = Calendars::new(Store::default(), Publisher("v1"), Publisher("p1"));
            let window = Refetch::After(Duration::from_mins(45));
            let asked = |seconds, refetch| calendars.ask(Calendar::School, at(seconds), refetch);

            assert_eq!(
                asked(0, Refetch::WhenDue).await.expect("a store"),
                Ok(Found::Changed)
            );
            assert_eq!(
                asked(60, Refetch::WhenDue).await.expect("a store"),
                Ok(Found::Unchanged)
            );
            assert_eq!(
                asked(120, window).await.expect("a store"),
                Ok(Found::NotDue(NotDue::Fetched {
                    within: Duration::from_mins(45)
                }))
            );
            assert_eq!(
                asked(60 + 45 * 60, window).await.expect("a store"),
                Ok(Found::Unchanged)
            );
        });
    }
}

//! Body composition, one series per method (#256).
//!
//! **The series are `domain`'s** ([`by_method`]); this reads the figures and
//! hands them over. Nothing is written: every series re-derives from them
//! exactly (§ 5).
//!
//! **Against the nutrition phase in force** (#489), the cells are `domain`'s
//! ([`read`]); this finds the macrocycle and the plans' mesocycles.

use domain::{
    analytical::{
        composition::{ComponentSeries, by_method},
        recomposition::{Recomposition, read},
    },
    macrocycle::Macrocycle,
    schedule::Holidays,
};
use jiff::civil::Date;

use crate::{
    error::{SourceError, StoreError},
    ports::{CompositionHistory, PlanStore},
};

/// The composition use case.
pub struct Composition<H> {
    history: H,
}

impl<H> Composition<H>
where
    H: CompositionHistory + Sync,
{
    pub const fn new(history: H) -> Self {
        Self { history }
    }

    /// Every component anything measured, one series per method.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn by_method(&self) -> Result<Vec<ComponentSeries>, StoreError> {
        Ok(by_method(&self.history.measured().await?))
    }
}

/// Body composition against the nutrition phase in force, on a day (#489).
pub enum AgainstPhase {
    /// The school calendar could not be read, so there is no macrocycle.
    Unreadable(SourceError),
    /// `today` falls past the calendar's reach.
    NoMacrocycle,
    Read(Recomposition),
}

/// The use case for [`AgainstPhase`].
pub struct InForce<H, P> {
    history: H,
    plans: P,
}

impl<H, P> InForce<H, P>
where
    H: CompositionHistory + Sync,
    P: PlanStore + Sync,
{
    pub const fn new(history: H, plans: P) -> Self {
        Self { history, plans }
    }

    /// The current macrocycle's periods as they stand on `today`. Every
    /// plan's mesocycles are read, since the competitive one starts
    /// maintenance.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn on(
        &self,
        today: Date,
        holidays: Result<Holidays, SourceError>,
    ) -> Result<AgainstPhase, StoreError> {
        let holidays = match holidays {
            Ok(holidays) => holidays,
            Err(error) => return Ok(AgainstPhase::Unreadable(error)),
        };
        let Some(macrocycle) = Macrocycle::on(today, &holidays) else {
            return Ok(AgainstPhase::NoMacrocycle);
        };
        let mut mesocycles = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await? {
                mesocycles.extend(plan.mesocycle_spans());
            }
        }
        let measured = self.history.measured().await?;
        Ok(AgainstPhase::Read(read(
            &measured,
            macrocycle,
            &mesocycles,
            today,
        )))
    }
}

//! Body composition, one series per method (#256).
//!
//! **The series are `domain`'s** ([`by_method`]); this reads the figures and
//! hands them over. Nothing is written: every series re-derives from them
//! exactly (§ 5).

use domain::analytical::composition::{ComponentSeries, by_method};

use crate::{error::StoreError, ports::CompositionHistory};

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

//! Body weight as one time series (#250).
//!
//! **The ordering and the trend are `domain`'s** ([`Series`]); this reads the canonical
//! weigh-ins and hands them over. Nothing is written: the series re-derives
//! from them exactly (§ 5).

use domain::analytical::body_weight::Series;

use crate::{error::StoreError, ports::WeighInHistory};

/// The body-weight use case.
pub struct BodyWeight<W> {
    weigh_ins: W,
}

impl<W> BodyWeight<W>
where
    W: WeighInHistory + Sync,
{
    pub const fn new(weigh_ins: W) -> Self {
        Self { weigh_ins }
    }

    /// Every canonical weigh-in, oldest first, and the trend through them.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn series(&self) -> Result<Series, StoreError> {
        Ok(Series::of(self.weigh_ins.weigh_ins().await?))
    }
}

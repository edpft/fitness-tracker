//! Body weight as one time series (#250).
//!
//! **The ordering is `domain`'s** ([`body_weight`]); this reads the canonical
//! weigh-ins and hands them over. Nothing is written: the series re-derives
//! from them exactly (§ 5).

use domain::analytical::{Weighed, body_weight::body_weight};

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

    /// Every canonical weigh-in, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn series(&self) -> Result<Vec<Weighed>, StoreError> {
        Ok(body_weight(self.weigh_ins.weigh_ins().await?))
    }
}

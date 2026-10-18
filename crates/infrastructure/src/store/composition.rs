//! Body composition, read back for the analytical layer (#256).
//!
//! **Every source's figures, beside the mass the same step weighed**, and the
//! method that measured each: the Body Scan's algorithm, or the spreadsheets.
//! What a series is and how a figure is charted is `domain`'s
//! ([`domain::analytical::composition`]).
//!
//! **A figure stated in litres is not read yet.** Nothing has stated one; InBody
//! will (#266), and how its water is charted is that issue's.

use application::{CompositionHistory, StoreError};
use domain::{
    analytical::composition::{Measured, Method, Stated},
    body::{Component, Percentage},
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use super::{SpreadsheetFileLandingStore, WithingsMeasurementLandingStore, corrupt, store_error};

/// Every composition figure in the store.
#[derive(Debug, Clone)]
pub struct SqliteCompositionHistory {
    pool: SqlitePool,
}

impl SqliteCompositionHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CompositionHistory for SqliteCompositionHistory {
    async fn measured(&self) -> Result<Vec<Measured>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT w.stream AS "stream!: String",
                   w.on_day AS "on_day: String",
                   w.measured_at_utc AS "measured_at_utc: String",
                   w.zone AS "zone: String",
                   w.mass_grams AS "weighed!: i64",
                   c.component AS "component!: String",
                   c.mass_grams AS "mass_grams: i64",
                   c.percentage_tenths AS "percentage_tenths: i64",
                   p.algorithm AS "algorithm?: i64"
            FROM body_composition c
            JOIN weigh_in w ON w.id = c.weigh_in
            LEFT JOIN body_scan_weigh_in b ON b.weigh_in = w.id
            LEFT JOIN body_scan_part p
                ON p.weigh_in = b.landing_record_id AND p.part = 'composition'
            WHERE c.volume_millilitres IS NULL
            ORDER BY COALESCE(w.measured_at_utc, w.on_day), w.id, c.component
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                let method = if row.stream == WithingsMeasurementLandingStore::STREAM {
                    let algorithm = row.algorithm.ok_or_else(|| StoreError::Corrupt {
                        detail: "a Body Scan's composition names no algorithm".to_owned(),
                    })?;
                    Method::BodyScan {
                        algorithm: u64::try_from(algorithm).map_err(|error| corrupt(&error))?,
                    }
                } else if row.stream == SpreadsheetFileLandingStore::STREAM {
                    Method::Spreadsheets
                } else {
                    return Err(StoreError::Corrupt {
                        detail: format!("{} states no composition", row.stream),
                    });
                };
                let component =
                    Component::from_key(&row.component).ok_or_else(|| StoreError::Corrupt {
                        detail: format!("{:?} is not a component", row.component),
                    })?;
                let stated = match (row.mass_grams, row.percentage_tenths) {
                    (Some(grams), None) => Stated::Mass(mass(grams)?),
                    (None, Some(tenths)) => Stated::Percentage(
                        u16::try_from(tenths)
                            .map_err(|error| corrupt(&error))
                            .and_then(|tenths| {
                                Percentage::from_tenths(tenths).map_err(|error| corrupt(&error))
                            })?,
                    ),
                    _ => {
                        return Err(StoreError::Corrupt {
                            detail: "a composition figure is neither a mass nor a percentage"
                                .to_owned(),
                        });
                    }
                };
                Ok(Measured {
                    on: day(
                        row.on_day.as_deref(),
                        row.measured_at_utc.as_deref(),
                        row.zone.as_deref(),
                    )?,
                    method,
                    component,
                    stated,
                    weighed: mass(row.weighed)?,
                })
            })
            .collect()
    }
}

/// The day a weigh-in fell on, by the zone it was weighed in.
fn day(on_day: Option<&str>, at: Option<&str>, zone: Option<&str>) -> Result<Date, StoreError> {
    match (on_day, at, zone) {
        (Some(on_day), None, None) => on_day.parse().map_err(|error| corrupt(&error)),
        (None, Some(at), Some(zone)) => {
            let zone = OperatorZone::try_from(zone.to_owned()).map_err(|error| corrupt(&error))?;
            let at: Timestamp = at.parse().map_err(|error| corrupt(&error))?;
            Ok(StartedAt::new(at, zone).wall_clock().date())
        }
        _ => Err(StoreError::Corrupt {
            detail: "a weigh-in is neither at an instant nor on a day".to_owned(),
        }),
    }
}

fn mass(grams: i64) -> Result<Kg, StoreError> {
    Ok(Kg::from_grams(
        u64::try_from(grams).map_err(|error| corrupt(&error))?,
    ))
}

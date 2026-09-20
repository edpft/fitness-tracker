//! Which Peloton classes can fill a holding week's two roles.
//!
//! **A mapping table in the adapter, keyed on the vocabulary `domain` owns** —
//! the same shape as [`super::mapping`], and for the same reason. A series id
//! is Peloton's identifier (§ II.3); what crosses the port is a
//! [`SessionRole`] and a [`RideVenue`], neither of which knows Peloton exists.
//!
//! **The class type cannot do this job.** *Power Zone Endurance Ride*, *Power
//! Zone Ride* and *Power Zone Max Ride* share one `class_type_id`, verified
//! against the operator's 309 landed cycling workouts on 2026-09-20. The series
//! is what separates them, and it is Peloton's own statement that two classes
//! are the same kind of thing rather than our reading of their titles.
//!
//! **Forty-five minutes is one fixed decision, not a knob** (§ 14.1). The
//! operator named it for both rides on 2026-09-20, and it is here beside the
//! series because the browse endpoint takes an exact duration — the two are one
//! query, not a shape and a parameter.

use application::{HoldingRides, SourceError};
use domain::{
    cycling::RideVenue,
    schedule::{Relative, SessionRole},
};

use super::class::{POWER_ZONE_ENDURANCE_SERIES, POWER_ZONE_SERIES, PelotonClasses};

/// How long both holding rides are, in seconds.
///
/// Forty-five minutes. **Equal on purpose**, which is why the role comparison
/// admits equality (#180): the higher-intensity session's duration is no more
/// than the other's rather than strictly less.
const HOLDING_SECONDS: u64 = 2_700;

/// Which series fills a role, if any does.
///
/// `None` for the two roles a holding week does not ask for. Four roles are
/// representable and the planner asks for two (`domain::schedule::role`), so
/// this answers for the two and refuses to invent the others.
const fn series_for(role: SessionRole) -> Option<&'static str> {
    match (role.intensity(), role.volume()) {
        (Relative::Higher, Relative::Lower) => Some(POWER_ZONE_SERIES),
        (Relative::Lower, Relative::Higher) => Some(POWER_ZONE_ENDURANCE_SERIES),
        _ => None,
    }
}

/// Peloton, as the catalogue a holding week is built from.
#[derive(Debug, Clone)]
pub struct PelotonHoldingRides<'a> {
    classes: &'a PelotonClasses,
}

impl<'a> PelotonHoldingRides<'a> {
    pub const fn new(classes: &'a PelotonClasses) -> Self {
        Self { classes }
    }
}

impl HoldingRides for PelotonHoldingRides<'_> {
    async fn candidates(&self, role: SessionRole) -> Result<Vec<RideVenue>, SourceError> {
        let Some(series) = series_for(role) else {
            return Ok(Vec::new());
        };
        let found = self
            .classes
            .newest_in_series(series, HOLDING_SECONDS)
            .await?;
        found
            .into_iter()
            .map(|class| {
                RideVenue::new(&class.id, &class.title).map_err(|error| SourceError::Malformed {
                    detail: format!("a listed class could not be named: {error}"),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_holding_roles_take_different_series() {
        let higher = series_for(SessionRole::new(Relative::Higher, Relative::Lower));
        let lower = series_for(SessionRole::new(Relative::Lower, Relative::Higher));
        assert_eq!(higher, Some(POWER_ZONE_SERIES));
        assert_eq!(lower, Some(POWER_ZONE_ENDURANCE_SERIES));
        assert_ne!(higher, lower);
    }

    #[test]
    fn a_role_a_holding_week_does_not_ask_for_has_no_series() {
        assert_eq!(
            series_for(SessionRole::new(Relative::Higher, Relative::Higher)),
            None
        );
        assert_eq!(
            series_for(SessionRole::new(Relative::Lower, Relative::Lower)),
            None
        );
    }

    /// The reason this module exists rather than a class-type filter.
    #[test]
    fn the_two_series_are_not_the_class_type_they_share() {
        assert_ne!(
            POWER_ZONE_SERIES,
            super::super::class::POWER_ZONE_CLASS_TYPE
        );
        assert_ne!(
            POWER_ZONE_ENDURANCE_SERIES,
            super::super::class::POWER_ZONE_CLASS_TYPE
        );
    }
}

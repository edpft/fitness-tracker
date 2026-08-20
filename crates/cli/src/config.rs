//! What the operator supplies, and where it comes from.
//!
//! Nothing is compiled in: the database path, the base URL and the credential
//! are all configuration, so moving between machines needs no code change.
//! Nor is anything named after one source — the variables come from
//! [`crate::catalogue`], which derives them from whichever source an
//! invocation names.

use std::{env::VarError, path::PathBuf};

use domain::{gym::OperatorZone, prescription::Calendar};
use jiff::{Timestamp, civil::Date};

use crate::catalogue::KnownStream;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "{variable} is not set. Get a key from {credential_url} and put it in the environment \
         or an untracked .env — never on the command line"
    )]
    MissingApiKey {
        variable: String,
        credential_url: &'static str,
    },

    #[error("{variable} is set but is not valid text")]
    UnreadableApiKey { variable: String },

    #[error("no database path: pass --database or set FITNESS_TRACKER_DATABASE")]
    MissingDatabase,

    #[error(
        "no time zone: pass --timezone or set FITNESS_TRACKER_TIMEZONE to an IANA identifier \
         such as Europe/London. Nothing is compiled in, because a default would be an \
         assumption about where you train — silently right here and silently wrong elsewhere"
    )]
    MissingTimeZone,

    #[error("{value:?} is not an IANA time zone identifier")]
    UnknownTimeZone { value: String },

    #[error("{value:?} is not a date: {detail}")]
    NotADate { value: String, detail: String },

    #[error("this programme has no session on or after {from}")]
    NoSessionScheduled { from: Date },
}

/// The zone the operator declares they train in.
///
/// § II.3 takes it from configuration, and § 34 forbids an environment
/// assumption — so there is no default. A compiled-in `Europe/London` would be
/// correct for this account and wrong for the next, and because it would be
/// correct here no test would ever catch it.
///
/// The value is passed in rather than read here, so this is testable without
/// touching the process environment. Which of the flag and the variable it came
/// from is clap's business.
///
/// # Errors
///
/// [`ConfigError`] if it is unset or is not an identifier the database knows.
pub fn timezone(declared: Option<&str>) -> Result<OperatorZone, ConfigError> {
    let value = match declared {
        Some(value) if !value.trim().is_empty() => value.to_owned(),
        _ => return Err(ConfigError::MissingTimeZone),
    };

    OperatorZone::try_from(value.as_str()).map_err(|_| ConfigError::UnknownTimeZone { value })
}

/// What it takes to reach a source.
///
/// Resolved only for commands that contact one. `status` and `reset` must keep
/// working with no credential and no network: a staleness report that is
/// itself unavailable whenever things go wrong reports nothing worth having.
#[derive(Debug, Clone)]
pub struct SourceAccess {
    pub base_url: String,
    pub api_key: String,
}

impl SourceAccess {
    /// The credential is passed in rather than read here so that this is
    /// testable without touching the process environment.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] if the credential is absent or unreadable.
    pub fn resolve(
        known: &KnownStream,
        base_url: String,
        api_key: Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let api_key = match api_key {
            Ok(key) if !key.trim().is_empty() => key,
            Ok(_) | Err(VarError::NotPresent) => {
                return Err(ConfigError::MissingApiKey {
                    variable: known.api_key_variable(),
                    credential_url: known.credential_url(),
                });
            }
            Err(VarError::NotUnicode(_)) => {
                return Err(ConfigError::UnreadableApiKey {
                    variable: known.api_key_variable(),
                });
            }
        };

        Ok(Self { base_url, api_key })
    }
}

/// # Errors
///
/// [`ConfigError::MissingDatabase`] if no path was given. There is no default:
/// a store that appears wherever the command was run is worse than one that
/// has to be named.
pub fn database(path: Option<PathBuf>) -> Result<PathBuf, ConfigError> {
    path.ok_or(ConfigError::MissingDatabase)
}

/// Which date to prescribe for: the one given, or the next session.
///
/// **Defaults forward rather than to today.** "The next session" is what an
/// operator wants on a rest day and today is what they want on a training day,
/// and the next programmed day at or after today gives both. It is printed, so
/// the default is never silent.
///
/// **Composed here rather than at the call site, and `now` is a parameter.** The
/// default is three decisions stacked — resolve the instant in the operator's
/// zone, take the day that lands on, then walk forward to a day the programme
/// runs and does not skip — and each of them is silently right on the machine
/// that wrote it. A stub for the store cannot catch any of them, so the whole
/// composition is a pure function with the clock passed in.
///
/// # Errors
///
/// [`ConfigError::NotADate`] for a `--date` that will not parse, and
/// [`ConfigError::NoSessionScheduled`] where the block has no remaining session
/// — which is what asking after the last week looks like.
pub fn date(given: Option<&str>, calendar: &Calendar, now: Timestamp) -> Result<Date, ConfigError> {
    if let Some(text) = given {
        return text.parse::<Date>().map_err(|error| ConfigError::NotADate {
            value: text.to_owned(),
            detail: error.to_string(),
        });
    }

    let today = calendar.today(now);
    calendar
        .next_programmed(today)
        .ok_or(ConfigError::NoSessionScheduled { from: today })
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, SourceAccess, database, date};
    use crate::catalogue::{KnownStream, lookup};
    use domain::prescription::{Calendar, SessionRole, Weekdays};
    use jiff::{Timestamp, civil::Weekday};
    use std::{env::VarError, path::PathBuf};

    fn hevy() -> Option<&'static KnownStream> {
        lookup("hevy.workouts")
    }

    /// A Monday-and-Friday block of four weeks from Monday 2026-09-07, skipping
    /// the week of the 21st, in a zone ten hours ahead of UTC.
    ///
    /// The zone is deliberately not the operator's: a test in `Europe/London`
    /// agrees with UTC for half the year, which is exactly how a default that
    /// resolves the day in the wrong zone survives a test suite.
    fn calendar(interruptions: &[&str]) -> Result<Calendar, Box<dyn std::error::Error>> {
        let weekdays = Weekdays::new(vec![
            (Weekday::Monday, SessionRole::Light),
            (Weekday::Friday, SessionRole::Heavy),
        ])?;
        let skipped: Vec<_> = interruptions
            .iter()
            .map(|week| week.parse())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Calendar::new(
            "2026-09-07".parse()?,
            4,
            &skipped,
            weekdays,
            jiff::tz::TimeZone::get("Pacific/Auckland")?,
        )?)
    }

    #[test]
    fn a_blank_credential_is_treated_as_missing() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let resolved = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("   ".to_owned()),
        );
        assert_eq!(
            resolved.unwrap_err(),
            ConfigError::MissingApiKey {
                variable: "HEVY_API_KEY".to_owned(),
                credential_url: known.credential_url(),
            }
        );
    }

    #[test]
    fn the_database_path_is_required() {
        assert_eq!(database(None).unwrap_err(), ConfigError::MissingDatabase);
        assert_eq!(
            database(Some(PathBuf::from("/tmp/x.db"))).unwrap(),
            PathBuf::from("/tmp/x.db")
        );
    }

    /// An explicit `--date` is taken as given, and the clock is not consulted.
    #[test]
    fn a_given_date_is_the_date() {
        let calendar = calendar(&[]).expect("a calendar");
        let resolved = date(Some("2026-09-11"), &calendar, Timestamp::UNIX_EPOCH);
        assert_eq!(resolved.expect("a date").to_string(), "2026-09-11");
    }

    /// On a training day the default is that day, not the next one.
    #[test]
    fn the_default_is_today_when_today_is_programmed() {
        let calendar = calendar(&[]).expect("a calendar");
        // Monday 2026-09-07, mid-morning in Auckland.
        let now: Timestamp = "2026-09-06T22:00:00Z".parse().expect("an instant");
        let resolved = date(None, &calendar, now);
        assert_eq!(resolved.expect("a date").to_string(), "2026-09-07");
    }

    /// On a rest day it is the next programmed day.
    #[test]
    fn the_default_walks_forward_to_the_next_programmed_day() {
        let calendar = calendar(&[]).expect("a calendar");
        // Wednesday 2026-09-09 in Auckland; the programme runs Monday and Friday.
        let now: Timestamp = "2026-09-08T22:00:00Z".parse().expect("an instant");
        let resolved = date(None, &calendar, now);
        assert_eq!(resolved.expect("a date").to_string(), "2026-09-11");
    }

    /// **The day is the operator's, not UTC's.**
    ///
    /// This instant is Thursday evening in UTC and Friday morning in Auckland,
    /// and Friday is a training day. Resolving in UTC would default to a rest day
    /// and then walk forward to the *following* Monday — a whole session skipped,
    /// on exactly one day in seven, which is why this case is pinned rather than
    /// left to the zone the machine happens to be in.
    #[test]
    fn the_default_resolves_the_day_in_the_operators_zone() {
        let calendar = calendar(&[]).expect("a calendar");
        let now: Timestamp = "2026-09-10T21:00:00Z".parse().expect("an instant");
        assert_eq!(
            date(None, &calendar, now).expect("a date").to_string(),
            "2026-09-11",
            "Friday in Auckland is a training day"
        );
    }

    /// A week the block skips is not a session, so the default steps over it.
    #[test]
    fn the_default_steps_over_an_interrupted_week() {
        let calendar = calendar(&["2026-09-21"]).expect("a calendar");
        // Saturday 2026-09-19 in Auckland: the next programmed weekday is Monday
        // the 21st, which falls in the skipped week.
        let now: Timestamp = "2026-09-18T22:00:00Z".parse().expect("an instant");
        let resolved = date(None, &calendar, now);
        assert_eq!(
            resolved.expect("a date").to_string(),
            "2026-09-28",
            "a holiday is not a rest day with a session after it"
        );
    }

    /// Past the last week there is nothing to default to, and saying so beats
    /// answering with a date outside the block.
    #[test]
    fn the_default_refuses_once_the_block_is_over() {
        let calendar = calendar(&[]).expect("a calendar");
        let now: Timestamp = "2026-12-24T22:00:00Z".parse().expect("an instant");
        let from = "2026-12-25".parse().expect("a date");
        assert_eq!(
            date(None, &calendar, now).unwrap_err(),
            ConfigError::NoSessionScheduled { from }
        );
    }

    /// The message names the variable the invocation actually needs, which is
    /// derived from the source rather than compiled in.
    #[test]
    fn a_missing_variable_names_itself() {
        let resolved = SourceAccess::resolve(
            hevy().expect("hevy.workouts is in the catalogue"),
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
        );
        let message = resolved.unwrap_err().to_string();
        assert!(message.contains("HEVY_API_KEY"), "{message}");
        assert!(message.contains("hevy.com/settings"), "{message}");
        assert!(message.contains("never on the command line"), "{message}");
    }
}

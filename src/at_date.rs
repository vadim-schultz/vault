//! `AtDate` — a validated point in time for time-travel queries.
//!
//! Three accepted textual forms, each with its own constructor: `YYYY-MM-DD` (start of day,
//! UTC), `YYYY-MM-DD HH:MM` (local time), and full RFC3339 (exact — what `vault log` prints, so
//! its output round-trips back into `--at`/`--to`). Internally always stored as a UTC RFC3339
//! string so plain string comparison against `MetaIndex::resolve_at`'s `created_at` column stays
//! chronologically correct — every producer of that column must also normalize to UTC.

use std::str::FromStr;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::error::VaultError;

const DATE_FMT: &str = "%Y-%m-%d";
const DATE_TIME_FMT: &str = "%Y-%m-%d %H:%M";

/// A CLI timestamp argument, resolved to a UTC RFC3339 string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtDate(String);

impl AtDate {
    /// Return the resolved UTC RFC3339 string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Try each accepted format in turn: calendar date, local date-time, then RFC3339.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidDate`] when `input` matches none of them.
    pub fn parse(input: &str) -> Result<Self, VaultError> {
        Self::from_calendar_date(input)
            .or_else(|_| Self::from_local_date_time(input))
            .or_else(|_| Self::from_rfc3339(input))
            .map_err(|_| VaultError::InvalidDate {
                input: input.to_string(),
            })
    }

    /// Parse `YYYY-MM-DD` as UTC midnight.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidDate`] when `input` isn't a valid calendar date.
    ///
    /// # Panics
    ///
    /// Never: midnight is a valid time on every calendar date.
    pub fn from_calendar_date(input: &str) -> Result<Self, VaultError> {
        let date =
            NaiveDate::parse_from_str(input, DATE_FMT).map_err(|_| VaultError::InvalidDate {
                input: input.to_string(),
            })?;
        let midnight = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
        Ok(Self(Utc.from_utc_datetime(&midnight).to_rfc3339()))
    }

    /// Parse `YYYY-MM-DD HH:MM` as local time, converted to UTC.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidDate`] when `input` doesn't match the format or names an
    /// ambiguous/nonexistent local time (DST transition).
    pub fn from_local_date_time(input: &str) -> Result<Self, VaultError> {
        let naive = NaiveDateTime::parse_from_str(input, DATE_TIME_FMT).map_err(|_| {
            VaultError::InvalidDate {
                input: input.to_string(),
            }
        })?;
        let local =
            Local
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| VaultError::InvalidDate {
                    input: input.to_string(),
                })?;
        Ok(Self(local.with_timezone(&Utc).to_rfc3339()))
    }

    /// Parse an exact RFC3339 timestamp (any offset, normalized to UTC on output).
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidDate`] when `input` isn't valid RFC3339.
    pub fn from_rfc3339(input: &str) -> Result<Self, VaultError> {
        let exact = DateTime::parse_from_rfc3339(input).map_err(|_| VaultError::InvalidDate {
            input: input.to_string(),
        })?;
        Ok(Self(exact.with_timezone(&Utc).to_rfc3339()))
    }
}

impl FromStr for AtDate {
    type Err = VaultError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_calendar_date_is_utc_midnight() {
        assert_eq!(
            AtDate::from_calendar_date("2026-06-01").unwrap().as_str(),
            "2026-06-01T00:00:00+00:00"
        );
    }

    #[test]
    fn from_local_date_time_converts_host_timezone_to_utc() {
        let naive = NaiveDateTime::parse_from_str("2026-06-01 23:58", DATE_TIME_FMT).unwrap();
        let expected = Local
            .from_local_datetime(&naive)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        assert_eq!(
            AtDate::from_local_date_time("2026-06-01 23:58")
                .unwrap()
                .as_str(),
            expected
        );
    }

    #[test]
    fn from_rfc3339_round_trips_utc_input() {
        let input = "2026-06-01T14:32:01+00:00";
        assert_eq!(AtDate::from_rfc3339(input).unwrap().as_str(), input);
    }

    #[test]
    fn parse_accepts_all_three_formats() {
        assert!(AtDate::parse("2026-06-01").is_ok());
        assert!(AtDate::parse("2026-06-01 23:58").is_ok());
        assert!(AtDate::parse("2026-06-01T14:32:01+00:00").is_ok());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(matches!(
            AtDate::parse("not-a-date"),
            Err(VaultError::InvalidDate { .. })
        ));
    }

    #[test]
    fn from_str_delegates_to_parse() {
        assert_eq!(
            "2026-06-01".parse::<AtDate>().unwrap(),
            AtDate::from_calendar_date("2026-06-01").unwrap()
        );
    }
}

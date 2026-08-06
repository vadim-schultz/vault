//! `AtDate` — a validated point in time for time-travel queries.
//!
//! Three accepted textual forms, each with its own constructor: `YYYY-MM-DD` (end of day, local
//! timezone — inclusive of that day's activity), `YYYY-MM-DD HH:MM` (local time), and full
//! RFC3339 (exact — what `vault log` prints, so its output round-trips back into `--at`/`--to`).
//! Internally always stored as a UTC RFC3339 string so plain string comparison against
//! `MetaIndex::resolve_at`'s `created_at` column stays chronologically correct — every producer
//! of that column must also normalize to UTC.

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

    /// Parse `YYYY-MM-DD` as the end of that day (23:59:59.999999999) in the host's local
    /// timezone, converted to UTC.
    ///
    /// End of day (not start) so a bare date reads as inclusive of that day's activity — "show me
    /// X as of this date" naturally means as of when the date finished, not when it began. Local
    /// (not UTC) so it agrees with the `HH:MM` form's timezone basis and resolves correctly
    /// regardless of the host's UTC offset.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidDate`] when `input` isn't a valid calendar date.
    ///
    /// # Panics
    ///
    /// Never: 23:59:59.999999999 is a valid time on every calendar date.
    pub fn from_calendar_date(input: &str) -> Result<Self, VaultError> {
        let date =
            NaiveDate::parse_from_str(input, DATE_FMT).map_err(|_| VaultError::InvalidDate {
                input: input.to_string(),
            })?;
        let end_of_day = date
            .and_hms_nano_opt(23, 59, 59, 999_999_999)
            .expect("23:59:59.999999999 is always valid");
        // `.latest()` rather than `.single()`/erroring on a DST-ambiguous instant: this is
        // already an approximate day-granularity boundary, not an exact instant a user typed, so
        // picking either valid interpretation is fine — unlike `from_local_date_time`, where the
        // user named a specific wall-clock time and an ambiguous match should be reported.
        let local = Local
            .from_local_datetime(&end_of_day)
            .latest()
            .ok_or_else(|| VaultError::InvalidDate {
                input: input.to_string(),
            })?;
        Ok(Self(local.with_timezone(&Utc).to_rfc3339()))
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
    fn from_calendar_date_is_end_of_local_day() {
        let naive = NaiveDate::parse_from_str("2026-06-01", DATE_FMT)
            .unwrap()
            .and_hms_nano_opt(23, 59, 59, 999_999_999)
            .unwrap();
        let expected = Local
            .from_local_datetime(&naive)
            .latest()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        assert_eq!(
            AtDate::from_calendar_date("2026-06-01").unwrap().as_str(),
            expected
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

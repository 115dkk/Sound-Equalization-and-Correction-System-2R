//! Wall-clock readings for folder names. The user's time zone is an
//! operating-system question, so the app asks `secs-sys` for the UTC offset
//! and the service only does calendar arithmetic.

use std::time::{SystemTime, UNIX_EPOCH};

/// `time` in UTC as chrono's `DateTime::<Utc>::to_rfc3339()` wrote it:
/// `2026-11-19T01:02:03.123456700+00:00`. The fraction is omitted when the
/// nanoseconds are zero, and otherwise has 3, 6 or 9 digits, the fewest that
/// lose nothing.
pub fn utc_rfc3339(time: SystemTime) -> String {
    let (seconds, nanoseconds) = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (i128::from(after.as_secs()), after.subsec_nanos()),
        Err(before) => {
            let before = before.duration();
            let seconds = -i128::from(before.as_secs());
            if before.subsec_nanos() == 0 {
                (seconds, 0)
            } else {
                (seconds - 1, 1_000_000_000 - before.subsec_nanos())
            }
        }
    };
    format_utc(seconds, nanoseconds, false, false)
}

/// `unix_ms` in UTC as `2026-11-19T01:02:03.123Z`; `None` outside years
/// 0000..=9999, which RFC 3339 cannot write.
pub(crate) fn utc_rfc3339_millis(unix_ms: i64) -> Option<String> {
    let seconds = unix_ms.div_euclid(1_000);
    let nanoseconds = (unix_ms.rem_euclid(1_000) * 1_000_000) as u32;
    let text = format_utc(i128::from(seconds), nanoseconds, true, true);
    let days = seconds.div_euclid(86_400);
    let (year, _, _) = civil_from_days(days);
    (0..=9999).contains(&year).then_some(text)
}

fn format_utc(seconds: i128, nanoseconds: u32, use_z: bool, fixed_millis: bool) -> String {
    // `seconds` comes from a `SystemTime` duration (u64 seconds at most) or
    // an i64 millisecond count, so its day count always fits i64.
    let days = seconds.div_euclid(86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let of_day = seconds.rem_euclid(86_400);
    let hour = of_day / 3_600;
    let minute = of_day % 3_600 / 60;
    let second = of_day % 60;
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+05}")
    };
    let fraction = if fixed_millis {
        format!(".{:03}", nanoseconds / 1_000_000)
    } else if nanoseconds == 0 {
        String::new()
    } else if nanoseconds.is_multiple_of(1_000_000) {
        format!(".{:03}", nanoseconds / 1_000_000)
    } else if nanoseconds.is_multiple_of(1_000) {
        format!(".{:06}", nanoseconds / 1_000)
    } else {
        format!(".{nanoseconds:09}")
    };
    let offset = if use_z { "Z" } else { "+00:00" };
    format!("{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}{offset}")
}

/// A wall-clock reading in the user's time zone, as upstream's
/// `datetime.now()` gives it for export folder names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i64,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl LocalTime {
    /// `time` in a zone `offset_seconds` east of UTC (whole seconds).
    pub fn from_system(time: SystemTime, offset_seconds: i32) -> Self {
        let unix = match time.duration_since(UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
            Err(before) => {
                let before = before.duration();
                let whole = i64::try_from(before.as_secs()).unwrap_or(i64::MAX);
                // 1.5 s before the epoch is second -2, as floor division gives.
                -whole - i64::from(before.subsec_nanos() > 0)
            }
        };
        let local = unix.saturating_add(i64::from(offset_seconds));
        let (year, month, day) = civil_from_days(local.div_euclid(86_400));
        let of_day = local.rem_euclid(86_400);
        let part = |value: i64| u8::try_from(value).expect("time of day fits u8");
        Self {
            year,
            month,
            day,
            hour: part(of_day / 3600),
            minute: part(of_day % 3600 / 60),
            second: part(of_day % 60),
        }
    }

    /// `2024-01-02`, the date part of the default export folder.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `2024-01-02 03:04`, the stamp embedded in filter text.
    pub fn minute_stamp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }

    /// Returns this reading shifted by a signed number of seconds.
    pub fn shift_seconds(&self, seconds: i32) -> Self {
        let days = days_from_civil(self.year, self.month, self.day);
        let of_day =
            i64::from(self.hour) * 3600 + i64::from(self.minute) * 60 + i64::from(self.second);
        let shifted = days
            .saturating_mul(86_400)
            .saturating_add(of_day)
            .saturating_add(i64::from(seconds));
        let (year, month, day) = civil_from_days(shifted.div_euclid(86_400));
        let of_day = shifted.rem_euclid(86_400);
        let part = |value: i64| u8::try_from(value).expect("time of day fits u8");
        Self {
            year,
            month,
            day,
            hour: part(of_day / 3600),
            minute: part(of_day % 3600 / 60),
            second: part(of_day % 60),
        }
    }

    /// RFC 3339 UTC timestamp used by the local API.
    pub fn rfc3339_utc(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `20240102-030405`, the stamp of a before/after mdat folder.
    pub fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Day count from 1970-01-01 for a proleptic Gregorian date (Howard Hinnant,
/// "chrono-Compatible Low-Level Date Algorithms", public domain).
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day count from 1970-01-01 (Howard
/// Hinnant, "chrono-Compatible Low-Level Date Algorithms", public domain).
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let small = |value: i64| u8::try_from(value).expect("month and day fit u8");
    (year, small(month), small(day))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: i64, offset: i32) -> LocalTime {
        let time = if seconds >= 0 {
            UNIX_EPOCH + Duration::from_secs(seconds.unsigned_abs())
        } else {
            UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
        };
        LocalTime::from_system(time, offset)
    }

    #[test]
    fn local_time_matches_known_dates_and_offsets() {
        let utc = at(1_704_164_645, 0);
        assert_eq!(
            (utc.date(), utc.minute_stamp(), utc.stamp()),
            (
                "2024-01-02".into(),
                "2024-01-02 03:04".into(),
                "20240102-030405".into()
            )
        );
        // Seoul is nine hours ahead; Los Angeles eight behind crosses midnight.
        assert_eq!(at(1_704_164_645, 9 * 3600).stamp(), "20240102-120405");
        assert_eq!(at(1_704_164_645, -8 * 3600).stamp(), "20240101-190405");
        assert_eq!(at(1_704_164_645, 9 * 3600).shift_seconds(-9 * 3600), utc);
        assert_eq!(at(0, 0).stamp(), "19700101-000000");
        assert_eq!(at(-1, 0).stamp(), "19691231-235959");
        assert_eq!(at(951_782_400, 0).date(), "2000-02-29");
        assert_eq!(at(1_709_164_800, 0).date(), "2024-02-29");
        assert_eq!(at(4_107_542_400, 0).date(), "2100-03-01");
        let before = UNIX_EPOCH - Duration::from_millis(1500);
        assert_eq!(LocalTime::from_system(before, 0).stamp(), "19691231-235958");
    }

    fn assert_utc(time: SystemTime, expected: &str) {
        assert_eq!(utc_rfc3339(time), expected);
    }

    fn assert_utc_parts(seconds: i64, nanoseconds: u32, expected: &str) {
        assert_eq!(
            format_utc(i128::from(seconds), nanoseconds, false, false),
            expected
        );
    }

    #[test]
    fn utc_rfc3339_matches_chrono_fraction_precision() {
        assert_utc(UNIX_EPOCH, "1970-01-01T00:00:00+00:00");
        assert_utc(
            UNIX_EPOCH + Duration::from_nanos(123_000_000),
            "1970-01-01T00:00:00.123+00:00",
        );
        assert_utc(
            UNIX_EPOCH + Duration::from_nanos(123_456_000),
            "1970-01-01T00:00:00.123456+00:00",
        );
        assert_utc(
            UNIX_EPOCH + Duration::from_nanos(100),
            "1970-01-01T00:00:00.000000100+00:00",
        );
        // Windows SystemTime has 100 ns resolution, so exercise the shared
        // formatter directly for the 1 ns case.
        assert_utc_parts(0, 1, "1970-01-01T00:00:00.000000001+00:00");
    }

    #[test]
    fn utc_rfc3339_handles_before_epoch_leap_day_and_year_boundary() {
        assert_utc(
            UNIX_EPOCH - Duration::from_millis(500),
            "1969-12-31T23:59:59.500+00:00",
        );
        let leap_day = days_from_civil(2024, 2, 29) * 86_400 + 3_723;
        assert_utc(
            UNIX_EPOCH + Duration::new(leap_day as u64, 123_456_700),
            "2024-02-29T01:02:03.123456700+00:00",
        );
        let year_boundary = days_from_civil(2025, 1, 1) * 86_400;
        assert_utc_parts(
            year_boundary - 1,
            999_999_999,
            "2024-12-31T23:59:59.999999999+00:00",
        );
    }

    #[test]
    fn utc_rfc3339_millis_accepts_only_rfc3339_years() {
        let expected = |unix_ms, text: Option<&str>| {
            assert_eq!(utc_rfc3339_millis(unix_ms).as_deref(), text);
        };
        expected(0, Some("1970-01-01T00:00:00.000Z"));
        expected(-1, Some("1969-12-31T23:59:59.999Z"));

        let first_ms_10000 = days_from_civil(10_000, 1, 1) * 86_400_000;
        expected(first_ms_10000 - 1, Some("9999-12-31T23:59:59.999Z"));
        assert_eq!(utc_rfc3339_millis(first_ms_10000), None);

        let first_ms_year_minus_one = days_from_civil(-1, 1, 1) * 86_400_000;
        assert_eq!(utc_rfc3339_millis(first_ms_year_minus_one), None);
    }
}

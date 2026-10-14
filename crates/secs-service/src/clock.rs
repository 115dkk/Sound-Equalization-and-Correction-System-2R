//! Wall-clock readings for folder names. The user's time zone is an
//! operating-system question, so the app asks `secs-sys` for the UTC offset
//! and the service only does calendar arithmetic.

use std::time::{SystemTime, UNIX_EPOCH};

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

    /// `20240102-030405`, the stamp of a before/after mdat folder.
    pub fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
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
            (utc.date(), utc.stamp()),
            ("2024-01-02".into(), "20240102-030405".into())
        );
        // Seoul is nine hours ahead; Los Angeles eight behind crosses midnight.
        assert_eq!(at(1_704_164_645, 9 * 3600).stamp(), "20240102-120405");
        assert_eq!(at(1_704_164_645, -8 * 3600).stamp(), "20240101-190405");
        assert_eq!(at(0, 0).stamp(), "19700101-000000");
        assert_eq!(at(-1, 0).stamp(), "19691231-235959");
        assert_eq!(at(951_782_400, 0).date(), "2000-02-29");
        assert_eq!(at(1_709_164_800, 0).date(), "2024-02-29");
        assert_eq!(at(4_107_542_400, 0).date(), "2100-03-01");
        let before = UNIX_EPOCH - Duration::from_millis(1500);
        assert_eq!(LocalTime::from_system(before, 0).stamp(), "19691231-235958");
    }
}

//! The user's time zone, which only the operating system knows.

use std::time::{SystemTime, UNIX_EPOCH};

use secs_service::LocalTime;

/// Seconds east of UTC in the user's time zone at `time` (daylight saving
/// included), for `secs_service::LocalTime::from_system`.
pub fn utc_offset_seconds(time: SystemTime) -> i32 {
    let timestamp = jiff::Timestamp::try_from(time).unwrap_or_else(|_| {
        if time.duration_since(UNIX_EPOCH).is_ok() {
            jiff::Timestamp::MAX
        } else {
            jiff::Timestamp::MIN
        }
    });
    jiff::tz::TimeZone::system().to_offset(timestamp).seconds()
}

/// Reads the user's current local wall clock.
#[must_use]
pub fn local_clock() -> LocalTime {
    let now = SystemTime::now();
    LocalTime::from_system(now, utc_offset_seconds(now))
}

/// Reads the current offset east of UTC from the same operating-system source.
#[must_use]
pub fn local_offset_seconds() -> i32 {
    utc_offset_seconds(SystemTime::now())
}

/// Reads milliseconds since the Unix epoch from the system clock.
#[must_use]
pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |after| {
            i64::try_from(after.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_is_within_a_day() {
        let offset = utc_offset_seconds(SystemTime::now());
        assert!((-18 * 3600..=18 * 3600).contains(&offset), "{offset}");
    }
}

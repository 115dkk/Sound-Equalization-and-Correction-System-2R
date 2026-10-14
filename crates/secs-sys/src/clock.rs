//! The user's time zone, which only the operating system knows.

use std::time::SystemTime;

/// Seconds east of UTC in the user's time zone at `time` (daylight saving
/// included), for `secs_service::LocalTime::from_system`.
pub fn utc_offset_seconds(time: SystemTime) -> i32 {
    chrono::DateTime::<chrono::Local>::from(time)
        .offset()
        .local_minus_utc()
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

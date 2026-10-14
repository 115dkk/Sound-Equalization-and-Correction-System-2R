#![forbid(unsafe_code)]
use secs_rew::*;
use serde_json::json;
use std::time::Duration;
fn dead(path: &str, seconds: u64) -> RecordedExchange {
    RecordedExchange {
        request: Request::new("GET", path, None, Duration::from_secs(seconds)),
        response: Err(RewError::Connection {
            path: path.into(),
            message: "refused".into(),
        }),
    }
}
#[test]
fn test_second_request_fails_without_touching_the_network() {
    let t = RecordedTransport::new([dead("/version", 2)]);
    let mut c = RewClient::new(t.clone());
    assert!(!c.is_running());
    for _ in 0..50 {
        assert!(!c.is_running());
        assert!(c.output_devices().is_empty());
    }
    assert_eq!(c.version(), None);
    assert_eq!(c.pro_state(), None);
    assert!(c.is_pro());
    assert_eq!(t.calls().len(), 1);
    t.assert_finished();
}
#[test]
fn test_retries_after_the_window_and_uses_a_short_connect_timeout() {
    let t = RecordedTransport::new([dead("/version", 2), dead("/version", 2)]);
    let mut c = RewClient::new(t.clone());
    assert!(!c.is_running());
    c.hold_offline(Duration::ZERO);
    assert!(!c.is_running());
    assert!(CONNECT_TIMEOUT <= Duration::from_secs(1));
    t.assert_finished();
}
#[test]
fn test_probe_ignores_the_hold_and_reopens_it_when_rew_is_down() {
    let t = RecordedTransport::new([
        dead("/version", 1),
        RecordedExchange::new(
            Request::new("GET", "/version", None, Duration::from_secs(1)),
            json!({"message":"REW"}),
        ),
    ]);
    let mut c = RewClient::new(t.clone());
    c.hold_offline(Duration::from_secs(60));
    assert!(!c.is_running());
    assert!(t.calls().is_empty());
    assert!(!c.probe());
    assert!(!c.is_running());
    assert_eq!(t.calls().len(), 1);
    assert!(c.probe());
    t.assert_finished();
}
#[test]
fn test_r2_probe_circuit_breaker_does_not_block_measurement_requests() {
    let t = RecordedTransport::new([
        dead("/version", 1),
        RecordedExchange::new(
            Request::new(
                "POST",
                "/measure/command",
                Some(json!({"command":"SPL"})),
                MEASURE_TIMEOUT,
            ),
            json!({"message":"Starting measurement"}),
        ),
        RecordedExchange::new(
            Request::new(
                "POST",
                "/measure/command",
                Some(json!({"command":"Cancel"})),
                READ_TIMEOUT,
            ),
            json!({"message":"ok"}),
        ),
    ]);
    let mut c = RewClient::new(t.clone());
    assert!(!c.probe());
    assert!(!c.is_running());
    c.start_sweep().unwrap();
    c.abort();
    t.assert_finished();
}
#[test]
fn test_measure_level_dbfs_and_input_calibration_file_are_none_when_down() {
    for path in ["/measure/level", "/audio/input-cal"] {
        let t = RecordedTransport::new([dead(path, 3)]);
        let mut c = RewClient::new(t.clone());
        if path == "/measure/level" {
            assert_eq!(c.measure_level_dbfs(), None);
        } else {
            assert_eq!(c.input_calibration_file(), None);
        }
        t.assert_finished();
    }
}

#![forbid(unsafe_code)]
use secs_rew::*;
use serde_json::{Value, json};
use std::time::Duration;

fn recorded(key: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("data/recorded.json")).unwrap()[key].clone()
}
fn get(path: &str, body: Value, seconds: u64) -> RecordedExchange {
    RecordedExchange::new(
        Request::new("GET", path, None, Duration::from_secs(seconds)),
        body,
    )
}

#[test]
fn rew_info_reads_recorded_api_shapes_without_writes() {
    let transport = RecordedTransport::new([
        get("/version", recorded("version"), 2),
        get("/application/blocking", json!(true), 5),
        get("/audio/input-cal", recorded("input_cal"), 3),
        get("/audio/driver", recorded("asio_driver"), 5),
        get("/audio/asio/inputs", recorded("inputs"), 5),
        get("/audio/asio/devices", recorded("devices"), 5),
        get("/audio/asio/devices", recorded("devices"), 5),
        get("/audio/asio/outputs", recorded("asio_outputs"), 5),
        get(
            "/audio/samplerate",
            json!({"value": 48000.0, "unit": "Hz"}),
            5,
        ),
        get(
            "/measure/sweep/configuration",
            json!({"startFrequency":20,"endFrequency":20000,"length":"256k"}),
            5,
        ),
        get("/measure/level", recorded("measure_level"), 3),
    ]);
    let mut client = RewClient::new(transport.clone());
    client.set_pro(Some(false));
    let info = client.info();
    assert!(info.running && info.reachable);
    assert_eq!(info.version.as_deref(), Some("5.40 Beta 124 API 0.9.4"));
    assert_eq!(info.pro, Some(false));
    assert_eq!(info.blocking, Some(true));
    assert_eq!(info.input_calibration_file.as_deref(), Some(""));
    assert_eq!(info.sample_rate, Some(48000.0));
    assert_eq!(info.sweep.unwrap().length.as_deref(), Some("256k"));
    assert_eq!(info.level_dbfs, Some(-24.0));
    assert_eq!(info.input_devices.unwrap()[0].channels, 2);
    assert_eq!(info.output_devices.unwrap()[0].channels, 4);
    assert_eq!(info.input_channel_names.unwrap(), ["1: In 1", "2: In 2"]);
    transport.assert_finished();
    assert!(transport.calls().iter().all(|r| r.method == "GET"));
}

#[test]
fn rew_info_unreachable_leaves_all_api_facts_unknown() {
    let transport = RecordedTransport::new([RecordedExchange {
        request: Request::new("GET", "/version", None, Duration::from_secs(2)),
        response: Err(RewError::Connection {
            path: "/version".into(),
            message: "offline".into(),
        }),
    }]);
    let info = RewClient::new(transport.clone()).info();
    assert!(!info.reachable);
    assert!(info.version.is_none() && info.pro.is_none() && info.blocking.is_none());
    assert!(info.input_devices.is_none() && info.sample_rate.is_none() && info.sweep.is_none());
    transport.assert_finished();
}

struct NoIr;
impl IrSource for NoIr {
    fn impulse_response(&self, _: &str, _: u32, _: u32) -> Result<ImpulseResponse, RewError> {
        panic!("info must not measure")
    }
}

#[test]
fn fake_rew_info_covers_running_no_api_and_no_pro() {
    let mut fake = FakeRew::new(NoIr);
    let info = fake.info();
    assert!(info.running && info.reachable);
    assert_eq!(info.pro, Some(true));
    assert_eq!(info.output_devices.unwrap()[0].channels, 16);
    assert_eq!(info.input_channel_names.unwrap().len(), 8);
    fake.options.pro = false;
    assert_eq!(fake.info().pro, Some(false));
    fake.options.api_enabled = false;
    let info = fake.info();
    assert!(info.running && !info.reachable);
    assert!(info.version.is_none() && info.input_calibration_file.is_none());
    fake.options.running = false;
    assert!(!fake.info().running);
}

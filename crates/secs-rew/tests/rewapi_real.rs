#![forbid(unsafe_code)]
// Protocol parity with tests/test_rewapi_real.py (GPL-3.0-only); replies are
// recorded protocol data, not a second implementation of the client.
use secs_rew::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
fn data(key: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("data/recorded.json")).unwrap()[key].clone()
}
fn get(path: &str, reply: Value, seconds: u64) -> RecordedExchange {
    RecordedExchange::new(
        Request::new("GET", path, None, Duration::from_secs(seconds)),
        reply,
    )
}
fn post(path: &str, body: Value) -> RecordedExchange {
    RecordedExchange::new(
        Request::new("POST", path, Some(body), READ_TIMEOUT),
        data("ok"),
    )
}
fn client(script: Vec<RecordedExchange>) -> (RewClient<RecordedTransport>, RecordedTransport) {
    let t = RecordedTransport::new(script);
    (RewClient::new(t.clone()), t)
}
fn configure(reps: u32, timing: &str, current: Value) -> Vec<RecordedExchange> {
    let mut script = vec![
        post("/application/blocking", json!(true)),
        post("/audio/samplerate", json!({"value":48000.0,"unit":"Hz"})),
        post(
            "/measure/sweep/configuration",
            json!({"startFrequency":20,"endFrequency":20000,"length":"256k"}),
        ),
        post("/measure/level", json!({"value":-20.0,"unit":"dBFS"})),
    ];
    if reps > 1 {
        script.push(post("/measure/timing/reference", json!(timing)));
    }
    script.push(get("/measure/sweep/repetitions", current.clone(), 5));
    if current != json!(reps) {
        script.push(post("/measure/sweep/repetitions", json!(reps)));
    }
    if reps <= 1 {
        script.push(post("/measure/timing/reference", json!(timing)));
    }
    script
}
#[test]
fn test_is_running_and_version() {
    let (mut c, t) = client(vec![
        get("/version", data("version"), 2),
        get("/version", data("version"), 2),
    ]);
    assert!(c.is_running());
    assert_eq!(c.version().unwrap(), "5.40 Beta 124 API 0.9.4");
    t.assert_finished();
}
#[test]
fn test_pro_state_only_comes_from_set_pro() {
    let (mut c, t) = client(vec![]);
    assert_eq!(c.pro_state(), None);
    assert!(c.is_pro());
    c.set_pro(Some(false));
    assert!(!c.is_pro());
    c.set_pro(Some(true));
    assert!(c.is_pro());
    t.assert_finished();
}
#[test]
fn test_configure_sends_documented_payloads() {
    let (mut c, t) = client(configure(1, "Acoustic", data("ok")));
    c.configure(&fake_plan(&["L", "R"], &[1])).unwrap();
    t.assert_finished();
}
#[test]
fn test_configure_repetitions_before_acoustic_and_timing_before_multiple() {
    for (reps, timing, current) in [
        (1, "Acoustic", json!(1)),
        (1, "Acoustic", json!(3)),
        (3, "Loopback", json!(1)),
    ] {
        let (mut c, t) = client(configure(reps, timing, current));
        let mut plan = fake_plan(&["L"], &[1]);
        plan.repetitions = reps;
        plan.timing_reference = timing.into();
        c.configure(&plan).unwrap();
        t.assert_finished();
    }
}
#[test]
fn test_sweep_length_is_mapped_to_a_rew_name() {
    for (a, b) in [
        ("256k", "256k"),
        ("1m", "1M"),
        ("262144", "256k"),
        ("300000", "256k"),
    ] {
        assert_eq!(sweep_length_name(a).unwrap(), b);
    }
    assert!(sweep_length_name("nonsense").is_err());
}
#[test]
fn test_driver_is_queried_once_and_cached() {
    let (mut c, t) = client(vec![
        get("/audio/driver", data("asio_driver"), 5),
        get("/audio/asio/outputs", data("asio_outputs"), 5),
    ]);
    assert_eq!(c.driver(), "ASIO");
    assert_eq!(c.output_channel_names().len(), 4);
    assert_eq!(c.driver(), "ASIO");
    assert_eq!(c.output_channel_names().len(), 4);
    t.assert_finished();
}
#[test]
fn test_output_channel_uses_rew_channel_names() {
    let (mut c, t) = client(vec![
        get("/audio/driver", data("java_driver"), 5),
        get("/audio/java/outputs", json!(["L", "R", "C", "SW"]), 5),
        post("/audio/java/output", json!({"output":"C"})),
    ]);
    c.set_output_channel(3).unwrap();
    t.assert_finished();
}
#[test]
fn test_output_channel_matches_asio_number_prefix() {
    let (mut c, t) = client(vec![
        get("/audio/driver", data("asio_driver"), 5),
        get("/audio/asio/outputs", data("asio_outputs"), 5),
        post("/audio/asio/output", json!({"output":"3: Line Out 3"})),
    ]);
    c.set_output_channel(3).unwrap();
    assert_eq!(
        channel_name_for(&["17: Optical".into()], 17),
        Some("17: Optical".into())
    );
    t.assert_finished();
}
#[test]
fn test_output_channel_falls_back_to_number() {
    let mut bad = get("/audio/java/outputs", json!("Bad Request"), 5);
    bad.response.as_mut().unwrap().status = 400;
    let (mut c, t) = client(vec![
        get("/audio/driver", data("java_driver"), 5),
        bad,
        post("/audio/java/output", json!({"output":"5"})),
    ]);
    c.set_output_channel(5).unwrap();
    t.assert_finished();
}
#[test]
fn test_devices_are_plain_string_lists() {
    let (mut c, t) = client(vec![
        get("/audio/driver", data("asio_driver"), 5),
        get("/audio/asio/devices", data("devices"), 5),
        get("/audio/asio/outputs", data("asio_outputs"), 5),
        get("/audio/asio/devices", data("devices"), 5),
        get("/audio/asio/inputs", data("inputs"), 5),
    ]);
    let d = c.output_devices();
    assert_eq!(d[0].name, "MOTU Gen 5");
    assert_eq!(d[0].channels, 4);
    assert_eq!(c.input_devices()[0].name, "MOTU Gen 5");
    assert_eq!(c.output_devices(), d);
    t.assert_finished();
    let (mut c, t) = client(vec![
        get("/audio/driver", data("java_driver"), 5),
        get("/audio/java/input-devices", data("java_inputs"), 5),
        get("/audio/java/inputs", data("inputs"), 5),
    ]);
    assert_eq!(c.input_devices()[0].name, "UMIK-1");
    t.assert_finished();
}
#[test]
fn test_sequential_channels_uses_channel_list_object() {
    let (mut c, t) = client(vec![post(
        "/measure/sequential-channels",
        json!({"channels":["1","2"]}),
    )]);
    c.set_sequential_channels(&["1".into(), "2".into()])
        .unwrap();
    t.assert_finished();
}
#[test]
fn test_timing_ref_output_uses_driver_specific_body() {
    for asio in [true, false] {
        let mut script = vec![get(
            "/audio/driver",
            data(if asio { "asio_driver" } else { "java_driver" }),
            5,
        )];
        script.push(get(
            if asio {
                "/audio/asio/outputs"
            } else {
                "/audio/java/ref-output-channels"
            },
            if asio {
                data("asio_outputs")
            } else {
                json!(["L", "R"])
            },
            5,
        ));
        script.push(post(
            if asio {
                "/audio/asio/ref-output"
            } else {
                "/audio/java/ref-output-channel"
            },
            if asio {
                json!({"output":"2: Main Out 2"})
            } else {
                json!({"channel":"R"})
            },
        ));
        let (mut c, t) = client(script);
        c.set_timing_ref_output(2).unwrap();
        t.assert_finished();
    }
}
#[test]
fn test_r1_pro_required_uses_only_status_and_measure_command_path() {
    for (status, path, pro) in [
        (403, "/measure/command", true),
        (401, "/measure/command", true),
        (402, "/measure/command", true),
        (400, "/measure/command", false),
        (403, "/version", false),
        (403, "/measurements", false),
        (500, "/measure/command", false),
    ] {
        let mut e = post(path, json!({"command":"SPL"}));
        e.response = Ok(Response {
            status,
            body: json!("process problem pro"),
        });
        let (mut c, t) = client(vec![e]);
        let error = c.post(path, json!({"command":"SPL"})).unwrap_err();
        assert_eq!(matches!(error, RewError::ProRequired { .. }), pro);
        assert!(matches!(error.code(), "rew.http" | "rew.pro_required"));
        t.assert_finished();
    }
}
#[test]
fn test_http_error_raises_rew_error() {
    let mut e = get("/measurements", json!("server error"), 10);
    e.response.as_mut().unwrap().status = 500;
    let (mut c, t) = client(vec![e]);
    assert!(matches!(
        c.measurements(),
        Err(RewError::Http { status: 500, .. })
    ));
    t.assert_finished();
}
fn ir_exchange(id: &str, body: Value) -> RecordedExchange {
    let mut e = get(&format!("/measurements/{id}/impulse-response"), body, 15);
    e.request.query = vec![("normalised".into(), "false".into())];
    e
}
#[test]
fn test_get_ir_decodes_big_endian_float32() {
    let (mut c, t) = client(vec![ir_exchange("abc", data("ir"))]);
    assert_eq!(c.get_ir("abc").unwrap(), (vec![1.0, -0.25, 0.125], 48000));
    t.assert_finished();
    assert!(b64_be_f32("%%%!").is_err());
    assert!(b64_be_f32("AA==").is_err());
}
#[test]
fn test_impulse_response_reports_start_time_as_t0() {
    let (mut c, t) = client(vec![ir_exchange("abc", data("ir"))]);
    assert_eq!(c.impulse_response("abc").unwrap().t0_s, -0.125);
    t.assert_finished();
}
#[test]
fn test_missing_start_time_falls_back_to_zero() {
    let mut ir = data("ir");
    ir.as_object_mut().unwrap().remove("startTime");
    let (mut c, t) = client(vec![ir_exchange("abc", ir)]);
    assert_eq!(c.impulse_response("abc").unwrap().t0_s, 0.0);
    t.assert_finished();
}
#[test]
fn test_measurements_map_is_keyed_by_position() {
    let map = json!({"10":{"uuid":"aa","title":"later"},"2":{"uuid":"zz","title":"earlier"}});
    let (mut c, t) = client(vec![
        get("/measurements", map.clone(), 10),
        get("/measurements", map.clone(), 10),
        get("/measurements", map, 10),
    ]);
    assert_eq!(
        c.measurement_ids().unwrap(),
        BTreeSet::from(["aa".into(), "zz".into()])
    );
    assert_eq!(c.measurements().unwrap()[0]["title"], "earlier");
    assert_eq!(
        c.wait_for_new_measurement(
            &BTreeSet::new(),
            Duration::from_secs(1),
            &CancelToken::default()
        )
        .unwrap(),
        Some("aa".into())
    );
    t.assert_finished();
}
fn role_script(role: &str, index: usize, pos: u32, first: bool) -> Vec<RecordedExchange> {
    let id = format!("m{index:03}");
    let mut s = vec![];
    if first {
        s.push(get("/audio/driver", data("java_driver"), 5));
        s.push(get("/audio/java/outputs", json!(["L", "R", "SW"]), 5));
    }
    s.push(post("/audio/java/output", json!({"output":role})));
    s.push(post("/measure/naming",json!({"title":format!("{role} P{pos}"),"namingOption":"Use as entered","prefixMeasNameWithOutput":false})));
    s.push(post(
        "/measure/notes",
        json!(format!("SECS {role} P{pos} (acoustic timing)")),
    ));
    s.push(get(
        "/measurements",
        if first {
            json!({})
        } else {
            json!({"1":{"uuid":"m001"}})
        },
        10,
    ));
    let mut start = post("/measure/command", json!({"command":"SPL"}));
    start.request.timeout = MEASURE_TIMEOUT;
    start.response = Ok(Response {
        status: 202,
        body: data("starting"),
    });
    s.push(start);
    s.push(get("/measurements", json!({}), 10));
    s.push(get("/measurements",json!({index.to_string():{"uuid":id,"title":format!("{role} P{pos}"),"sampleRate":48000.0}}),10));
    s.push(ir_exchange(&id, data("ir")));
    s.push(get("/spl-meter/1/levels", data("spl"), 3));
    s.push(get(
        &format!("/measurements/{id}"),
        json!({"signalToNoisedB":12.9}),
        5,
    ));
    s.push(get(
        &format!("/measurements/{id}"),
        json!({"splOffsetdB":116.12}),
        5,
    ));
    s
}
#[test]
fn test_run_position_sequence_and_ir() {
    let (mut c, t) = client(vec![]);
    let url = c.callback_url("/measure-progress").unwrap();
    for e in configure(1, "Acoustic", json!(1)) {
        t.push(e);
    }
    t.push(post(
        "/measure/subscribe",
        json!({"url":url,"parameters":{}}),
    ));
    for e in role_script("L", 1, 2, true)
        .into_iter()
        .chain(role_script("R", 2, 2, false))
    {
        t.push(e);
    }
    t.push(post("/measure/unsubscribe", json!({"url":url})));
    c.poll_interval = Duration::ZERO;
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let results = c
        .run_position(
            &fake_plan(&["L", "R"], &[1, 2]),
            2,
            Some(Arc::new(move |p| ev.lock().unwrap().push(p))),
            &CancelToken::default(),
        )
        .unwrap();
    assert_eq!(
        results.iter().map(|r| r.role.as_str()).collect::<Vec<_>>(),
        ["L", "R"]
    );
    for r in &results {
        assert_eq!(r.ir, [1.0, -0.25, 0.125]);
        assert_eq!(r.sample_rate, 48000);
        assert_eq!(r.t0_s, -0.125);
        assert_eq!(r.spl_db, Some(78.5));
        assert_eq!(r.snr_db, Some(12.9));
        assert_eq!(r.spl_offset_db, Some(116.12));
    }
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase.as_str())
            .collect::<Vec<_>>(),
        ["sweep", "process", "done", "sweep", "process", "done"]
    );
    t.assert_finished();
}
#[test]
fn test_run_position_waits_until_the_measurement_appears() {
    let mut script = configure(1, "Acoustic", json!(1));
    script.extend(role_script("L", 1, 1, true));
    let (mut c, t) = client(script);
    c.poll_interval = Duration::ZERO;
    assert!(RewClient::<RecordedTransport>::sweep_wait_s(&fake_plan(&["L"], &[1])) > 30.0);
    assert_eq!(
        c.run_position(&fake_plan(&["L"], &[1]), 1, None, &CancelToken::default())
            .unwrap()[0]
            .uuid
            .as_deref(),
        Some("m001")
    );
    t.assert_finished();
}
#[test]
fn test_wait_for_new_measurement_stops_on_cancel() {
    let (mut c, t) = client(vec![]);
    let cancel = CancelToken::default();
    cancel.cancel();
    assert_eq!(
        c.wait_for_new_measurement(&BTreeSet::new(), Duration::from_secs(5), &cancel)
            .unwrap(),
        None
    );
    t.assert_finished();
}
#[test]
fn test_run_position_respects_cancel() {
    let (mut c, t) = client(configure(1, "Acoustic", json!(1)));
    let cancel = CancelToken::default();
    cancel.cancel();
    assert!(
        c.run_position(&fake_plan(&["L", "R"], &[1]), 1, None, &cancel)
            .unwrap()
            .is_empty()
    );
    t.assert_finished();
}
#[test]
fn test_abort_uses_the_cancel_command() {
    let (mut c, t) = client(vec![post("/measure/command", json!({"command":"Cancel"}))]);
    c.abort();
    assert!(MEASURE_COMMANDS.contains(&"Cancel"));
    assert!(MEASURE_COMMANDS.contains(&"Check levels"));
    t.assert_finished();
}
#[test]
fn test_parse_progress_reads_the_real_callback_strings() {
    for (s, p, m) in [
        (
            "0% Capturing noise floor...13%",
            0.0,
            "Capturing noise floor...13%",
        ),
        (
            "41% Remaining sweeps: 1, time: 4 s",
            41.0,
            "Remaining sweeps: 1, time: 4 s",
        ),
        ("100% Measurement complete", 100.0, "Measurement complete"),
    ] {
        assert_eq!(parse_progress(&json!(s)), (Some(p), m.into()));
    }
    assert_eq!(parse_progress(&json!("other")), (None, "other".into()));
    assert_eq!(parse_progress(&Value::Null), (None, String::new()));
}
#[test]
fn test_level_payload_reads_input_levels_arrays() {
    let body = json!({"unit":"dBFS","rms":[-30.0,-12.0],"peak":[-0.5,-6.0],"timeSpanSeconds":0.25});
    assert_eq!(
        level_payload(&body, 0),
        Level {
            input_dbfs: -30.0,
            spl_db: None,
            clip: true
        }
    );
    assert_eq!(
        level_payload(&body, 1),
        Level {
            input_dbfs: -12.0,
            spl_db: None,
            clip: false
        }
    );
    assert_eq!(
        level_payload(&json!({"unit":"SPL","rms":[74.0],"peak":[80.0]}), 0).spl_db,
        Some(74.0)
    );
    assert_eq!(
        level_payload(&json!({"inputs":[{"rmsLevel":-18.0,"peakLevel":-9.0}]}), 0).input_dbfs,
        -18.0
    );
    assert_eq!(level_payload(&json!("bad"), 0).input_dbfs, -120.0);
    assert_eq!(
        level_payload(&json!({"unit":"SPL","rms":[-180.0]}), 0).spl_db,
        None
    );
    assert!(!level_payload(&json!({"peak":-1.0}), 0).clip);
}
#[test]
fn test_play_pink_uses_sine_for_subs() {
    let mut s = vec![
        get("/audio/driver", data("java_driver"), 5),
        get("/audio/java/outputs", json!(["L", "R", "SW"]), 5),
    ];
    for (output, sub, level) in [("L", false, -20.0), ("SW", true, -24.0)] {
        s.push(post("/audio/java/output", json!({"output":output})));
        s.push(post(
            "/generator/signal",
            json!({"signal":if sub {"sine"} else {"pinknoise"}}),
        ));
        if sub {
            s.push(post(
                "/generator/signals/sine/configuration",
                json!({"frequency":50.0}),
            ));
        }
        s.push(post(
            "/generator/level",
            json!({"value":level,"unit":"dBFS"}),
        ));
        s.push(post("/generator/command", json!({"command":"Play"})));
    }
    s.push(post("/generator/command", json!({"command":"Stop"})));
    let (mut c, t) = client(s);
    c.play_pink("L", 1, -20.0).unwrap();
    c.play_pink("SW1", 3, -24.0).unwrap();
    c.stop_generator();
    t.assert_finished();
}
#[test]
fn test_last_levels_is_available_as_a_polling_fallback() {
    let (mut c, t) = client(vec![get("/input-levels/last-levels", data("levels"), 3)]);
    assert_eq!(c.last_levels(), data("levels"));
    t.assert_finished();
}
#[test]
fn test_measure_level_dbfs_reads_the_real_shape() {
    let (mut c, t) = client(vec![get("/measure/level", data("measure_level"), 3)]);
    assert_eq!(c.measure_level_dbfs(), Some(-24.0));
    t.assert_finished();
}
#[test]
fn test_input_calibration_file_empty_loaded_and_per_input() {
    for (path, separate, expected) in [
        ("", false, Some("")),
        ("D:\\cal\\UMIK-1.cal", false, Some("D:\\cal\\UMIK-1.cal")),
        ("", true, None),
    ] {
        let mut d = data("input_cal");
        d["calDataAllInputs"]["calFilePath"] = json!(path);
        d["separateCalFileForEachInput"] = json!(separate);
        let (mut c, t) = client(vec![get("/audio/input-cal", d, 3)]);
        assert_eq!(c.input_calibration_file().as_deref(), expected);
        t.assert_finished();
    }
}
#[test]
fn test_spl_minus_180_is_none_and_missing_snr_is_none() {
    let (mut c, t) = client(vec![
        get("/spl-meter/1/levels", json!({"spl":-180.0}), 3),
        get("/measurements/abc", json!({}), 5),
    ]);
    assert_eq!(c.spl(1), None);
    assert_eq!(c.snr_db("abc"), None);
    t.assert_finished();
}
#[test]
#[should_panic(expected = "does not match next recorded request")]
fn test_recorded_transport_rejects_wrong_json() {
    let (mut c, _) = client(vec![post(
        "/measure/level",
        json!({"value":-20.0,"unit":"dBFS"}),
    )]);
    let _ = c.set_level(-24.0);
}
// The transport still validates the complete script; this wrapper only injects
// the real callback strings when REW would send them during the SPL command.
#[derive(Debug)]
struct CallingTransport {
    recorded: RecordedTransport,
    subscriber: Mutex<Option<String>>,
    delivered: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl Transport for CallingTransport {
    fn request(&self, request: &Request) -> Result<Response, RewError> {
        let response = self.recorded.request(request)?;
        if request.path == "/measure/subscribe" {
            *self.subscriber.lock().unwrap() = request
                .body
                .as_ref()
                .and_then(|v| v["url"].as_str())
                .map(str::to_owned);
        }
        if request.path == "/measure/command"
            && request.body.as_ref().is_some_and(|v| v["command"] == "SPL")
            && let Some(url) = self.subscriber.lock().unwrap().clone()
        {
            let suffix = url.strip_prefix("http://").unwrap();
            let (host, path) = suffix.split_once('/').unwrap();
            let http = HttpTransport::new(&format!("http://{host}")).unwrap();
            for body in [
                "0% Capturing noise floor...50%",
                "41% Remaining sweeps: 1, time: 4 s",
                "100% Measurement complete",
            ] {
                let response = http
                    .request(&Request::new(
                        "POST",
                        &format!("/{path}"),
                        Some(json!(body)),
                        READ_TIMEOUT,
                    ))
                    .unwrap();
                assert_eq!(response.status, 200);
                self.delivered
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
        }
        Ok(response)
    }
}
#[test]
fn test_rew_progress_strings_reach_the_progress_callback() {
    let recorded = RecordedTransport::new([]);
    let (sent, delivered) = std::sync::mpsc::channel();
    let mut c = RewClient::new(CallingTransport {
        recorded: recorded.clone(),
        subscriber: Mutex::new(None),
        delivered: Mutex::new(delivered),
    });
    let url = c.callback_url("measure-progress").unwrap();
    for e in configure(1, "Acoustic", json!(1)) {
        recorded.push(e);
    }
    recorded.push(post(
        "/measure/subscribe",
        json!({"url":url,"parameters":{}}),
    ));
    for e in role_script("L", 1, 1, true) {
        recorded.push(e);
    }
    recorded.push(post("/measure/unsubscribe", json!({"url":url})));
    c.poll_interval = Duration::ZERO;
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let results = c
        .run_position(
            &fake_plan(&["L"], &[1]),
            1,
            Some(Arc::new(move |p| {
                let running = p.phase == "running";
                ev.lock().unwrap().push(p);
                if running {
                    sent.send(()).unwrap();
                }
            })),
            &CancelToken::default(),
        )
        .unwrap();
    let events = events.lock().unwrap();
    let running = events
        .iter()
        .filter(|e| e.phase == "running")
        .collect::<Vec<_>>();
    assert_eq!(
        running
            .iter()
            .map(|e| (e.percent * 10.0).round() / 10.0)
            .collect::<Vec<_>>(),
        [0.0, 36.9, 90.0]
    );
    assert_eq!(running[1].message, "Remaining sweeps: 1, time: 4 s");
    assert!(running.iter().all(|e| e.role == "L"));
    assert_eq!(events.last().unwrap().phase, "done");
    assert_eq!(results[0].snr_db, Some(12.9));
    recorded.assert_finished();
}
#[test]
fn test_r2_full_position_ignores_offline_hold_including_device_lookups() {
    let mut script = configure(1, "Acoustic", json!(1));
    script.extend(role_script("L", 1, 1, true));
    let (mut c, t) = client(script);
    c.hold_offline(Duration::from_secs(60));
    c.poll_interval = Duration::ZERO;
    assert_eq!(
        c.run_position(&fake_plan(&["L"], &[1]), 1, None, &CancelToken::default())
            .unwrap()
            .len(),
        1
    );
    t.assert_finished();
}
#[test]
fn test_cancel_during_running_aborts_and_keeps_terminal_phase_last() {
    let recorded = RecordedTransport::new([]);
    let (sent, delivered) = std::sync::mpsc::channel();
    let mut c = RewClient::new(CallingTransport {
        recorded: recorded.clone(),
        subscriber: Mutex::new(None),
        delivered: Mutex::new(delivered),
    });
    let url = c.callback_url("measure-progress").unwrap();
    for e in configure(1, "Acoustic", json!(1)) {
        recorded.push(e);
    }
    recorded.push(post(
        "/measure/subscribe",
        json!({"url":url,"parameters":{}}),
    ));
    for e in role_script("L", 1, 1, true) {
        let start = e.request.path == "/measure/command";
        recorded.push(e);
        if start {
            break;
        }
    }
    recorded.push(post("/measure/command", json!({"command":"Cancel"})));
    recorded.push(post("/measure/unsubscribe", json!({"url":url})));
    let cancel = CancelToken::default();
    let token = cancel.clone();
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let result = c
        .run_position(
            &fake_plan(&["L"], &[1]),
            1,
            Some(Arc::new(move |p| {
                let running = p.phase == "running";
                ev.lock().unwrap().push(p);
                if running {
                    token.cancel();
                    sent.send(()).unwrap();
                }
            })),
            &cancel,
        )
        .unwrap();
    assert!(result.is_empty());
    assert_eq!(events.lock().unwrap().last().unwrap().phase, "cancelled");
    recorded.assert_finished();
}
#[test]
#[should_panic(expected = "does not match next recorded request")]
fn test_recorded_transport_rejects_wrong_order() {
    let (mut c, _) = client(vec![post("/measure/notes", json!("a"))]);
    let _ = c.set_blocking(true);
}

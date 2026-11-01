#![forbid(unsafe_code)]

#[path = "../examples/support/case_project.rs"]
mod case_project;

use secs_oracle::Fixture;
use secs_service::{Deps, EventSink, NoEvents, Service};
use secs_types::ipc::*;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
struct Events {
    events: Mutex<Vec<JobEvent>>,
    changed: Condvar,
}
impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        self.events.lock().unwrap().push(event);
        self.changed.notify_all();
    }
}
impl Events {
    fn wait(&self, id: JobId) {
        let end = Instant::now() + Duration::from_secs(300);
        let mut events = self.events.lock().unwrap();
        loop {
            if let Some(JobEvent::Finished { status, .. }) = events
                .iter()
                .find(|e| matches!(e, JobEvent::Finished {job, ..} if *job == id))
            {
                assert_eq!(*status, JobStatus::Succeeded, "{events:?}");
                return;
            }
            let remaining = end.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "calibration timed out");
            events = self.changed.wait_timeout(events, remaining).unwrap().0;
        }
    }
}
fn call(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(value) => *value,
        other => panic!("{other:?}"),
    }
}
fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("secs-curves-{}-{name}", std::process::id()))
}
fn open(service: &Service, path: &std::path::Path) {
    call(
        service,
        Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
    );
}
fn graph(service: &Service, tab: ResultTab, role: Option<&str>) -> ResultCurvesResult {
    let Response::ResultCurves(result) = call(
        service,
        Request::ResultCurves(ResultCurvesParams {
            tab,
            channel: role.map(str::to_owned),
            group: None,
            xo_hz: None,
            corrected: None,
        }),
    ) else {
        panic!()
    };
    result
}
fn compare(label: &str, got: &[f64], want: &[f64], tolerance: f64) {
    let (abs, rel) = secs_oracle::errors(got, want);
    eprintln!(
        "MEASURE service_curves/{label} n={} max_abs={abs:e} max_rel={rel:e}",
        got.len()
    );
    secs_oracle::assert_close(label, got, want, tolerance, secs_oracle::PLATFORM_RTOL);
}

#[test]
fn measurement_curves_match_oracle_and_selection() {
    for name in ["dolby", "room_dba"] {
        let root = root(&format!("measurement-{name}"));
        let (path, _) = case_project::write(&root, name);
        let service = Service::new(Deps::default());
        open(&service, &path);
        let fx = Fixture::load("service_curves", name);
        let Response::MeasurementCurves(result) =
            call(&service, Request::MeasurementCurves(Default::default()))
        else {
            panic!()
        };
        compare(
            &format!("{name}/measurement_grid"),
            &result.grid,
            &fx.f64s(&fx.outputs()["measurement_grid"]),
            1e-10,
        );
        let expected = fx.outputs()["measurements"].as_array().unwrap();
        assert_eq!(result.curves.len(), expected.len());
        for curve in &result.curves {
            let want = expected
                .iter()
                .find(|v| v["role"] == curve.role && v["position"].as_i64() == Some(curve.position))
                .unwrap();
            let (samples, rate) =
                secs_project::load_ir(&path, &curve.role, curve.position).unwrap();
            let upstream = secs_engine::display::magnitude_db(
                &samples
                    .iter()
                    .map(|&sample| f64::from(sample))
                    .collect::<Vec<_>>(),
                f64::from(rate),
            );
            compare(
                &format!(
                    "{name}/upstream_measurement/{}/{}",
                    curve.role, curve.position
                ),
                &upstream,
                &fx.f64s(&want["values"]),
                1e-9,
            );
            // U17 preserves upstream's curve shape while sharing one reference:
            // applying its per-curve midrange reference recovers magnitude_db.
            let own_reference = secs_engine::display::midrange_reference(&curve.values);
            let normalized: Vec<_> = curve
                .values
                .iter()
                .map(|value| value - own_reference)
                .collect();
            compare(
                &format!(
                    "{name}/shared_measurement/{}/{}",
                    curve.role, curve.position
                ),
                &normalized,
                &upstream,
                1e-12,
            );
        }
        let one = &result.curves[0];
        let Response::MeasurementCurves(filtered) = call(
            &service,
            Request::MeasurementCurves(MeasurementCurvesParams {
                positions: Some(vec![one.position]),
                roles: Some(vec![one.role.clone()]),
            }),
        ) else {
            panic!()
        };
        assert_eq!(filtered.curves, vec![one.clone()]);
        assert_eq!(filtered.reference, result.reference);
        let preview = graph(&service, ResultTab::Spl, Some("L"));
        assert_eq!(preview.grid.len(), 1390);
        assert_eq!(preview.curves.len(), 2);
        assert!(!preview.stale);
        assert_eq!(
            graph(&service, ResultTab::Gd, None).axes.value,
            [-10.0, 100.0]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn record_ir(
    store: &mut secs_project::MeasurementStore,
    role: &str,
    position: i64,
    samples: Vec<f32>,
) {
    store
        .record(
            secs_project::MeasurementResult {
                role: role.into(),
                pos: position,
                ir: samples,
                sample_rate: 48_000,
                t0_s: 0.0,
                spl_db: None,
                spl_offset_db: None,
                snr_db: None,
                uuid: None,
                source: None,
                source_info: None,
            },
            None,
        )
        .unwrap();
}

fn curve_at<'a>(
    result: &'a MeasurementCurvesResult,
    role: &str,
    position: i64,
) -> &'a MeasurementCurve {
    result
        .curves
        .iter()
        .find(|curve| curve.role == role && curve.position == position)
        .unwrap()
}

fn value_on_grid(grid: &[f64], values: &[f64], frequency: f64) -> f64 {
    let index = grid
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (**left - frequency)
                .abs()
                .total_cmp(&(**right - frequency).abs())
        })
        .unwrap()
        .0;
    values[index]
}

fn value_at(result: &MeasurementCurvesResult, curve: &MeasurementCurve, frequency: f64) -> f64 {
    value_on_grid(&result.grid, &curve.values, frequency)
}

#[test]
fn measurement_curves_share_the_configured_reference() {
    let root = root("measurement-shared-reference");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
    let project = secs_project::Project::from_preset("2.1").unwrap();
    let mut store = secs_project::MeasurementStore::new(&path, project);

    let mut impulse = vec![0.0; 16_384];
    impulse[0] = 1.0;
    let lowpass =
        secs_dsp::iir::butter(2, &[100.0], secs_dsp::iir::BType::Lowpass, 48_000.0).unwrap();
    let sub = secs_dsp::iir::sosfilt(&lowpass, &impulse).unwrap();
    let sub_f32: Vec<_> = sub.iter().map(|&sample| sample as f32).collect();
    let minus_ten = 10.0_f64.powf(-10.0 / 20.0) as f32;

    // Storage order differs from configuration order. L has no position-one
    // measurement, so its lowest position must remain the reference instead of R.
    record_ir(&mut store, "SW1", 1, sub_f32.clone());
    record_ir(
        &mut store,
        "R",
        1,
        impulse
            .iter()
            .map(|&sample| sample as f32 * minus_ten)
            .collect(),
    );
    record_ir(
        &mut store,
        "L",
        2,
        impulse.iter().map(|&sample| sample as f32).collect(),
    );
    store.save().unwrap();

    let service = Service::new(Deps::default());
    open(&service, &path);
    let Response::MeasurementCurves(result) =
        call(&service, Request::MeasurementCurves(Default::default()))
    else {
        panic!()
    };
    assert_eq!(
        result.reference,
        Some(CurveReference {
            role: "L".into(),
            position: 2,
            band: CurveReferenceBand::Mid,
        })
    );
    let sub_curve = curve_at(&result, "SW1", 1);
    let sub_passband = value_at(&result, sub_curve, 40.0);
    assert!((-1.0..0.5).contains(&sub_passband), "{sub_passband}");
    let upstream_sub = secs_engine::display::magnitude_db(
        &sub_f32
            .iter()
            .map(|&sample| f64::from(sample))
            .collect::<Vec<_>>(),
        48_000.0,
    );
    let upstream_passband = value_on_grid(&result.grid, &upstream_sub, 40.0);
    assert!(upstream_passband > 20.0, "{upstream_passband}");
    let scaled = value_at(&result, curve_at(&result, "R", 1), 1000.0);
    assert!((scaled + 10.0).abs() < 1e-5, "{scaled}");

    let Response::MeasurementCurves(filtered) = call(
        &service,
        Request::MeasurementCurves(MeasurementCurvesParams {
            positions: Some(vec![1]),
            roles: Some(vec!["SW1".into()]),
        }),
    ) else {
        panic!()
    };
    assert_eq!(filtered.reference, result.reference);
    assert_eq!(filtered.curves, vec![sub_curve.clone()]);

    let sub_path = root.join("sub-only.secs");
    let mut sub_project = secs_project::Project::from_preset("2.1").unwrap();
    sub_project
        .config
        .speakers
        .retain(|speaker| speaker.role == "SW1");
    let mut sub_store = secs_project::MeasurementStore::new(&sub_path, sub_project);
    record_ir(&mut sub_store, "SW1", 4, sub_f32);
    sub_store.save().unwrap();
    let sub_service = Service::new(Deps::default());
    open(&sub_service, &sub_path);
    let Response::MeasurementCurves(sub_only) =
        call(&sub_service, Request::MeasurementCurves(Default::default()))
    else {
        panic!()
    };
    assert_eq!(
        sub_only.reference,
        Some(CurveReference {
            role: "SW1".into(),
            position: 4,
            band: CurveReferenceBand::Sub,
        })
    );
    let sub_curve = curve_at(&sub_only, "SW1", 4);
    let band: Vec<_> = sub_only
        .grid
        .iter()
        .zip(&sub_curve.values)
        .filter(|(frequency, _)| (40.0..=120.0).contains(*frequency))
        .map(|(_, value)| *value)
        .collect();
    assert!((secs_dsp::pairwise_sum(&band) / band.len() as f64).abs() < 1e-12);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_curves_use_the_corrected_ir() {
    let root = root("measurement-calibration");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
    let raw = {
        let mut values = vec![0.0_f32; 4096];
        values[0] = 1.0;
        values
    };
    let calibration_points = vec![
        secs_audio::CalibrationPoint {
            frequency: 20.0,
            gain_db: 0.0,
            phase_degrees: None,
        },
        secs_audio::CalibrationPoint {
            frequency: 1000.0,
            gain_db: 12.0,
            phase_degrees: None,
        },
        secs_audio::CalibrationPoint {
            frequency: 20_000.0,
            gain_db: 0.0,
            phase_degrees: None,
        },
    ];
    let source = secs_mdat::SourceInfo {
        cal: vec![(
            "meterCal".into(),
            secs_mdat::CalInfo {
                inverse_c_selected: false,
                source_name: "shaped.cal".into(),
                source_path: "shaped.cal".into(),
                freq: Some(
                    calibration_points
                        .iter()
                        .map(|point| point.frequency as f32)
                        .collect(),
                ),
                gain: Some(
                    calibration_points
                        .iter()
                        .map(|point| point.gain_db as f32)
                        .collect(),
                ),
                phase: None,
                boxed: vec![],
            },
        )],
        ..Default::default()
    };
    let mut store = secs_project::MeasurementStore::new(
        &path,
        secs_project::Project::from_preset("2.0").unwrap(),
    );
    store
        .record(
            secs_project::MeasurementResult {
                role: "L".into(),
                pos: 1,
                ir: raw.clone(),
                sample_rate: 48_000,
                t0_s: 0.0,
                spl_db: None,
                spl_offset_db: None,
                snr_db: None,
                uuid: None,
                source: None,
                source_info: Some(source),
            },
            None,
        )
        .unwrap();
    store.save().unwrap();

    let curve = secs_audio::MicCalibration::from_points(calibration_points).unwrap();
    let corrected = secs_audio::ResponseCorrection::new(Some(curve), vec![], false)
        .apply(&raw, 48_000)
        .unwrap();
    let expected = secs_engine::display::magnitude_db(
        &corrected
            .iter()
            .map(|sample| f64::from(*sample))
            .collect::<Vec<_>>(),
        48_000.0,
    );

    let service = Service::new(Deps::default());
    open(&service, &path);
    let Response::MeasurementCurves(result) =
        call(&service, Request::MeasurementCurves(Default::default()))
    else {
        panic!()
    };
    assert_eq!(result.curves.len(), 1);
    assert_eq!(result.curves[0].role, "L");
    assert_eq!(result.curves[0].values, expected);
    assert!(
        result.curves[0]
            .values
            .iter()
            .zip(secs_engine::display::magnitude_db(
                &raw.iter()
                    .map(|sample| f64::from(*sample))
                    .collect::<Vec<_>>(),
                48_000.0,
            ))
            .any(|(corrected, raw)| (corrected - raw).abs() > 1.0)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn result_curves_use_stored_then_run_extra_rc_bands() {
    use secs_types::calibration::RcBand;

    let root = root("rc-bands");
    let (path, mut run_settings) = case_project::write(&root, "room");
    let service = Service::new(Deps::default());
    open(&service, &path);

    let Response::CalibrationSettingsGet(mut stored) =
        call(&service, Request::CalibrationSettingsGet)
    else {
        panic!()
    };
    stored.rc_bands = vec![RcBand {
        lo_hz: 15_000.0,
        hi_hz: 20_000.0,
    }];
    call(
        &service,
        Request::CalibrationSettingsSet(CalibrationSettingsParams { settings: stored }),
    );
    assert_eq!(
        graph(&service, ResultTab::Spl, None).rc_bands_hz,
        [(15_000.0, 20_000.0)]
    );
    assert!(graph(&service, ResultTab::Xo, None).rc_bands_hz.is_empty());

    // A request carries its own settings past the edit checks: the run keeps
    // only the bands the engine uses (ADR 0024).
    run_settings.rc_bands = vec![
        RcBand {
            lo_hz: 10_000.0,
            hi_hz: 12_000.0,
        },
        RcBand {
            lo_hz: 5.0,
            hi_hz: 30.0,
        },
        RcBand {
            lo_hz: 1_000.0,
            hi_hz: 2_000.0,
        },
        RcBand {
            lo_hz: 3_000.0,
            hi_hz: 2_500.0,
        },
    ];
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) = service.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(run_settings),
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let Response::Calibrate(started) = *response else {
        panic!()
    };
    events.wait(started.job);
    for tab in [ResultTab::Spl, ResultTab::Gd] {
        assert_eq!(
            graph(&service, tab, None).rc_bands_hz,
            [(1_000.0, 2_000.0), (10_000.0, 12_000.0)]
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn input_hp_result_curves_show_the_cut_without_changing_the_verdict() {
    let root = root("input-hp");
    let (path, mut settings) = case_project::write(&root, "room");
    let service = Service::new(Deps::default());
    open(&service, &path);

    let calibrate = |settings| {
        let events = Arc::new(Events::default());
        let Envelope::Ok(response) = service.call(
            Request::Calibrate(CalibrateParams {
                settings: Some(settings),
                ..Default::default()
            }),
            events.clone(),
        ) else {
            panic!()
        };
        let Response::Calibrate(started) = *response else {
            panic!()
        };
        events.wait(started.job);
    };
    let verdict = || {
        let Response::Results(results) = call(&service, Request::Results) else {
            panic!()
        };
        results.calibration.unwrap().verdict
    };
    let values = |result: &ResultCurvesResult, kind: &str| {
        result
            .curves
            .iter()
            .find(|curve| curve.kind == kind)
            .unwrap()
            .values
            .clone()
    };

    calibrate(settings.clone());
    let off_spl = graph(&service, ResultTab::Spl, Some("L"));
    let off_gd = graph(&service, ResultTab::Gd, Some("L"));
    let off_ba = graph(&service, ResultTab::Ba, Some("L"));
    let off_verdict = verdict();

    settings.input_hp_hz = Some(27.0);
    calibrate(settings);
    let on_spl = graph(&service, ResultTab::Spl, Some("L"));
    let on_gd = graph(&service, ResultTab::Gd, Some("L"));
    let on_ba = graph(&service, ResultTab::Ba, Some("L"));
    assert_eq!(verdict(), off_verdict);
    assert_eq!(values(&on_spl, "before"), values(&off_spl, "before"));
    assert_eq!(values(&on_gd, "before"), values(&off_gd, "before"));

    for kind in ["after", "target"] {
        let off = values(&off_spl, kind);
        let on = values(&on_spl, kind);
        let at_cut =
            value_on_grid(&on_spl.grid, &on, 27.0) - value_on_grid(&off_spl.grid, &off, 27.0);
        let at_200 =
            value_on_grid(&on_spl.grid, &on, 200.0) - value_on_grid(&off_spl.grid, &off, 200.0);
        assert!((at_cut + 3.0103).abs() < 0.1, "{kind} at 27 Hz: {at_cut}");
        assert!(at_200.abs() < 0.01, "{kind} at 200 Hz: {at_200}");
    }
    let gd_added = value_on_grid(&on_gd.grid, &values(&on_gd, "after"), 27.0)
        - value_on_grid(&off_gd.grid, &values(&off_gd, "after"), 27.0);
    assert!(gd_added > 1.0, "group delay at 27 Hz: {gd_added}");

    for on in &on_ba.curves {
        let off = off_ba
            .curves
            .iter()
            .find(|off| off.kind == on.kind && off.role == on.role)
            .unwrap();
        if matches!(on.kind.as_str(), "target" | "after" | "sum_after") {
            let at_cut = value_on_grid(&on_ba.grid, &on.values, 27.0)
                - value_on_grid(&off_ba.grid, &off.values, 27.0);
            assert!(
                (at_cut + 3.0103).abs() < 0.1,
                "{} at 27 Hz: {at_cut}",
                on.kind
            );
        } else {
            assert_eq!(on.values, off.values, "{} must stay measured", on.kind);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn result_curves_lines_take_values_and_staleness() {
    let name = "room_dba";
    let root = root("results");
    let (path, _) = case_project::write(&root, name);
    let service = Service::new(Deps::default());
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) =
        service.call(Request::Calibrate(Default::default()), events.clone())
    else {
        panic!()
    };
    let Response::Calibrate(started) = *response else {
        panic!()
    };
    events.wait(started.job);
    let fx = Fixture::load("service_curves", name);
    let out = fx.outputs();
    for tab in [ResultTab::Spl, ResultTab::Gd, ResultTab::Xo, ResultTab::Ba] {
        let result = graph(&service, tab, Some("L"));
        assert!(!result.stale);
        assert!(!result.curves.is_empty());
        assert!(
            result
                .curves
                .iter()
                .all(|c| c.values.len() == result.grid.len())
        );
        assert!(result.axes.value[0] < result.axes.value[1]);
    }
    let spl = graph(&service, ResultTab::Spl, Some("L"));
    let expected = out["spl"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["role"] == "L")
        .unwrap();
    compare(
        "room_dba/spl/before",
        &spl.curves[0].values,
        &fx.f64s(&expected["before"]),
        1e-8,
    );
    // Production fitting has its own ADR 0004 end-to-end statistical gate.
    // The fixed-design display transformation is checked separately below.
    assert!(spl.curves[1].values.iter().all(|v| v.is_finite()));
    let Response::ResultLines(lines) = call(&service, Request::ResultLines) else {
        panic!()
    };
    let ba = lines.ba.as_ref().unwrap();
    assert_eq!(ba.rows.len(), 4);
    for row in &ba.rows {
        let value = out["ba"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["role"] == row.role)
            .unwrap();
        let cache: serde_json::Value = serde_json::from_slice(
            &std::fs::read(path.with_extension("irs").join("secs_run_cache.json")).unwrap(),
        )
        .unwrap();
        let chain: secs_types::chain::SignalChain =
            serde_json::from_value(cache["results"]["calibration_chain"].clone()).unwrap();
        let source = chain.subs.iter().find(|sub| sub.role == row.role).unwrap();
        assert_eq!(row.gain_db, source.gain_db);
        assert_eq!(row.delay_ms, source.delay_ms);
        assert_eq!(row.polarity as i64, value["polarity"].as_i64().unwrap());
        assert_eq!(row.mode, value["mode"].as_str().unwrap());
    }
    let Response::SubTakeValues(settings) = call(&service, Request::SubTakeValues) else {
        panic!()
    };
    for row in &ba.rows {
        let entry = &settings
            .subs_manual
            .iter()
            .find(|(r, _)| r == &row.role)
            .unwrap()
            .1;
        assert_eq!(entry.gain_db, row.gain_db);
        assert_eq!(entry.delay_ms, row.delay_ms);
        assert_eq!(entry.allpass, row.allpass);
    }
    call(
        &service,
        Request::PositionClear(PositionClearParams { position: 1 }),
    );
    let stale = graph(&service, ResultTab::Spl, Some("L"));
    assert!(stale.stale);
    assert_eq!(stale.curves, spl.curves);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn curve_requests_validate_and_require_project() {
    let service = Service::new(Deps::default());
    for request in [
        Request::ResultLines,
        Request::SubTakeValues,
        Request::MeasurementCurves(Default::default()),
    ] {
        assert!(matches!(
            service.call(request, Arc::new(NoEvents)),
            Envelope::Err(IpcError {
                code: ErrorCode::ProjectNotOpen,
                ..
            })
        ));
    }
    assert_eq!(
        secs_engine::display::magnitude_db(&[], 48000.0),
        vec![0.0; 280]
    );
}

fn calibrate(service: &Service, subs_only: bool) {
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) = service.call(
        Request::Calibrate(CalibrateParams {
            subs_only,
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("calibration must start")
    };
    let Response::Calibrate(started) = *response else {
        panic!()
    };
    events.wait(started.job);
}

fn ba_lines(service: &Service) -> SubResultLines {
    let Response::ResultLines(lines) = call(service, Request::ResultLines) else {
        panic!()
    };
    lines.ba.expect("room_dba has subs")
}

#[test]
fn subs_only_run_returns_ba_lines_without_full_calibration() {
    let root = root("subs-only-lines");
    let (path, _) = case_project::write(&root, "room_dba");
    let service = Service::new(Deps::default());
    open(&service, &path);

    calibrate(&service, true);
    let Response::ResultLines(lines) = call(&service, Request::ResultLines) else {
        panic!()
    };
    assert!(lines.channels.is_empty());
    let ba = lines
        .ba
        .expect("subs-only calibration must provide BA lines");
    assert!(ba.subs_only);
    assert!(!ba.rows.is_empty());

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn subs_only_run_feeds_the_ba_tab_until_the_next_full_run() {
    // Upstream [서브만 계산]: the BA tab draws `sub_run or run`, the other
    // tabs keep the full result, and a full run drops the subs-only one
    // (`step_calibration.py:1868-1876`, `:2320`).
    let root = root("subs-only");
    let (path, _) = case_project::write(&root, "room_dba");
    let service = Service::new(Deps::default());
    open(&service, &path);

    calibrate(&service, false);
    let spl_full = graph(&service, ResultTab::Spl, Some("L"));
    assert!(!graph(&service, ResultTab::Ba, None).subs_only);
    assert!(!ba_lines(&service).subs_only);

    calibrate(&service, true);
    let ba = graph(&service, ResultTab::Ba, None);
    assert!(ba.subs_only && !ba.stale && !ba.curves.is_empty());
    let first = ba_lines(&service);
    assert!(first.subs_only && !first.rows.is_empty());
    // The other tabs still show the full run.
    let spl = graph(&service, ResultTab::Spl, Some("L"));
    assert!(!spl.subs_only);
    assert_eq!(spl.curves, spl_full.curves);

    // The same inputs give the same subs-only design.
    calibrate(&service, true);
    assert_eq!(ba_lines(&service).rows, first.rows);

    calibrate(&service, false);
    assert!(!graph(&service, ResultTab::Ba, None).subs_only);
    assert!(!ba_lines(&service).subs_only);
    std::fs::remove_dir_all(root).ok();
}

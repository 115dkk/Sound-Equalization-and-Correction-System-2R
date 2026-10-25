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
            compare(
                &format!("{name}/measurement/{}/{}", curve.role, curve.position),
                &curve.values,
                &fx.f64s(&want["values"]),
                1e-9,
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

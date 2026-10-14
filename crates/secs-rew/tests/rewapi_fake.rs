#![forbid(unsafe_code)]
use secs_rew::*;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
#[derive(Debug, Clone)]
struct Source;
impl IrSource for Source {
    fn impulse_response(
        &self,
        role: &str,
        pos: u32,
        sample_rate: u32,
    ) -> Result<ImpulseResponse, RewError> {
        let mut ir = vec![0.0; 2048];
        ir[100 + (pos as usize - 1) * 17] = (if role.starts_with("SW") { 0.5 } else { 1.0 })
            * 10.0_f32.powf(-0.4 * (pos as f32 - 1.0) / 20.0);
        Ok(ImpulseResponse {
            ir,
            sample_rate,
            t0_s: -0.125,
        })
    }
    fn snr_db(&self, _: &str, _: u32) -> Option<f64> {
        Some(45.0)
    }
}
fn fake() -> FakeRew<Source> {
    FakeRew::with_options(
        Source,
        FakeOptions {
            delay: Duration::ZERO,
            level_interval: Duration::from_millis(5),
            ..FakeOptions::default()
        },
    )
}
#[test]
fn test_fake_satisfies_all_protocols() {
    fn protocols<T: RewStatus + DeviceLister + LevelMonitor + Measurer + IRProvider>(_: &T) {}
    protocols(&fake());
    protocols(&RewClient::new(RecordedTransport::new([])));
}
#[test]
fn test_status_and_devices() {
    let mut f = fake();
    assert!(f.is_running());
    assert!(f.is_pro());
    assert!(f.version().is_some());
    assert!(f.output_devices().iter().all(|d| d.kind == "output"));
    assert!(f.input_devices().iter().all(|d| d.kind == "input"));
    assert_eq!(f.output_channel_names().len(), 16);
    assert_eq!(f.input_channel_names().len(), 8);
    f.set_running(false);
    assert!(!f.is_running());
    assert!(!f.is_pro());
    assert_eq!(f.version(), None);
}
#[test]
fn test_plan_validate_and_naming() {
    let p = fake_plan(&["L", "R", "SW1"], &[1, 2, 3]);
    assert!(p.validate().is_empty());
    assert_eq!(p.name_for("L", 2), "L P2");
    assert_eq!(p.output_for("SW1"), 3);
    assert_eq!(p.total_steps(), 9);
    let mut bad = fake_plan(&["L", "L"], &[1, 1]);
    bad.repetitions = 3;
    bad.sweep_length = "300k".into();
    assert_eq!(bad.validate().len(), 4);
}
#[test]
fn test_split_role_and_sweep_length() {
    assert_eq!(split_role("L:LF"), ("L", Some("LF")));
    assert_eq!(split_role("SW1"), ("SW1", None));
    assert_eq!(sweep_length_samples("256k").unwrap(), 262144);
    assert_eq!(sweep_length_samples("1M").unwrap(), 1048576);
}
#[test]
fn test_r4_fake_two_position_plan_uses_only_supplied_ir_source() {
    let mut f = fake();
    let p = fake_plan(&["L", "R", "SW1"], &[1, 2]);
    let events = Arc::new(Mutex::new(vec![]));
    for pos in [1, 2] {
        let ev = events.clone();
        let results = f
            .run_position(
                &p,
                pos,
                Some(Arc::new(move |p| ev.lock().unwrap().push(p))),
                &CancelToken::default(),
            )
            .unwrap();
        assert_eq!(results.len(), 3);
        for r in results {
            assert_eq!(
                r.ir,
                Source.impulse_response(&r.role, pos, 48000).unwrap().ir
            );
            assert_eq!(r.t0_s, -0.125);
            assert_eq!(r.snr_db, Some(45.0));
            assert_eq!(f.get_ir(r.uuid.as_deref().unwrap()).unwrap(), (r.ir, 48000));
        }
    }
    let events = events.lock().unwrap();
    for e in events.chunks(8) {
        assert_eq!(
            e.iter().map(|e| e.phase.as_str()).collect::<Vec<_>>(),
            [
                "sweep", "running", "running", "running", "running", "running", "process", "done"
            ]
        );
        assert_eq!(e[7].percent, 100.0);
    }
    assert_eq!(f.measurements().len(), 6);
    assert_eq!(PHASES, ["sweep", "running", "process", "done", "cancelled"]);
}
#[test]
fn test_ir_is_deterministic_and_position_dependent() {
    let f = fake();
    let a = f.ir_for("L", 1, None).unwrap();
    assert_eq!(a, f.ir_for("L", 1, None).unwrap());
    assert_ne!(a.ir, f.ir_for("L", 2, None).unwrap().ir);
    assert_eq!(a, fake().ir_for("L", 1, None).unwrap());
}
#[test]
fn test_ir_provider_round_trip_and_unknown_id() {
    let mut f = fake();
    let r = f
        .run_position(&fake_plan(&["L"], &[1]), 1, None, &CancelToken::default())
        .unwrap()
        .remove(0);
    assert_eq!(f.get_ir(r.uuid.as_deref().unwrap()).unwrap(), (r.ir, 48000));
    assert!(f.get_ir("missing").is_err());
}
#[test]
fn test_cancel_stops_after_current_role() {
    let mut f = fake();
    let cancel = CancelToken::default();
    let token = cancel.clone();
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let r = f
        .run_position(
            &fake_plan(&["L", "R", "SW1"], &[1]),
            1,
            Some(Arc::new(move |p| {
                if p.role == "R" && p.phase == "sweep" {
                    token.cancel();
                }
                ev.lock().unwrap().push(p);
            })),
            &cancel,
        )
        .unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(events.lock().unwrap().last().unwrap().phase, "cancelled");
}
#[test]
fn test_delay_s_controls_sweep_time() {
    let mut f = fake();
    f.options.delay = Duration::from_millis(100);
    let start = Instant::now();
    f.run_position(&fake_plan(&["L"], &[1]), 1, None, &CancelToken::default())
        .unwrap();
    assert!(start.elapsed() >= Duration::from_millis(90));
}
#[test]
fn test_level_monitor_and_generator() {
    let mut f = fake();
    f.play_pink("L", 1, -20.0);
    let (tx, rx) = mpsc::channel();
    f.start(Arc::new(move |l| tx.send(l).unwrap()));
    for _ in 0..3 {
        let l = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(l.spl_db.is_some());
        assert!(!l.clip);
        assert!(l.input_dbfs > -120.0 && l.input_dbfs < 0.0);
    }
    assert!(f.generator_running());
    assert!(f.level_running());
    f.stop();
    f.stop_generator();
    assert!(!f.level_running());
    assert!(!f.generator_running());
}
#[test]
fn test_level_without_generator_is_noise_floor() {
    let mut f = fake();
    let (tx, rx) = mpsc::channel();
    f.start(Arc::new(move |l| tx.send(l).unwrap()));
    let l = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(l.input_dbfs < -50.0);
    assert_eq!(l.spl_db, None);
    assert!(!l.clip);
    f.stop();
}
#[test]
fn test_measure_level_dbfs_and_calibration_when_running_or_down() {
    let mut f = fake();
    assert_eq!(f.measure_level_dbfs(), Some(-20.0));
    assert_eq!(f.input_calibration_file(), Some(String::new()));
    f.options.measure_level_dbfs = -24.0;
    f.options.cal_file = "C:\\cal\\UMIK-1.cal".into();
    assert_eq!(f.measure_level_dbfs(), Some(-24.0));
    assert_eq!(
        f.input_calibration_file().as_deref(),
        Some("C:\\cal\\UMIK-1.cal")
    );
    f.set_running(false);
    assert_eq!(f.measure_level_dbfs(), None);
    assert_eq!(f.input_calibration_file(), None);
}

#![forbid(unsafe_code)]
use secs_project::{MeasurementStore, Project, PyJson};
use secs_service::measure::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
fn project() -> Project {
    Project::from_preset("2.0").unwrap()
}
fn plan() -> secs_project::MeasurementPlan {
    let mut p = plan_for(&project(), &MeasurementSettings::default());
    p.sweep_length = "64k".into();
    p
}
fn temp(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/measure-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}
fn onset(ir: &[f32]) -> usize {
    let peak = ir.iter().map(|x| x.abs()).fold(0.0_f32, f32::max);
    ir.iter().position(|x| x.abs() >= peak * 0.1).unwrap()
}
fn spectrum(mut re: Vec<f64>) -> Vec<(f64, f64)> {
    let n = re.len();
    let mut im = vec![0.0; n];
    let mut j = 0;
    for i in 1..n {
        let mut b = n >> 1;
        while j & b != 0 {
            j ^= b;
            b >>= 1;
        }
        j ^= b;
        if i < j {
            re.swap(i, j);
        }
    }
    let mut width = 2;
    while width <= n {
        let (wi, wr) = (-2.0 * std::f64::consts::PI / width as f64).sin_cos();
        for start in (0..n).step_by(width) {
            let (mut ur, mut ui) = (1.0, 0.0);
            for k in 0..width / 2 {
                let a = start + k;
                let b = a + width / 2;
                let (vr, vi) = (re[b] * ur - im[b] * ui, re[b] * ui + im[b] * ur);
                re[b] = re[a] - vr;
                im[b] = im[a] - vi;
                re[a] += vr;
                im[a] += vi;
                (ur, ui) = (ur * wr - ui * wi, ur * wi + ui * wr);
            }
        }
        width *= 2;
    }
    re.into_iter().zip(im).take(n / 2 + 1).collect()
}
fn band_error(ir: &[f32], start: isize, expected: &[f32], rate: u32) -> f64 {
    let n = (ir.len() + expected.len() + 4096).next_power_of_two();
    // Shift both signals away from the left edge, retaining pre-onset data.
    let offset = 48000_isize;
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    for (i, x) in ir.iter().enumerate() {
        let j = offset + start + i as isize;
        if j >= 0 && (j as usize) < n {
            a[j as usize] = f64::from(*x);
        }
    }
    for (i, x) in expected.iter().enumerate() {
        b[offset as usize + i] = f64::from(*x);
    }
    let (mut error, mut power) = (0.0, 0.0);
    for (i, ((ar, ai), (br, bi))) in spectrum(a).into_iter().zip(spectrum(b)).enumerate() {
        let f = i as f64 * f64::from(rate) / n as f64;
        if (20.0..=20000.0).contains(&f) {
            error += (ar - br).powi(2) + (ai - bi).powi(2);
            power += br * br + bi * bi;
        }
    }
    10.0 * (error / power).log10()
}
#[test]
fn test_measure_two_positions_then_redo_last() {
    let mut p = plan();
    p.positions = vec![1, 2];
    let (room, origin) = sample::room(&p, 1).unwrap();
    let mut mock = MockMeasurer::with_rooms(BTreeMap::from([(1, room.clone()), (2, room)]));
    let dir = temp("two");
    let path = dir.join("room.secs");
    let mut store = MeasurementStore::new(&path, project());
    for pos in [1, 2] {
        let results = mock
            .measure_position(&p, pos, None, &CancelToken::default())
            .unwrap();
        store.start_position(pos);
        for r in results {
            let (expected, rate, t0) = sample::ir(&r.role, 1).unwrap();
            let arrival = r.t0_s + onset(&r.ir) as f64 / f64::from(rate) + origin;
            let expected_arrival = t0 + onset(&expected) as f64 / f64::from(rate);
            assert!(
                (arrival - expected_arrival).abs() * f64::from(rate) <= 1.0,
                "{} {arrival} {expected_arrival}",
                r.role
            );
            let start = ((r.t0_s + origin - t0) * f64::from(rate)).round() as isize;
            let db = band_error(&r.ir, start, &expected, rate);
            println!(
                "sample {} P{pos}: waveform error {db:.3} dB, arrival error {:.4} samples",
                r.role,
                (arrival - expected_arrival) * f64::from(rate)
            );
            assert!(db < -60.0, "{db}");
            let role = r.role.clone();
            let samples = r.ir.clone();
            store.record(r, Some(&p.roles)).unwrap();
            assert_eq!(store.ir_of(&role, pos).unwrap().0, samples);
        }
    }
    assert_eq!(store.done_positions(Some(&p.roles)), vec![1, 2]);
    assert!(store.clear_position(2));
    store.save().unwrap();
    assert_eq!(
        MeasurementStore::open(&path).unwrap().done_positions(None),
        vec![1]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn test_sample_mode_refuses_position_2() {
    let p = plan();
    let mut mock = MockMeasurer::new();
    assert!(
        mock.measure_position(&p, 2, None, &CancelToken::default())
            .is_err()
    );
    assert!(sample::ir("L:LF", 1).is_err());
    assert_eq!(sample::index().entries.len(), 14);
    assert_eq!(sample::SAMPLE_BYTES.len(), 2_688_000);
}
#[test]
fn test_sample_mode_keeps_the_real_inter_channel_t0_differences() {
    let mut project = Project::from_preset("7.1.4").unwrap();
    project.config.speakers = sample::index()
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| secs_project::Speaker {
            role: e.role.clone(),
            size: "small".into(),
            bands: secs_project::default_bands(&e.role, i as i64 + 1, false),
        })
        .collect();
    let mut p = plan_for(&project, &MeasurementSettings::default());
    p.sweep_length = "64k".into();
    let mut mock = MockMeasurer::new();
    let got = mock
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap();
    let mut arrivals = vec![];
    for r in got {
        let (ir, rate, t0) = sample::ir(&r.role, 1).unwrap();
        // The sweep removes out-of-band energy, which moves some raw 10%
        // threshold crossings. Estimate waveform arrival by correlation, not
        // that discontinuous threshold on differently band-limited signals.
        let start = ((r.t0_s - t0) * f64::from(rate)).round() as isize;
        let lag = (-8_isize..=8)
            .map(|lag| {
                let score = ir
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        let j = i as isize - start + lag;
                        if j >= 0 {
                            f64::from(*x) * f64::from(r.ir.get(j as usize).copied().unwrap_or(0.0))
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>();
                (lag, score)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap()
            .0;
        arrivals.push(lag as f64 + (r.t0_s - t0) * f64::from(rate) - start as f64);
        let db = band_error(&r.ir, start, &ir, rate);
        println!("14-role {} band error {db:.3} dB", r.role);
        assert!(db < -60.0, "{} {db}", r.role);
    }
    println!("14-role sample arrival errors in samples: {arrivals:?}");
    assert_eq!(arrivals.len(), 14);
    assert!(
        arrivals.iter().all(|e| (e - arrivals[0]).abs() <= 1.0),
        "{arrivals:?}"
    );
}
#[test]
fn test_measurements_follow_contract_shape() {
    let p = plan();
    let mut fake = secs_rew::FakeRew::new(sample::SampleSource);
    fake.options.delay = Duration::ZERO;
    let mut rew = RewMeasurer::new(fake, Arc::new(|| 1_704_164_645_000));
    assert!(rew.status().unwrap().available);
    assert!(!rew.devices().unwrap().is_empty());
    let dir = temp("rew");
    let mut store = MeasurementStore::new(dir.join("room.secs"), project());
    for r in rew
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap()
    {
        let (ir, rate, t0) = sample::ir(&r.role, 1).unwrap();
        assert_eq!(r.ir, ir);
        assert_eq!(r.sample_rate, rate);
        assert_eq!(r.t0_s, t0);
        assert!(r.uuid.is_some());
        store.record(r, Some(&p.roles)).unwrap();
    }
    let records = store.records();
    assert_eq!(records.len(), 1);
    assert!(records[0].done);
    assert_eq!(records[0].ir.len(), p.roles.len());
    assert_eq!(records[0].uuid.len(), p.roles.len());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn rew_measurements_capture_calibration_and_keep_bad_sensitivity_measurements() {
    let mut p = plan();
    p.output_device = "출력 장치".into();
    p.input_device = "입력 장치".into();
    let dir = temp("rew-source");
    let cal = dir.join("마이크.cal");
    std::fs::write(&cal, b"Sens Factor = -1.337 dB\n20 -1.25 0.5\n1000 0.25\n").unwrap();
    let mut fake = secs_rew::FakeRew::new(sample::SampleSource);
    fake.options.delay = Duration::ZERO;
    fake.options.cal_file = cal.to_string_lossy().into_owned();
    let clock_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let clock_copy = clock_calls.clone();
    let mut rew = RewMeasurer::new(
        fake,
        Arc::new(move || {
            clock_copy.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            1_704_164_645_000
        }),
    );
    let results = rew
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap();
    assert_eq!(
        clock_calls.load(std::sync::atomic::Ordering::Relaxed),
        results.len()
    );
    let source = results[0].source_info.as_ref().unwrap();
    assert_eq!(
        source.date,
        Some(secs_mdat::SourceDate::Int(1_704_164_645_000))
    );
    assert_eq!(
        source.strings,
        vec![
            (
                "outputName".into(),
                format!(
                    "{}, out {}",
                    rew.client
                        .options
                        .current_audio
                        .output_device
                        .as_deref()
                        .unwrap(),
                    p.output_for(&results[0].role)
                )
            ),
            (
                "sourceFileName".into(),
                rew.client
                    .options
                    .current_audio
                    .input_device
                    .clone()
                    .unwrap(),
            ),
        ]
    );
    assert_eq!(
        source.boxed,
        vec![(
            "usesEmbeddedCalData".into(),
            secs_mdat::Boxed::Boolean(true)
        )]
    );
    let meter = &source.cal[0].1;
    assert_eq!(meter.source_name, "마이크.cal");
    assert_eq!(meter.source_path, cal.to_string_lossy());
    assert_eq!(meter.freq, Some(vec![20.0, 1000.0]));
    assert_eq!(meter.gain, Some(vec![-1.25, 0.25]));
    assert_eq!(meter.phase, Some(vec![0.5, 0.0]));
    assert_eq!(
        meter.boxed,
        vec![("sensitivitydB".into(), secs_mdat::Boxed::Double(-1.337))]
    );

    std::fs::write(&cal, b"Sens Factor = 1.2.3 dB\n20 -1.0\n").unwrap();
    let bad = rew
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap();
    assert_eq!(bad.len(), p.roles.len());
    assert!(bad.iter().all(|result| result.source_info.is_none()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rew_source_devices_fall_back_only_to_names_in_rew_lists() {
    let mut p = plan();
    p.roles.truncate(1);
    p.output_device = "Demo Speakers (2ch)".into();
    p.input_device = "Demo USB Mic".into();
    let mut fake = secs_rew::FakeRew::new(sample::SampleSource);
    fake.options.delay = Duration::ZERO;
    fake.options.current_audio = secs_rew::CurrentAudio::default();
    let mut rew = RewMeasurer::new(fake, Arc::new(|| 0));
    let result = rew
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap();
    let source = result[0].source_info.as_ref().unwrap();
    assert!(source.strings.iter().any(|(key, value)| {
        key == "outputName" && value.starts_with("Demo Speakers (2ch), out ")
    }));
    assert!(
        source
            .strings
            .iter()
            .any(|(key, value)| key == "sourceFileName" && value == "Demo USB Mic")
    );

    p.output_device = "wasapi:{not-rew-output}".into();
    p.input_device = "wasapi:{not-rew-input}".into();
    let result = rew
        .measure_position(&p, 1, None, &CancelToken::default())
        .unwrap();
    assert!(result[0].source_info.is_none());
}

#[test]
fn test_cancel_before_start_measures_nothing() {
    let cancel = CancelToken::default();
    cancel.cancel();
    let events = Arc::new(Mutex::new(vec![]));
    let copy = events.clone();
    let mut mock = MockMeasurer::new();
    assert!(
        mock.measure_position(
            &plan(),
            1,
            Some(Arc::new(move |p| copy.lock().unwrap().push(p))),
            &cancel
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(events.lock().unwrap().last().unwrap().phase, "cancelled");
}
#[test]
fn level_check_mock_and_apply_recommended_level_u1() {
    let mut project = project();
    let p = plan();
    let mut mock = MockMeasurer::new();
    let report = mock
        .level_check(&p, "L", 0.5, &CancelToken::default())
        .unwrap();
    assert!(report.input_dbfs.is_finite());
    let level = apply_level(&mut project, report, p.sweep_level_db).unwrap();
    let next = plan_for(&project, &MeasurementSettings::default());
    assert_eq!(next.sweep_level_db, level);
    let report = mock
        .level_check(&next, "L", 0.5, &CancelToken::default())
        .unwrap();
    println!(
        "U1 recommended {level:.4} dBFS; measured {:.4} dBFS",
        report.input_dbfs
    );
    assert!((report.input_dbfs + 22.5).abs() < 1e-4);
}
#[test]
fn test_changes_update_extra_plan_and_step1_combo() {
    let mut project = project();
    store_devices(
        &mut project,
        vec![
            ("output".into(), PyJson::Str("sim-room".into())),
            ("timing_reference".into(), PyJson::Str("none".into())),
            ("repetitions".into(), PyJson::Int("2".into())),
            ("sweep_length".into(), PyJson::Str("512k".into())),
        ],
    );
    let p = plan_for(&project, &MeasurementSettings::default());
    assert_eq!(p.output_device, "sim-room");
    assert_eq!(project.config.device, "sim-room");
    assert_eq!(p.repetitions, 2);
    assert_eq!(p.sweep_length, "512k");
    assert_eq!(to_rew_plan(&p).unwrap().repetitions, 2);
    store_devices(
        &mut project,
        vec![("timing_reference".into(), PyJson::Str("acoustic".into()))],
    );
    assert_eq!(
        plan_for(&project, &MeasurementSettings::default()).repetitions,
        1
    );
}

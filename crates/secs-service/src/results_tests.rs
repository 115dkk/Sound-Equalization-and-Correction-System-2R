use super::*;
use secs_project::{MeasurementResult, MeasurementStore, Project, store_calibration};
use secs_types::chain::{MimoFilter, MimoPath, MimoPaths, SignalChain};
use secs_types::compute::{ComputeBackend, ComputePreference, ComputeReason, ComputeStatus};
use secs_types::ipc::{CalibrationSummary, MimoSummary, VerdictSummary};

fn project(positions: usize, mimo_on: bool) -> Project {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "secs-result-book-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut store = MeasurementStore::new(root.with_extension("secs"), Project::default());
    for pos in 1..=positions {
        store
            .record(
                MeasurementResult {
                    role: "L".into(),
                    pos: pos as i64,
                    ir: vec![1.0],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: Some(format!("uuid-{positions}-{pos}")),
                    source: None,
                    source_info: None,
                },
                None,
            )
            .unwrap();
    }
    let mut project = store.project().clone();
    let mut settings = calibration_from_project(&project);
    settings.mimo_on = mimo_on;
    store_calibration(&mut project, &settings);
    let _ = std::fs::remove_dir_all(root.with_extension("irs"));
    project
}

fn calibration_summary() -> CalibrationSummary {
    CalibrationSummary {
        cached: false,
        stale: false,
        stale_reason: None,
        mode: secs_types::calibration::Mode::Mixed,
        ultra_ms: 100.0,
        verdict: VerdictSummary {
            within_1db_before_pct: 0.0,
            within_1db_after_pct: 0.0,
            low_band_time_spread_before_ms: 0.0,
            low_band_time_spread_after_ms: 0.0,
            added_delay_ms: 0.0,
            position_spread_db: 0.0,
        },
        scorecard: vec![],
        late_energy: vec![],
        latency_ms: 0.0,
        notes: vec![],
        main_roles: vec!["L".into()],
        sub_roles: vec![],
    }
}

fn mimo_summary(supports: usize) -> MimoSummary {
    MimoSummary {
        compute: Default::default(),
        stale: false,
        kind: "iir".into(),
        fsiso: 120.0,
        support_count: supports,
        fir_taps_total: 0,
        notes: vec![],
        input_count: usize::from(supports > 0),
        scorecard: vec![],
        late_energy: vec![],
    }
}

fn paths(supports: usize) -> MimoPaths {
    MimoPaths {
        fsiso: 120.0,
        paths: (0..supports)
            .map(|index| MimoPath {
                input: "L".into(),
                support: format!("SW{index}"),
                filter: MimoFilter::Iir {
                    gain_db: 0.0,
                    delay_ms: 0.0,
                    polarity: 1,
                    peq: vec![],
                    allpass: vec![],
                },
                low_hz: 20.0,
                delay_ms: 0.0,
            })
            .collect(),
    }
}

fn cache(project: &Project, supports: usize, compute: ComputeIdentity) -> CachedResults {
    let settings = calibration_from_project(project);
    let signature = signature_key(project);
    let target = target_key_string(project);
    let config = config_key(project);
    let run = crate::calibration::run_key(&signature, &target, &settings.engine_key(), &config);
    CachedResults {
        calibration: calibration_summary(),
        calibration_chain: SignalChain {
            sample_rate: 48_000,
            mains: vec![],
            subs: vec![],
            sub_bus_peq: vec![],
            main_extra_ms: 0.0,
            lift_ms: 0.0,
            latency_ms: 0.0,
        },
        mimo: Some(mimo_summary(supports)),
        measurement_signature: signature,
        target_key: target,
        engine_key: settings.engine_key(),
        run_key: Some(run.clone()),
        config_key: Some(config),
        mimo_key: Some(settings.mimo_key()),
        mimo_calibration_key: Some(run),
        labels: crate::cache::CachedRunLabels {
            mode: "mixed".into(),
            sample_rate_policy: Default::default(),
            ultra_ms: 100.0,
            latency_ms: 0.0,
            rc_hi_hz: 500.0,
            rc_bands: Vec::new(),
            lf_align: false,
            mode_decay: false,
            hf_phase: false,
            mimo_max_supports: 0,
        },
        mimo_paths: Some(paths(supports)),
        mimo_compute_key: compute,
    }
}

fn live_book(project: &Project, supports: usize, compute: ComputeIdentity) -> ResultBook {
    live_book_with_settings(
        project,
        calibration_from_project(project),
        measured_positions(project).len(),
        supports,
        compute,
    )
}

fn live_book_with_settings(
    project: &Project,
    settings: secs_types::calibration::CalibrationSettings,
    positions: usize,
    supports: usize,
    compute: ComputeIdentity,
) -> ResultBook {
    let signature = signature_key(project);
    let target = target_key_string(project);
    let config = config_key(project);
    let run = crate::calibration::run_key(&signature, &target, &settings.engine_key(), &config);
    let mut book = ResultBook::default();
    book.commit_calibration(
        CalibrationRun::metadata_only(
            calibration_summary(),
            settings.clone(),
            signature,
            target,
            config,
            run.clone(),
            positions,
        ),
        false,
    );
    book.commit_mimo(MimoResult {
        summary: mimo_summary(supports),
        settings_key: settings.mimo_key(),
        compute_key: compute,
        calibration_key: run,
        run: secs_engine::MimoRun {
            key: secs_engine::MimoKey::from_settings(&settings),
            ideal: Default::default(),
            paths: paths(supports),
            final_resp: vec![],
            applied: vec![],
            scorecard: vec![],
            late_energy: vec![],
            spl_mimo: vec![],
            notes: vec![],
            fir: false,
            compute: Default::default(),
        },
    });
    book
}

type CacheMutation = fn(&mut CachedResults);

fn gpu_compute() -> ComputeIdentity {
    ComputeIdentity::from(&ComputeStatus {
        preference: ComputePreference::Gpu,
        selected: ComputeBackend::Cuda,
        reason: ComputeReason::UserGpu,
        device: Some(secs_types::compute::GpuDeviceInfo {
            id: 1,
            name: "test".into(),
            compute_major: 8,
            compute_minor: 0,
            multiprocessors: 1,
            memory_bytes: 1,
            integrated: false,
            supported: true,
            recommended: true,
            reason: ComputeReason::Recommended,
        }),
        ..Default::default()
    })
}

#[test]
fn cached_result_book_centralizes_staleness_reasons_and_mimo_export_decisions() {
    let current = project(3, true);
    let cpu = ComputeIdentity::default();
    let fresh = cache(&current, 1, cpu.clone());
    let cases = [
        (
            "included",
            current.clone(),
            fresh.clone(),
            cpu.clone(),
            None,
            MimoExportDecision::Included,
        ),
        (
            "no supports",
            current.clone(),
            cache(&current, 0, cpu.clone()),
            cpu.clone(),
            None,
            MimoExportDecision::NoSupports,
        ),
        (
            "compute",
            current.clone(),
            fresh.clone(),
            gpu_compute(),
            None,
            MimoExportDecision::Stale,
        ),
        (
            "off",
            project(3, false),
            fresh.clone(),
            cpu.clone(),
            None,
            MimoExportDecision::Off,
        ),
        (
            "positions",
            project(2, true),
            cache(&project(2, true), 1, cpu.clone()),
            cpu.clone(),
            None,
            MimoExportDecision::TooFewPositions,
        ),
    ];
    for (name, project, cache, compute, stale_reason, decision) in cases {
        let summary = ResultBook::from_cache(Some(cache)).summary(&project, &compute);
        assert_eq!(
            summary.calibration.unwrap().stale_reason.as_deref(),
            stale_reason,
            "{name}"
        );
        assert_eq!(summary.mimo_export, decision, "{name}");
    }

    let stale_cases: [(&str, CacheMutation); 4] = [
        ("measurements", |cache| {
            cache.measurement_signature = "old".into()
        }),
        ("config", |cache| cache.config_key = Some("old".into())),
        ("settings", |cache| cache.engine_key = "old".into()),
        ("target", |cache| cache.target_key = "old".into()),
    ];
    for (reason, mutate) in stale_cases {
        let mut stale = fresh.clone();
        mutate(&mut stale);
        let summary = ResultBook::from_cache(Some(stale)).summary(&current, &cpu);
        assert_eq!(
            summary.calibration.unwrap().stale_reason.as_deref(),
            Some(reason)
        );
        assert_eq!(summary.mimo_export, MimoExportDecision::Stale);
    }

    let summary = ResultBook::from_cache(None).summary(&current, &cpu);
    assert_eq!(summary.mimo_export, MimoExportDecision::None);

    // A cache older than run keys cannot show which calibration its MIMO
    // came from: the calibration stays current, the MIMO does not.
    let mut keyless = fresh.clone();
    keyless.run_key = None;
    keyless.mimo_calibration_key = None;
    let summary = ResultBook::from_cache(Some(keyless)).summary(&current, &cpu);
    assert_eq!(summary.calibration.unwrap().stale_reason, None);
    assert!(summary.mimo.unwrap().stale);
    assert_eq!(summary.mimo_export, MimoExportDecision::Stale);

    let summary = ResultBook::from_cache(None).summary(&project(2, true), &cpu);
    assert_eq!(summary.mimo_export, MimoExportDecision::TooFewPositions);

    let summary = live_book(&current, 1, cpu.clone()).summary(&current, &cpu);
    assert_eq!(summary.calibration.unwrap().stale_reason, None);
    assert_eq!(summary.mimo_export, MimoExportDecision::Included);

    // A live run counts the positions its engine used, as upstream does.
    let calibration = calibration_from_project(&current);
    let summary =
        live_book_with_settings(&current, calibration, 2, 1, cpu.clone()).summary(&current, &cpu);
    assert_eq!(summary.calibration.unwrap().stale_reason, None);
    assert_eq!(summary.mimo_export, MimoExportDecision::TooFewPositions);

    let mut stale_settings = calibration_from_project(&current);
    stale_settings.rc_lo += 1.0;
    let summary = live_book_with_settings(&current, stale_settings, 3, 1, cpu.clone())
        .summary(&current, &cpu);
    assert_eq!(
        summary.calibration.unwrap().stale_reason.as_deref(),
        Some("settings")
    );
    assert_eq!(summary.mimo_export, MimoExportDecision::Stale);

    let live = live_book(&current, 1, cpu.clone());
    let mut changed_mimo = current.clone();
    let mut changed_settings = calibration_from_project(&changed_mimo);
    changed_settings.fsiso -= 1.0;
    store_calibration(&mut changed_mimo, &changed_settings);
    let summary = live.summary(&changed_mimo, &cpu);
    assert_eq!(summary.calibration.unwrap().stale_reason, None);
    assert_eq!(summary.mimo_export, MimoExportDecision::Stale);

    let changed = project(4, true);
    let live = live_book(&current, 1, cpu.clone());
    let summary = live.summary(&changed, &cpu);
    assert_eq!(
        summary.calibration.unwrap().stale_reason.as_deref(),
        Some("measurements")
    );
    assert_eq!(summary.mimo_export, MimoExportDecision::Stale);
}

#[test]
fn graphs_draw_mimo_without_supports_but_exports_need_them() {
    let current = project(3, true);
    let cpu = ComputeIdentity::default();
    let empty = live_book(&current, 0, cpu.clone());
    let verdict = empty.mimo(&current, &cpu);
    assert_eq!(verdict.decision, MimoExportDecision::NoSupports);
    assert!(matches!(verdict.shown(), Some(MimoView::Live(_))));
    assert!(verdict.exported().is_none());

    let full = live_book(&current, 1, cpu.clone());
    let verdict = full.mimo(&current, &cpu);
    assert!(matches!(verdict.shown(), Some(MimoView::Live(_))));
    assert!(matches!(verdict.exported(), Some(MimoView::Live(_))));

    let stale = full.mimo(&current, &gpu_compute());
    assert!(stale.shown().is_none());
    assert!(stale.exported().is_none());
}

#[test]
fn a_live_calibration_hides_the_cached_mimo_until_mimo_runs_again() {
    let current = project(3, true);
    let cpu = ComputeIdentity::default();
    let mut book = ResultBook::from_cache(Some(cache(&current, 1, cpu.clone())));
    let live = live_book(&current, 1, cpu.clone());
    book.commit_calibration(
        CalibrationRun::metadata_only(
            calibration_summary(),
            calibration_from_project(&current),
            signature_key(&current),
            target_key_string(&current),
            config_key(&current),
            live.live_calibration().unwrap().run_key.clone(),
            3,
        ),
        false,
    );
    let summary = book.summary(&current, &cpu);
    assert!(summary.mimo.is_none());
    assert_eq!(summary.mimo_export, MimoExportDecision::None);
}

#[test]
fn calibration_signature_keeps_old_caches_without_curves() {
    let mut current = project(1, false);
    let identities = match &current.measurements {
        secs_project::PyJson::Array(entries) => entries
            .iter()
            .map(|entry| match entry {
                secs_project::PyJson::Object(fields) => fields
                    .iter()
                    .find(|(key, _)| key == "uuid")
                    .map(|(_, value)| value),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    let old_signature = format!(
        "{:?}:{identities:?}",
        secs_project::measurement_signature(&current)
    );
    assert_eq!(signature_key(&current), old_signature);

    let cached = cache(&current, 0, ComputeIdentity::default());
    assert_eq!(cached_stale_reason(&cached, &current), None);
    current
        .measurement_calibration
        .insert((1, "L".into()), 0x0123_4567_89ab_cdef);
    assert_ne!(signature_key(&current), old_signature);
    assert_eq!(cached_stale_reason(&cached, &current), Some("measurements"));
}

#[test]
fn stale_reasons_prioritize_measurements_then_config_settings_and_target() {
    let project = Project::default();
    let signature = signature_key(&project);
    let engine = calibration_from_project(&project).engine_key();
    let target = target_key_string(&project);
    let config = config_key(&project);
    assert_eq!(
        stale_reason("changed", "changed", "changed", Some("changed"), &project),
        Some("measurements")
    );
    assert_eq!(
        stale_reason(&signature, "changed", "changed", Some("changed"), &project),
        Some("config")
    );
    assert_eq!(
        stale_reason(&signature, "changed", "changed", Some(&config), &project),
        Some("settings")
    );
    assert_eq!(
        stale_reason(&signature, &engine, "changed", Some(&config), &project),
        Some("target")
    );
    assert_eq!(
        stale_reason(&signature, &engine, &target, Some(&config), &project),
        None
    );
    // Caches older than the config key are not stale for it.
    assert_eq!(
        stale_reason(&signature, &engine, &target, None, &project),
        None
    );
}

#[test]
fn xo_recommendation_staleness_ignores_xo_and_clears_after_recommendation() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    settings.xo_rec = vec![("프론트".into(), 80.0)];
    store_calibration(&mut project, &settings);
    let book = ResultBook::default();
    assert!(
        !book
            .summary(&project, &ComputeIdentity::default())
            .xo_rec_stale
    );

    settings.xo_rec_key = xo_rec_key(&project, &settings);
    store_calibration(&mut project, &settings);
    assert!(
        !book
            .summary(&project, &ComputeIdentity::default())
            .xo_rec_stale
    );

    settings.xo = vec![("프론트".into(), 100.0)];
    store_calibration(&mut project, &settings);
    assert!(
        !book
            .summary(&project, &ComputeIdentity::default())
            .xo_rec_stale
    );

    settings.boost_max_db = 6.0;
    store_calibration(&mut project, &settings);
    assert!(
        book.summary(&project, &ComputeIdentity::default())
            .xo_rec_stale
    );

    settings.xo_rec_key = xo_rec_key(&project, &settings);
    store_calibration(&mut project, &settings);
    assert!(
        !book
            .summary(&project, &ComputeIdentity::default())
            .xo_rec_stale
    );
}

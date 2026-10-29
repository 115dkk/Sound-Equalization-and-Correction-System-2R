//! End-to-end acoustic comparison with actual production fitters and CUDA.
#![forbid(unsafe_code)]
use secs_compute::{CancelToken, Context, MemoryBudget, NoProgress};
use secs_engine::{Tuning, mimo};
use secs_engine_testkit as test_support;
use secs_types::{calibration::CalibrationSettings, compute::ComputePreference};
use std::{sync::Arc, time::Instant};

fn main() {
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let args: Vec<_> = std::env::args().collect();
    let name = args.get(1).map_or("room", String::as_str);
    let fir = args.get(2).is_some_and(|v| v == "fir");
    let threads = std::env::var("SECS_PERF_THREADS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap().get());
    let case = test_support::production_case(name, threads);
    let ctx = Context::new(
        threads,
        MemoryBudget::new(u64::MAX),
        Arc::new(NoProgress),
        CancelToken::new(),
    );
    let mut settings: CalibrationSettings = serde_json::from_value(case.settings.clone()).unwrap();
    settings.mimo_on = true;
    settings.mimo_fir = fir;
    let mut runs = Vec::new();
    let mut acoustics = Vec::new();
    let requested = if std::env::var_os("SECS_GPU_PROBE_AUTO").is_some() {
        ComputePreference::Auto
    } else {
        ComputePreference::Gpu
    };
    for preference in [ComputePreference::Cpu, requested] {
        let selection = secs_gpu::select(preference);
        let repeats: usize = std::env::var("SECS_PERF_RUNS")
            .ok()
            .map(|v| v.parse().unwrap())
            .unwrap_or(1);
        assert!(repeats > 0);
        let mut times = Vec::new();
        let mut latest = None;
        for sample in 0..if repeats > 1 { repeats + 1 } else { 1 } {
            let start = Instant::now();
            let result = mimo::compute_with_acceleration(
                &case.design,
                &case.measurements,
                &case.config,
                &settings,
                &Tuning::default(),
                &ctx,
                &selection,
            )
            .unwrap();
            let seconds = start.elapsed().as_secs_f64();
            let warmup = repeats > 1 && sample == 0;
            println!(
                "{}",
                serde_json::json!({"case":name,"fir":fir,"threads":threads,"sample":sample,"warmup":warmup,"seconds":seconds,"compute":result.compute})
            );
            if !warmup {
                times.push(seconds);
            }
            latest = Some(result);
        }
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"case":name,"fir":fir,"preference":preference,"samples_s":times,"median_s":sorted[sorted.len()/2]})
        );
        let result = latest.unwrap();
        let metrics = test_support::mimo_acoustic_metrics(&case, &result, &ctx).unwrap();
        for metric in &metrics {
            println!(
                "{}",
                serde_json::json!({"case":name,"fir":fir,"preference":preference,"input":metric.input,"late_energy_db":metric.late_energy_db,"worst_band_db":metric.worst_band_db,"modal_tail_guard":metric.modal_tail_guard,"paths":metric.paths})
            );
        }
        acoustics.push(metrics);
        runs.push(result);
    }
    let cpu = &runs[0];
    let gpu = &runs[1];
    assert_eq!(cpu.ideal.supports.len(), gpu.ideal.supports.len());
    let mut ideal_diff = 0.0;
    let mut ideal_norm = 0.0;
    let mut structure_equal = true;
    for ((role, a), (other, b)) in cpu.ideal.supports.iter().zip(&gpu.ideal.supports) {
        structure_equal &= role == other && a.len() == b.len();
        for ((support, aa), (other_support, bb)) in a.iter().zip(b) {
            structure_equal &= support == other_support && aa.len() == bb.len();
            for (x, y) in aa.iter().zip(bb) {
                ideal_diff += (x - y).powi(2);
                ideal_norm += x * x;
            }
        }
    }
    let relative = (ideal_diff / ideal_norm.max(1e-30)).sqrt();
    let mut spl_max: f64 = 0.0;
    let mut spl_energy = 0.0;
    let mut spl_count = 0;
    for ((role, a), (other, b)) in cpu.spl_mimo.iter().zip(&gpu.spl_mimo) {
        assert_eq!(role, other);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            let d = x - y;
            spl_max = spl_max.max(d.abs());
            spl_energy += d * d;
            spl_count += 1;
        }
    }
    let spl_rms = (spl_energy / spl_count as f64).sqrt();
    println!(
        "{}",
        serde_json::json!({"case":name,"fir":fir,"ideal_structure_equal":structure_equal,"ideal_support_relative_l2":relative,"spl_max_absolute_db":spl_max,"spl_rms_db":spl_rms,"cpu_path_count":cpu.paths.paths.len(),"gpu_path_count":gpu.paths.paths.len()})
    );
    assert!(structure_equal, "ideal support structure changed");
    assert!(relative < 1e-6, "ideal support error {relative}");
    assert!(spl_max < 0.5, "applied SPL difference {spl_max}dB");
    assert_eq!(gpu.compute.fallbacks, 0);
    for (cpu, gpu) in acoustics[0].iter().zip(&acoustics[1]) {
        assert_eq!(cpu.input, gpu.input);
        assert_eq!(cpu.paths, gpu.paths, "support paths changed");
        assert!(
            (cpu.late_energy_db - gpu.late_energy_db).abs() < 1e-6,
            "late-energy change"
        );
        assert!(
            (cpu.worst_band_db - gpu.worst_band_db).abs() < 1e-6,
            "band-guard change"
        );
        assert!(
            (cpu.modal_tail_guard - gpu.modal_tail_guard).abs() < 1e-7,
            "modal-tail guard change"
        );
        if cpu.worst_band_db <= 1.0 + 1e-9 {
            assert!(
                gpu.worst_band_db <= 1.0 + 1e-9,
                "GPU violated a passing band guard"
            );
        }
        if cpu.modal_tail_guard <= 0.15 + 1e-9 {
            assert!(
                gpu.modal_tail_guard <= 0.15 + 1e-9,
                "GPU violated a passing modal-tail guard"
            );
        }
    }
    if !fir {
        assert_eq!(
            gpu.compute.backend,
            secs_types::compute::ComputeBackend::Cpu
        );
        assert_eq!(
            gpu.compute.reason,
            Some(secs_types::compute::ComputeReason::IirReference)
        );
        let mut comparable = gpu.clone();
        comparable.compute = cpu.compute.clone();
        assert_eq!(format!("{comparable:?}"), format!("{cpu:?}"));
    } else if requested == ComputePreference::Gpu || name == "big_fir" {
        assert!(
            gpu.compute.accelerated_systems > 0,
            "no actual CUDA systems"
        );
    } else {
        assert_eq!(
            gpu.compute.reason,
            Some(secs_types::compute::ComputeReason::SmallProblem)
        );
        assert_eq!(gpu.compute.accelerated_systems, 0);
    }
}

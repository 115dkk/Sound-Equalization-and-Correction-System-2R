//! Release export benchmark; requires SECS_PERF_OUTPUT for disposable files.
#![forbid(unsafe_code)]
use std::{hint::black_box, path::PathBuf, time::Instant};
fn main() {
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let args: Vec<_> = std::env::args().collect();
    let op = &args[1];
    let name = &args[2];
    let folder =
        PathBuf::from(std::env::var("SECS_PERF_OUTPUT").expect("disposable export directory"));
    let threads = std::env::var("SECS_PERF_THREADS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap().get());
    let case = secs_engine::test_support::production_case(name, threads);
    let settings = serde_json::from_value(case.settings).unwrap();
    let run = secs_export::ExportRun {
        design: &case.design,
        measurements: &case.measurements,
        config: &case.config,
        settings: &settings,
    };
    let labels = secs_camilla::RunLabels {
        mode: serde_json::to_value(settings.mode)
            .unwrap()
            .as_str()
            .unwrap()
            .into(),
        ultra_ms: settings.ultra_ms,
        latency_ms: case.design.chain.latency_ms,
        rc_hi_hz: case.design.rc_hi,
    };
    let call = || match op.as_str() {
        "profile_camilla" => {
            let start = Instant::now();
            let config = secs_camilla::make_config(
                &case.design.chain,
                &folder,
                &folder.join("configs"),
                "secs_mixed_",
                &secs_camilla::Options::default(),
            )
            .unwrap();
            let construct = start.elapsed().as_secs_f64();
            let start = Instant::now();
            std::fs::create_dir_all(folder.join("coeffs")).unwrap();
            std::fs::create_dir_all(folder.join("configs")).unwrap();
            for (path, data) in &config.coeffs {
                std::fs::write(path, data).unwrap();
            }
            std::fs::write(folder.join("configs/profile.yml"), &config.yaml).unwrap();
            println!(
                "{}",
                serde_json::json!({"construct_s":construct,"writes_s":start.elapsed().as_secs_f64()})
            );
        }
        "camilla" => {
            black_box(secs_camilla::export(&case.design.chain, &folder, &labels).unwrap());
        }
        "mdat_staged" => {
            black_box(secs_export::export_mdat(&run, &folder, None).unwrap());
        }
        "mdat_before_after" => {
            black_box(secs_export::export_before_after_mdat(&run, &folder, None).unwrap());
        }
        _ => panic!("unknown export"),
    };
    call();
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        call();
        samples.push(start.elapsed().as_secs_f64());
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"implementation":"rust", "op":op, "case":name, "samples_s":samples, "median_s":sorted[2]})
    );
}

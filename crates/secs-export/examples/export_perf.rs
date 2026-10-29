//! Release export benchmark; requires SECS_PERF_OUTPUT for disposable files.
#![forbid(unsafe_code)]
use secs_camilla as camilla;
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
    let case = secs_engine_testkit::production_case(name, threads);
    let settings = serde_json::from_value(case.settings).unwrap();
    let run = secs_export::ExportRun {
        design: &case.design,
        settings: &settings,
    };
    let labels = secs_camilla::RunLabels {
        mode: camilla::note::mode_name(settings.mode, settings.ultra_ms),
        note: camilla::note::camilla_note(
            settings.mode,
            case.design.chain.latency_ms,
            case.design.rc_hi,
            None,
        ),
        latency_ms: case.design.chain.latency_ms,
    };
    if op == "profile_camilla" {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&case.design.chain).unwrap();
        println!(
            "{}",
            serde_json::json!({"chain_sha256":Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>(), "threads":threads})
        );
    }
    let call = || match op.as_str() {
        "profile_camilla" => {
            let start = Instant::now();
            let stages = std::cell::RefCell::new(Vec::new());
            let previous = std::cell::Cell::new(start);
            let options = camilla::Options {
                note: labels.note.clone(),
                latency_ms: Some(labels.latency_ms),
                mode: labels.mode.clone(),
                ..camilla::Options::default()
            };
            let config = camilla::make_config_observed(
                &case.design.chain,
                &folder,
                &folder.join("configs"),
                "secs_mixed_",
                &options,
                None,
                &|name| {
                    let now = Instant::now();
                    stages.borrow_mut().push((
                        name.to_owned(),
                        now.duration_since(previous.replace(now)).as_secs_f64(),
                    ));
                },
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
            let writes = start.elapsed().as_secs_f64();
            println!(
                "{}",
                serde_json::json!({"construct_s":construct,"stages":stages.into_inner(),"writes_s":writes,
                    "yaml_bytes":config.yaml.len(),"coeff_bytes":config.coeffs.iter().map(|(_, b)| b.len()).sum::<usize>()})
            );
        }
        "camilla" => {
            black_box(secs_camilla::export(&case.design.chain, &folder, &labels, None).unwrap());
        }
        "mdat_staged" => {
            black_box(secs_export::export_mdat(&run, &folder, None, 0).unwrap());
        }
        "mdat_before_after" => {
            black_box(secs_export::export_before_after_mdat(&run, &folder, None, None, 0).unwrap());
        }
        _ => panic!("unknown export"),
    };
    call();
    let mut samples = Vec::new();
    let runs = std::env::var("SECS_PERF_RUNS")
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(31);
    for _ in 0..runs {
        let start = Instant::now();
        call();
        samples.push(start.elapsed().as_secs_f64());
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"implementation":"rust", "op":op, "case":name, "samples_s":samples, "median_s":sorted[sorted.len() / 2]})
    );
}

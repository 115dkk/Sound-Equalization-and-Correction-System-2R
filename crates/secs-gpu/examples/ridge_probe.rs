//! Release hardware audit of a recorded production ideal-FIR problem.
#![forbid(unsafe_code)]
use secs_compute::CancelToken;
use secs_engine::mimo::{CpuRidgeBackend, RidgeBackend, RidgeReplayInput, replay_ridge};
use secs_types::compute::ComputePreference;
use std::{path::PathBuf, time::Instant};

fn main() {
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let folder = PathBuf::from(std::env::args().nth(1).expect("recorded problem directory"));
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("meta.json")).unwrap()).unwrap();
    let read = |name: &str| {
        let bytes = std::fs::read(folder.join(format!("{name}.f64"))).unwrap();
        assert_eq!(bytes.len() % 8, 0);
        bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| f64::from_le_bytes(*b))
            .collect::<Vec<_>>()
    };
    let mut ata = read("ata");
    let mut atb = read("atb");
    let lp = read("lp");
    let direct = read("direct");
    let original_taps = meta["taps"].as_u64().unwrap() as usize;
    let original_roles = atb.len() / original_taps;
    let taps: usize = std::env::var("SECS_GPU_PROBE_TAPS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(original_taps);
    let roles: usize = std::env::var("SECS_GPU_PROBE_ROLES")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(original_roles);
    assert!(taps > 0 && taps <= original_taps && roles > 0 && roles <= original_roles);
    if taps != original_taps || roles != original_roles {
        let indices: Vec<_> = (0..roles)
            .flat_map(|r| (0..taps).map(move |k| r * original_taps + k))
            .collect();
        let dimension = atb.len();
        ata = indices
            .iter()
            .flat_map(|&i| indices.iter().map(move |&j| (i, j)))
            .map(|(i, j)| ata[i * dimension + j])
            .collect();
        atb = indices.iter().map(|&i| atb[i]).collect();
    }
    let selection = secs_gpu::select(ComputePreference::Gpu);
    println!("{}", serde_json::json!({"status": selection.status}));
    let gpu = selection.backend.expect("usable CUDA device/runtime");
    let cpu = CpuRidgeBackend;
    let input = RidgeReplayInput {
        ata: &ata,
        atb: &atb,
        lp: &lp,
        direct: &direct,
        taps,
        level_db: meta["level_db"].as_f64().unwrap(),
    };
    let runs: usize = std::env::var("SECS_PERF_RUNS")
        .unwrap_or("5".into())
        .parse()
        .unwrap();
    let mut reference = None;
    for (name, backend) in [("cpu", &cpu as &dyn RidgeBackend), ("cuda", &*gpu)] {
        let mut times = Vec::new();
        for sample in 0..=runs {
            let started = Instant::now();
            let out = replay_ridge(&input, backend, &CancelToken::new()).unwrap();
            let elapsed = started.elapsed().as_secs_f64();
            let expected = reference.get_or_insert_with(|| (out.u.clone(), out.trajectory.clone()));
            let relative = out
                .u
                .iter()
                .zip(&expected.0)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt()
                / expected
                    .0
                    .iter()
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt()
                    .max(1e-30);
            let branches_equal = out
                .trajectory
                .iter()
                .zip(&expected.1)
                .all(|(a, b)| a.0.to_bits() == b.0.to_bits() && a.2 == b.2);
            let ratio_error = out
                .trajectory
                .iter()
                .zip(&expected.1)
                .map(|(a, b)| (a.1 - b.1).abs())
                .fold(0.0, f64::max);
            if sample > 0 {
                times.push(elapsed);
            }
            println!(
                "{}",
                serde_json::json!({"backend":name,"sample":sample,"warmup":sample==0,"elapsed_s":elapsed,"selected_log_lambda":out.log_lambda,"selected_ratio":out.ratio,"selected_u_relative_l2":relative,"branches_equal":branches_equal,"max_ratio_absolute_error":ratio_error})
            );
            assert!(relative < 1e-6, "selected solution error {relative}");
            assert!(branches_equal, "lambda branches changed");
            assert!(ratio_error < 1e-8, "support-energy error {ratio_error}");
        }
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"backend":name,"dimension":atb.len(),"roles":roles,"taps":taps,"samples_s":times,"median_s":sorted[sorted.len()/2]})
        );
    }
}

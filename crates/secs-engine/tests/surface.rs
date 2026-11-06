#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn digest_f32(values: &[f64]) -> String {
    let values = values.iter().map(|&value| value as f32).collect::<Vec<_>>();
    digest_samples(&values)
}

fn digest_samples(values: &[f32]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.to_le_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn design_methods_reproduce_the_values_secs_export_reads() {
    let replay = secs_engine_testkit::replay_case("dolby");
    let design = replay.design;

    assert_eq!(design.config().main_position, 1);
    assert_eq!(design.main_position(), 1);
    assert_eq!(design.rc_hi().to_bits(), 500.0_f64.to_bits());
    assert_eq!(design.latency_ms().to_bits(), 8.0_f64.to_bits());
    assert_eq!(
        design.main_roles().collect::<Vec<_>>(),
        [
            "L", "R", "C", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr"
        ]
    );
    assert_eq!(
        digest_f32(design.fir("L").expect("Dolby L FIR")),
        "30692b161039d653f5a007f6469e06784ba57b2038de30c3841d652982f01421"
    );
    assert_eq!(
        design.sub_roles().collect::<Vec<_>>(),
        ["SW1", "SW2", "SW3"]
    );
    assert_eq!(
        design
            .measured(1, "L")
            .expect("Dolby reference measurement")
            .sample_rate,
        48_000
    );
    let stages = design.stage("L", 1).expect("Dolby L stages");
    let (samples, t0) = design.export_ir(&stages.output, secs_engine::ExportAxis::Corrected);
    assert_eq!(t0.to_bits(), (-0.005125325520833356_f64).to_bits());
    assert_eq!(samples.len(), 131_072);
}

fn public_items(source: &str) -> BTreeSet<String> {
    let source = source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join(" ");
    let mut items = BTreeSet::new();
    for statement in source.split(';').map(str::trim) {
        if let Some(rest) = statement.strip_prefix("pub mod ") {
            items.insert(format!("mod {}", rest.trim()));
        } else if let Some(rest) = statement.strip_prefix("pub use ") {
            if let Some((_, names)) = rest.split_once("::{") {
                for name in names.trim_end_matches('}').split(',').map(str::trim) {
                    if !name.is_empty() {
                        items.insert(format!("use {name}"));
                    }
                }
            } else if let Some(name) = rest.rsplit("::").next() {
                items.insert(format!("use {}", name.trim()));
            }
        }
    }
    items
}

#[test]
#[ignore = "ADR 0046 step 4"]
fn public_surface_is_the_adr_0046_list() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("engine lib.rs");
    let expected = BTreeSet::from([
        "mod input".to_owned(),
        "mod preview".to_owned(),
        "mod xo".to_owned(),
        "use AccelerationSelection".to_owned(),
        "use Calibration".to_owned(),
        "use CalibrationRequest".to_owned(),
        "use Design".to_owned(),
        "use EngineConfig".to_owned(),
        "use EngineError".to_owned(),
        "use ExportAxis".to_owned(),
        "use GuardCause".to_owned(),
        "use Measurement".to_owned(),
        "use MeasurementSet".to_owned(),
        "use Metrics".to_owned(),
        "use MimoKey".to_owned(),
        "use MimoNote".to_owned(),
        "use MimoRun".to_owned(),
        "use Note".to_owned(),
        "use NoteRecord".to_owned(),
        "use NoteRecords".to_owned(),
        "use ProjectView".to_owned(),
        "use RoleLateEnergy".to_owned(),
        "use Samples".to_owned(),
        "use StageResponses".to_owned(),
        "use SubFilter".to_owned(),
        "use Tuning".to_owned(),
        "use availability".to_owned(),
        "use calibrate".to_owned(),
        "use compute_with_acceleration".to_owned(),
        "use late_energy".to_owned(),
    ]);
    assert_eq!(public_items(&source), expected);
}

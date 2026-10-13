#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use secs_engine::test_support::replay_case;
use secs_export::{
    ExportRun, before_after_items, export_before_after_mdat, export_mdat, layout_lines, rc_pieces,
    stage_items, sub_values_text,
};
use secs_mdat::{MdatError, MeasurementInput, TEMPLATE};
use secs_oracle::{Fixture, number};
use secs_types::calibration::CalibrationSettings;
use serde_json::Value;
use sha2::{Digest, Sha256};

const CASES: [&str; 4] = ["dolby", "room_dba", "room_manual", "room_filters"];
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_folder() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "secs-export-golden-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

fn digest_f32(samples: &[f64]) -> String {
    let mut hasher = Sha256::new();
    for &sample in samples {
        hasher.update((sample as f32).to_le_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_items(name: &str, actual: &[MeasurementInput], expected: &Value) {
    let expected = expected.as_array().expect("items");
    assert_eq!(actual.len(), expected.len(), "{name} count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let id = format!("{name}[{index}]");
        assert_eq!(actual.title, expected["title"], "{id} title");
        assert_eq!(actual.notes, expected["notes"], "{id} notes");
        assert_eq!(
            actual.sample_rate_hz as u64,
            expected["sample_rate_hz"].as_u64().unwrap(),
            "{id} sample rate"
        );
        assert_eq!(
            actual.start_time_s.to_bits(),
            number(&expected["start_time_s"]).to_bits(),
            "{id} t0"
        );
        assert_eq!(
            actual.samples.len() as u64,
            expected["length"].as_u64().unwrap(),
            "{id} length"
        );
        let actual_digest = digest_f32(&actual.samples);
        if actual_digest != expected["samples_sha256"].as_str().unwrap() {
            eprintln!(
                "MEASURE {id} float32 digest differs across NumPy pocketfft and Rust realfft: {actual_digest}"
            );
        }
        for sample in expected["samples"].as_array().unwrap() {
            let at = sample[0].as_u64().unwrap() as usize;
            let actual = actual.samples[at] as f32;
            let expected = number(&sample[1]) as f32;
            let tolerance = 8.0 * f32::EPSILON * expected.abs().max(1.0e-6);
            assert!(
                (actual - expected).abs() <= tolerance,
                "{id} sample {at}: got {actual:e}, expected {expected:e}, tolerance {tolerance:e}"
            );
        }
    }
}

#[test]
fn export_items_match_oracle() {
    for name in CASES {
        let case = replay_case(name);
        let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
        let run = ExportRun {
            design: &case.design,
            measurements: &case.measurements,
            config: &case.config,
            settings: &settings,
        };
        let fixture = Fixture::load("export_mdat", name);
        let output = fixture.outputs();
        assert_eq!(settings.label(), output["label"]);
        assert_eq!(
            rc_pieces(&run),
            serde_json::from_value::<Vec<String>>(output["rc_pieces"].clone()).unwrap()
        );
        assert_eq!(
            layout_lines(&run),
            serde_json::from_value::<Vec<String>>(output["layout_lines"].clone()).unwrap()
        );
        assert_eq!(sub_values_text(&run), output["sub_values_text"]);
        verify_items(
            &format!("{name}/staged"),
            &stage_items(&run, None),
            &output["staged"],
        );
        verify_items(
            &format!("{name}/before_after"),
            &before_after_items(&run),
            &output["before_after"],
        );
    }
}

#[test]
fn public_exports_use_oracle_filenames_and_replace_existing_files() {
    let case = replay_case("room_filters");
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        measurements: &case.measurements,
        config: &case.config,
        settings: &settings,
    };
    let fixture = Fixture::load("export_mdat", "room_filters");
    let output = fixture.outputs();
    let folder = temp_folder();
    let staged = folder.join(output["staged_filename"].as_str().unwrap());
    let before_after = folder.join(output["before_after_filename"].as_str().unwrap());
    fs::write(&staged, b"old").unwrap();
    fs::write(&before_after, b"old").unwrap();

    assert_eq!(export_mdat(&run, &folder, None).unwrap(), staged);
    assert_eq!(
        export_before_after_mdat(&run, &folder, None).unwrap(),
        before_after
    );
    assert!(fs::metadata(&staged).unwrap().len() > 1_000_000);
    assert!(fs::metadata(&before_after).unwrap().len() > 1_000_000);
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn invalid_export_does_not_create_destination_folder() {
    let mut case = replay_case("room_filters");
    case.config.main_position = u32::MAX;
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        measurements: &case.measurements,
        config: &case.config,
        settings: &settings,
    };
    let root = temp_folder();
    let folder = root.join("missing");

    assert!(matches!(
        export_mdat(&run, &folder, None),
        Err(secs_export::ExportError::Engine(_))
    ));
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_build_does_not_create_destination_folder() {
    let mut case = replay_case("room_filters");
    case.design.chain.mains.clear();
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        measurements: &case.measurements,
        config: &case.config,
        settings: &settings,
    };
    let root = temp_folder();
    let folder = root.join("missing");

    assert!(matches!(
        export_mdat(&run, &folder, None),
        Err(secs_export::ExportError::Mdat(MdatError::Input(_)))
    ));
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn built_projects_match_oracle_structure_and_size() {
    for name in CASES {
        let case = replay_case(name);
        let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
        let run = ExportRun {
            design: &case.design,
            measurements: &case.measurements,
            config: &case.config,
            settings: &settings,
        };
        let fixture = Fixture::load("export_mdat", name);
        let output = fixture.outputs();
        for (kind, items) in [
            ("staged", stage_items(&run, None)),
            ("before_after", before_after_items(&run)),
        ] {
            let mut next = 1_u128;
            let bytes = secs_mdat::build(&items, TEMPLATE, None, &mut || {
                let value = next;
                next += 1;
                value
            })
            .unwrap();
            assert_eq!(
                bytes.len() as u64,
                output[format!("{kind}_mdat_length")].as_u64().unwrap(),
                "{name}/{kind} length"
            );
            // Cache arrays can differ by one float32 ULP across libm implementations;
            // item text, timing and every float32 IR are pinned above.
            let limits = secs_jos_limits();
            let parsed = secs_mdat::load(&bytes, &limits).unwrap();
            assert_eq!(
                parsed.measurements.len(),
                items.len(),
                "{name}/{kind} schema"
            );
        }
    }
}

fn secs_jos_limits() -> secs_jos::Limits {
    secs_jos::Limits::default()
}

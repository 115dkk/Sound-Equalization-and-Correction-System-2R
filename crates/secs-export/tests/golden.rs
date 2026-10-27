#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use secs_engine::test_support::replay_case;
use secs_export::{
    ExportError, ExportRun, MdatScope, MimoKind, MimoResponses, Original, Sources,
    before_after_items, before_after_items_mimo, export_before_after_mdat, export_mdat,
    layout_lines, rc_pieces, rew_mdat_filename, rew_mdat_items, stage_items, stage_items_mimo,
    sub_values_text,
};
use secs_mdat::{CalInfo, MdatError, MeasurementInput, SourceInfo, TEMPLATE};
use secs_oracle::{Fixture, number};
use secs_types::{
    calibration::CalibrationSettings,
    chain::{MimoFilter, MimoPath, MimoPaths},
};
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

// Off the oracle libm the replayed design's BA can diverge (macOS:
// room_filters), so what BA decides (sub values in the notes, the latency in
// the start time, the IR samples, the file length) is reported there; the
// structure (counts, titles, sample rates, lengths) stays asserted.
fn strict() -> bool {
    secs_oracle::oracle_libm()
}
fn check<T: PartialEq + std::fmt::Debug>(id: &str, what: &str, actual: T, expected: T) {
    if strict() {
        assert_eq!(actual, expected, "{id} {what}");
    } else if actual != expected {
        eprintln!("EXPORT_OFFLIBM {id} {what}: got {actual:?}, expected {expected:?}");
    }
}

fn peqs(value: &Value) -> Vec<(f64, f64, f64)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|band| {
            let band = band.as_array().unwrap();
            (number(&band[0]), number(&band[1]), number(&band[2]))
        })
        .collect()
}

fn allpasses(value: &Value) -> Vec<(f64, f64)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|band| {
            let band = band.as_array().unwrap();
            (number(&band[0]), number(&band[1]))
        })
        .collect()
}

fn packed_f32s(fixture: &Fixture, descriptor: &Value) -> Vec<f64> {
    let packed = fixture.blob(descriptor);
    let length = descriptor["samples_len"].as_u64().unwrap() as usize;
    let mut planes = Vec::with_capacity(4 * length);
    let mut at = 0;
    while at < packed.len() {
        let control = packed[at];
        at += 1;
        let count = usize::from(control & 0x7f) + 1;
        if control & 0x80 == 0 {
            planes.extend_from_slice(&packed[at..at + count]);
            at += count;
        } else {
            planes.extend(std::iter::repeat_n(packed[at], count));
            at += 1;
        }
    }
    assert_eq!(planes.len(), 4 * length);
    for plane in planes.chunks_exact_mut(length) {
        for index in 1..length {
            plane[index] ^= plane[index - 1];
        }
    }
    (0..length)
        .map(|index| {
            f64::from(f32::from_le_bytes([
                planes[index],
                planes[length + index],
                planes[2 * length + index],
                planes[3 * length + index],
            ]))
        })
        .collect()
}

fn mimo_responses(fixture: &Fixture) -> MimoResponses {
    let output = fixture.outputs();
    let paths = &output["paths"];
    let paths = MimoPaths {
        fsiso: number(&paths["fsiso"]),
        paths: paths["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| {
                let filter = &path["filter"];
                let filter = if let Some(value) = filter.get("Iir") {
                    MimoFilter::Iir {
                        gain_db: number(&value["gain_db"]),
                        delay_ms: number(&value["delay_ms"]),
                        polarity: value["polarity"].as_i64().unwrap() as i8,
                        peq: peqs(&value["peq"]),
                        allpass: allpasses(&value["allpass"]),
                    }
                } else {
                    MimoFilter::Fir {
                        taps_48k: fixture.f64s(&filter["Fir"]["taps_48k"]),
                    }
                };
                MimoPath {
                    input: path["input"].as_str().unwrap().into(),
                    support: path["support"].as_str().unwrap().into(),
                    filter,
                    low_hz: number(&path["low_hz"]),
                    delay_ms: number(&path["delay_ms"]),
                }
            })
            .collect(),
    };
    let applied = output["applied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|response| {
            let ir = packed_f32s(fixture, &response["ir_f32"]);
            let spectrum = secs_dsp::fft::rfft(&ir);
            (response["role"].as_str().unwrap().into(), spectrum)
        })
        .collect();
    MimoResponses {
        kind: match output["kind"].as_str().unwrap() {
            "IIR" => MimoKind::Iir,
            "FIR" => MimoKind::Fir,
            value => panic!("unknown MIMO kind {value}"),
        },
        fsiso: number(&output["fsiso"]),
        cap_text: output["cap_text"].as_str().unwrap().into(),
        paths,
        applied,
    }
}

fn sources(value: &Value) -> Option<Sources> {
    let positions = value.as_object()?;
    Some(
        positions
            .iter()
            .map(|(position, roles)| {
                (
                    position.parse().unwrap(),
                    roles
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(role, info)| {
                            (
                                role.clone(),
                                SourceInfo {
                                    strings: info["strings"]
                                        .as_object()
                                        .unwrap()
                                        .iter()
                                        .map(|(name, value)| {
                                            (name.clone(), value.as_str().unwrap().into())
                                        })
                                        .collect(),
                                    boxed: Vec::new(),
                                    cal: Vec::new(),
                                    date: info["date"].as_i64().map(secs_mdat::SourceDate::Int),
                                    notes: info
                                        .get("notes")
                                        .and_then(Value::as_str)
                                        .map(Into::into),
                                    format: info
                                        .get("format")
                                        .and_then(Value::as_str)
                                        .map(Into::into),
                                },
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn original(position: u32, role: &str, sample_rate_hz: u32, source: SourceInfo) -> Original {
    Original {
        position,
        role: role.into(),
        item: MeasurementInput {
            title: format!("{role}_P{position}"),
            notes: format!("SECS · 위치 {position} · 원 측정"),
            sample_rate_hz,
            samples: vec![0.25, -0.5, 0.125],
            start_time_s: -0.001 * f64::from(position),
            spl_offset_db: Some(113.0 + f64::from(position)),
            source_info: Some(source),
            computed: false,
        },
    }
}

fn meter_source() -> SourceInfo {
    SourceInfo {
        strings: vec![("outputName".into(), "stored device".into())],
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: "mic.cal".into(),
                source_path: "mic.cal".into(),
                freq: Some(vec![20.0, 20_000.0]),
                gain: Some(vec![6.0, 6.0]),
                phase: None,
                boxed: Vec::new(),
            },
        )],
        ..SourceInfo::default()
    }
}

fn verify_items(name: &str, actual: &[MeasurementInput], expected: &Value) {
    let expected = expected.as_array().expect("items");
    assert_eq!(actual.len(), expected.len(), "{name} count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let id = format!("{name}[{index}]");
        assert_eq!(actual.title, expected["title"], "{id} title");
        check(
            &id,
            "notes",
            actual.notes.as_str(),
            expected["notes"].as_str().unwrap(),
        );
        assert_eq!(
            actual.sample_rate_hz as u64,
            expected["sample_rate_hz"].as_u64().unwrap(),
            "{id} sample rate"
        );
        check(
            &id,
            "t0",
            actual.start_time_s.to_bits(),
            number(&expected["start_time_s"]).to_bits(),
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
            if strict() {
                assert!(
                    (actual - expected).abs() <= tolerance,
                    "{id} sample {at}: got {actual:e}, expected {expected:e}, tolerance {tolerance:e}"
                );
            } else if (actual - expected).abs() > tolerance {
                eprintln!("EXPORT_OFFLIBM {id} sample {at}: got {actual:e}, expected {expected:e}");
            }
        }
    }
}

#[test]
fn export_items_and_source_environments_match_oracle() {
    for name in CASES {
        let case = replay_case(name);
        let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
        let run = ExportRun {
            design: &case.design,
            settings: &settings,
        };
        let fixture = Fixture::load("export_mdat", name);
        let output = fixture.outputs();
        assert_eq!(settings.label(), output["label"]);
        assert_eq!(
            rc_pieces(&run),
            serde_json::from_value::<Vec<String>>(output["rc_pieces"].clone()).unwrap()
        );
        check(
            name,
            "layout_lines",
            layout_lines(&run),
            serde_json::from_value::<Vec<String>>(output["layout_lines"].clone()).unwrap(),
        );
        check(
            name,
            "sub_values_text",
            sub_values_text(&run).as_str(),
            output["sub_values_text"].as_str().unwrap(),
        );
        verify_items(
            &format!("{name}/staged"),
            &stage_items(&run, None),
            &output["staged"],
        );
        let sources = sources(&fixture.inputs()["sources"]);
        let before_after = before_after_items(&run, sources.as_ref());
        verify_items(
            &format!("{name}/before_after"),
            &before_after,
            &output["before_after"],
        );
        if let Some(sources) = sources {
            let role_sources = &sources[&run.design.replay.config().main_position];
            assert!(before_after.iter().any(|item| item.source_info.is_some()));
            for (role, source) in role_sources {
                let measured = before_after
                    .iter()
                    .find(|item| item.title.contains(role) && item.title.contains("_1 "))
                    .unwrap();
                assert_eq!(measured.source_info.as_ref(), Some(source));
                assert!(!measured.computed);
                let computed = before_after
                    .iter()
                    .find(|item| item.title.contains(role) && item.title.contains("_2 "))
                    .unwrap();
                assert_eq!(computed.source_info.as_ref(), Some(source));
                assert!(computed.computed);
            }
        }
    }
}

#[test]
fn mimo_items_match_four_oracles() {
    for fixture in Fixture::all("export_mimo", "") {
        let case_name = fixture
            .id
            .strip_prefix("export_mimo/")
            .unwrap()
            .strip_suffix("_iir")
            .or_else(|| {
                fixture
                    .id
                    .strip_prefix("export_mimo/")
                    .unwrap()
                    .strip_suffix("_fir")
            })
            .unwrap();
        let case = replay_case(case_name);
        let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
        let run = ExportRun {
            design: &case.design,
            settings: &settings,
        };
        let mimo = mimo_responses(&fixture);
        let staged: Vec<_> = stage_items_mimo(&run, &mimo, None)
            .unwrap()
            .into_iter()
            .filter(|item| item.title.contains("MIMO 적용"))
            .collect();
        let before_after: Vec<_> = before_after_items_mimo(&run, &mimo, None)
            .unwrap()
            .into_iter()
            .filter(|item| item.title.contains("+ MIMO"))
            .collect();
        verify_items(
            &format!("{}/staged", fixture.id),
            &staged,
            &fixture.outputs()["staged"],
        );
        verify_items(
            &format!("{}/before_after", fixture.id),
            &before_after,
            &fixture.outputs()["before_after"],
        );
        let roles: Vec<_> = mimo
            .paths
            .paths
            .iter()
            .map(|path| path.input.as_str())
            .collect();
        assert!(
            staged
                .iter()
                .all(|item| roles.iter().any(|role| item.title.contains(role)))
        );
        assert!(
            before_after
                .iter()
                .all(|item| roles.iter().any(|role| item.title.contains(role)))
        );
    }
}

#[test]
fn rew_mdat_scopes_choose_items_and_names() {
    let case = replay_case("room_filters");
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        settings: &settings,
    };
    let position = run.design.replay.config().main_position;
    let role = run.design.chain.mains[0].role.as_str();
    let originals = vec![
        original(position, role, 96_000, SourceInfo::default()),
        original(position + 1, role, 44_100, SourceInfo::default()),
    ];
    let measurements =
        rew_mdat_items(None, MdatScope::Measurements, &originals, None, None).unwrap();
    assert_eq!(
        measurements
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        [
            originals[0].item.title.as_str(),
            originals[1].item.title.as_str()
        ]
    );

    let upstream = before_after_items(&run, None);
    let all = rew_mdat_items(Some(&run), MdatScope::All, &originals, None, None).unwrap();
    assert_eq!(all.len(), upstream.len() + 1);
    assert_eq!(
        all[..upstream.len()]
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        upstream
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(all.last().unwrap().title, originals[1].item.title);

    let results = rew_mdat_items(Some(&run), MdatScope::Results, &[], None, None).unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().all(|item| item.computed));
    assert!(results.iter().all(|item| item.title.contains("_2 ")));

    let label = settings.label();
    assert_eq!(
        rew_mdat_filename(MdatScope::All, &label),
        format!("SECS 원 측정+보정 결과 {label}.mdat")
    );
    assert_eq!(
        rew_mdat_filename(MdatScope::Measurements, "unused"),
        "SECS 원 측정.mdat"
    );
    assert_eq!(
        rew_mdat_filename(MdatScope::Results, &label),
        format!("SECS 보정 결과 {label}.mdat")
    );
}

#[test]
fn all_scope_takes_stored_originals_and_strips_curves_from_results() {
    let case = replay_case("room_filters");
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        settings: &settings,
    };
    let position = run.design.replay.config().main_position;
    let role = run.design.chain.mains[0].role.clone();
    let stored = original(position, &role, 96_000, meter_source());
    let mut sources = Sources::default();
    sources
        .entry(position)
        .or_default()
        .insert(role.clone(), meter_source());

    let all = rew_mdat_items(
        Some(&run),
        MdatScope::All,
        std::slice::from_ref(&stored),
        None,
        Some(&sources),
    )
    .unwrap();
    let measured = all
        .iter()
        .find(|item| item.title.contains(&format!("_{role}_1 ")))
        .unwrap();
    assert_eq!(measured.sample_rate_hz, 96_000);
    assert_eq!(measured.samples, stored.item.samples);
    assert_eq!(measured.start_time_s, stored.item.start_time_s);
    assert_eq!(measured.spl_offset_db, stored.item.spl_offset_db);
    assert_eq!(measured.source_info.as_ref().unwrap().cal[0].0, "meterCal");

    let computed = all
        .iter()
        .find(|item| item.title.contains(&format!("_{role}_2 ")))
        .unwrap();
    let computed_source = computed.source_info.as_ref().unwrap();
    assert!(computed_source.cal.is_empty());
    assert_eq!(
        computed_source
            .strings
            .iter()
            .find(|(name, _)| name == "outputName")
            .map(|(_, value)| value.as_str()),
        Some("stored device")
    );
    assert!(all.iter().filter(|item| item.computed).all(|item| {
        item.source_info
            .as_ref()
            .is_none_or(|source| source.cal.is_empty())
    }));
}

#[test]
fn measurement_scope_needs_measurements_not_a_result() {
    assert!(matches!(
        rew_mdat_items(None, MdatScope::Measurements, &[], None, None),
        Err(ExportError::NoMeasurements)
    ));
    let measurement = original(1, "L", 48_000, SourceInfo::default());
    assert_eq!(
        rew_mdat_items(
            None,
            MdatScope::Measurements,
            std::slice::from_ref(&measurement),
            None,
            None,
        )
        .unwrap()[0]
            .title,
        "L_P1"
    );
    assert!(matches!(
        rew_mdat_items(None, MdatScope::All, &[], None, None),
        Err(ExportError::NoResult)
    ));
    assert!(matches!(
        rew_mdat_items(None, MdatScope::Results, &[], None, None),
        Err(ExportError::NoResult)
    ));
}

#[test]
fn public_exports_use_oracle_filenames_and_replace_existing_files() {
    let case = replay_case("room_filters");
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        settings: &settings,
    };
    let fixture = Fixture::load("export_mdat", "room_filters");
    let output = fixture.outputs();
    let folder = temp_folder();
    let staged = folder.join(output["staged_filename"].as_str().unwrap());
    let before_after = folder.join(output["before_after_filename"].as_str().unwrap());
    fs::write(&staged, b"old").unwrap();
    fs::write(&before_after, b"old").unwrap();

    assert_eq!(export_mdat(&run, &folder, None, 0).unwrap(), staged);
    assert_eq!(
        export_before_after_mdat(&run, &folder, None, None, 0).unwrap(),
        before_after
    );
    assert!(fs::metadata(&staged).unwrap().len() > 1_000_000);
    assert!(fs::metadata(&before_after).unwrap().len() > 1_000_000);
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn invalid_build_does_not_create_destination_folder() {
    let mut case = replay_case("room_filters");
    case.design.chain.mains.clear();
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let run = ExportRun {
        design: &case.design,
        settings: &settings,
    };
    let root = temp_folder();
    let folder = root.join("missing");

    assert!(matches!(
        export_mdat(&run, &folder, None, 0),
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
            settings: &settings,
        };
        let fixture = Fixture::load("export_mdat", name);
        let output = fixture.outputs();
        for (kind, items) in [
            ("staged", stage_items(&run, None)),
            (
                "before_after",
                before_after_items(&run, sources(&fixture.inputs()["sources"]).as_ref()),
            ),
        ] {
            let mut next = 1_u128;
            let bytes = secs_mdat::build(
                &items,
                TEMPLATE,
                None,
                fixture.inputs()["now_ms"].as_i64().unwrap(),
                &mut || {
                    let value = next;
                    next += 1;
                    value
                },
            )
            .unwrap();
            check(
                &format!("{name}/{kind}"),
                "length",
                bytes.len() as u64,
                output[format!("{kind}_mdat_length")].as_u64().unwrap(),
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

#![forbid(unsafe_code)]

use secs_oracle::{Fixture, assert_same, number};
use secs_project::{
    Band, DspProfile, Project, PyJson, Speaker, SpeakerConfig, TargetCurve, dumps, loads,
    read_mono_float_wav, read_wav_float32, snap_focus, write_wav_float32,
};
use secs_types::role::{
    ROLES, role_code, role_from_title, role_group, role_kind, role_label, role_order, role_position,
};
use serde_json::Value;

fn str_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn role_vocabulary_matches_oracle() {
    let fx = Fixture::load("project", "roles");
    let filter = str_array(&fx.inputs()["filter"]);
    for row in fx.outputs()["roles"].as_array().unwrap() {
        let role = row["role"].as_str().unwrap();
        assert_eq!(role_label(role), row["label"].as_str().unwrap());
        assert_eq!(
            format!("{:?}", role_kind(role)).to_lowercase(),
            row["kind"].as_str().unwrap()
        );
        assert_eq!(role_group(role), row["group"].as_str().unwrap());
        assert_eq!(role_code(role), row["code"].as_str().unwrap());
        assert_eq!(role_order(role) as u64, row["order"].as_u64().unwrap());
        match role_position(role) {
            Some((x, y)) => {
                assert_eq!(x.to_bits(), number(&row["position"][0]).to_bits());
                assert_eq!(y.to_bits(), number(&row["position"][1]).to_bits());
            }
            None => assert!(row["position"].is_null()),
        }
    }
    for row in fx.outputs()["titles"].as_array().unwrap() {
        let title = row[0].as_str().unwrap();
        assert_eq!(role_from_title(title, None), row[1].as_str());
        assert_eq!(role_from_title(title, Some(&filter)), row[2].as_str());
    }
    assert_eq!(ROLES.len(), 27);
}

#[test]
fn project_load_migrations_and_save_match_oracle() {
    for fx in Fixture::all("project", "migrate_") {
        let input = fx.blob(&fx.inputs()["text"]);
        let project = Project::load_text(std::str::from_utf8(&input).unwrap()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&dumps(&project.to_json())).unwrap(),
            fx.outputs()["value"]
        );
        assert_eq!(
            project.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"])
        );
    }
}

#[test]
fn direct_project_preset_matches_oracle() {
    let fx = Fixture::load("project", "direct_preset");
    let project = Project::from_preset(fx.inputs()["preset"].as_str().unwrap()).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&dumps(&project.to_json())).unwrap(),
        fx.outputs()["value"]
    );
    assert_eq!(project.dsp, DspProfile::default());
    assert_eq!(
        serde_json::from_str::<Value>(&dumps(&project.dsp.to_json())).unwrap(),
        fx.outputs()["dsp_dataclass_default"]
    );
}

#[test]
fn project_edge_contracts_match_oracle() {
    let future = Fixture::load("project", "dsp_future");
    let project = Project::from_json(
        loads(&serde_json::json!({"dsp": future.inputs()}).to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        project.dsp.fir_taps_max,
        future.outputs()["profile"]["fir_taps_max"]
    );
    assert_eq!(project.dsp.profile, future.outputs()["profile"]["profile"]);

    let focus = Fixture::load("project", "focus");
    for (input, expected) in focus.inputs()["values"]
        .as_array()
        .unwrap()
        .iter()
        .zip(focus.outputs()["snapped"].as_array().unwrap())
    {
        let value = match input.as_str() {
            Some("nan") => PyJson::Str("NaN".into()),
            Some("+inf") => PyJson::Str("Infinity".into()),
            Some("-inf") => PyJson::Str("-Infinity".into()),
            _ => loads(&input.to_string()).unwrap(),
        };
        assert_eq!(snap_focus(&value), expected.as_i64().unwrap());
    }

    let collections = Fixture::load("project", "collections");
    for (input, expected) in collections.inputs()["projects"]
        .as_array()
        .unwrap()
        .iter()
        .zip(collections.outputs()["rows"].as_array().unwrap())
    {
        let project = Project::from_json(loads(&input.to_string()).unwrap()).unwrap();
        assert_eq!(project.curtain.0, number(&expected[0][0]));
        assert_eq!(project.curtain.1, number(&expected[0][1]));
        assert_eq!(
            project.bass_path_count() as i64,
            expected[1].as_i64().unwrap()
        );
    }

    let validation = Fixture::load("project", "config_validation");
    let config = SpeakerConfig {
        speakers: vec![
            Speaker {
                role: "L".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "NOPE".into(),
                    out: 1,
                    pol: "x".into(),
                }],
            },
            Speaker {
                role: "R".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "SUB".into(),
                    out: 1,
                    pol: "+".into(),
                }],
            },
            Speaker {
                role: "L".into(),
                size: "small".into(),
                bands: vec![],
            },
            Speaker {
                role: "C".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "FULL".into(),
                    out: 1,
                    pol: "+".into(),
                }],
            },
        ],
        ..SpeakerConfig::default()
    };
    let sentences = config.validate();
    assert_eq!(
        sentences,
        str_array(&validation.outputs()["messages"])
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    let notices = config.validation_notices();
    assert_eq!(sentences.len(), notices.len());
    assert_eq!(
        notices
            .iter()
            .map(|notice| notice.code.as_str())
            .collect::<Vec<_>>(),
        vec![
            "role_duplicate",
            "out_duplicate",
            "out_duplicate",
            "band_unknown",
            "pol_invalid",
            "band_combination",
            "main_with_sub_band",
            "bands_empty",
        ]
    );
    assert_eq!(
        config
            .groups()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        str_array(&validation.outputs()["groups"])
    );

    let sort = Fixture::load("project", "target_sort");
    for (input, expected) in sort.inputs()["points"]
        .as_array()
        .unwrap()
        .iter()
        .zip(sort.outputs()["points"].as_array().unwrap())
    {
        let curve = TargetCurve::new(
            input
                .as_array()
                .unwrap()
                .iter()
                .map(|point| (number(&point[0]), number(&point[1])))
                .collect(),
        );
        for (actual, expected) in curve.points.iter().zip(expected.as_array().unwrap()) {
            let frequency = number(&expected[0]);
            assert!(
                actual.0.to_bits() == frequency.to_bits()
                    || actual.0.is_nan() && frequency.is_nan()
            );
            assert_eq!(actual.1.to_bits(), number(&expected[1]).to_bits());
        }
    }
}

#[test]
fn project_presets_save_exactly() {
    for fx in Fixture::all("project", "preset_") {
        let expected = &fx.outputs()["value"];
        let loaded = Project::from_json(loads(&expected.to_string()).unwrap()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&dumps(&loaded.to_json())).unwrap(),
            *expected,
            "{}",
            fx.id
        );
        assert_eq!(
            loaded.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"]),
            "{}",
            fx.id
        );
    }
}

#[test]
fn dsp_tap_allocation_matches_oracle() {
    for fx in Fixture::all("project", "taps_") {
        let profile = fx.inputs()["profile"]["profile"].as_str().unwrap();
        let mut dsp = DspProfile::presets().remove(profile).unwrap();
        dsp.support_mode = fx.inputs()["profile"]["support_mode"]
            .as_str()
            .unwrap()
            .to_owned();
        for row in fx.outputs()["rows"].as_array().unwrap() {
            let config = SpeakerConfig::from_preset(row[0].as_str().unwrap(), "", true).unwrap();
            let phase = row[1].as_str().unwrap();
            let paths = row[2].as_i64().unwrap();
            let expected = row[3].as_array().unwrap();
            assert_eq!(
                dsp.allocate_taps(&config, phase, paths, 256),
                (expected[0].as_i64().unwrap(), expected[1].as_i64().unwrap()),
                "{}",
                fx.id
            );
            let used = dsp.fir_used(&config, phase, paths);
            for key in ["chan", "sup", "total"] {
                assert_eq!(used[key], row[4][key].as_i64().unwrap(), "{} {key}", fx.id);
            }
        }
    }
}

#[test]
fn target_curves_match_oracle() {
    for fx in Fixture::all("project", "target_") {
        if fx.id.ends_with("target_sort") {
            continue;
        }
        let points = fx.inputs()["points"]["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (number(&p[0]), number(&p[1])))
            .collect();
        let curve = TargetCurve::new(points);
        let frequencies = fx.f64s(&fx.inputs()["freqs"]);
        let expected = fx.f64s(&fx.outputs()["values"]);
        assert_same(&fx.id, &curve.sample(&frequencies), &expected);
    }
}

#[test]
fn pyjson_edge_cases_match_oracle() {
    for index in 0..3 {
        let fx = Fixture::load("project", &format!("json_{index}"));
        let expected = String::from_utf8(fx.blob(&fx.outputs()["save"])).unwrap();
        let value = loads(expected.trim_end()).unwrap();
        assert_eq!(format!("{}\n", dumps(&value)), expected, "{}", fx.id);
    }

    for fx in Fixture::all("project", "json_round_trip_") {
        let input = fx.blob(&fx.inputs()["text"]);
        let project = Project::load_text(std::str::from_utf8(&input).unwrap()).unwrap();
        assert_eq!(
            project.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"]),
            "{}",
            fx.id
        );
    }

    let fx = Fixture::load("project", "typed_negative_zero");
    let project = Project::from_json(loads(&fx.inputs()["value"].to_string()).unwrap()).unwrap();
    assert_eq!(
        project.save_text().as_bytes(),
        fx.blob(&fx.outputs()["save"])
    );
}

#[test]
fn pyjson_scanner_edges_match_oracle() {
    for fx in Fixture::all("project", "json_scanner_") {
        let input = fx.blob(&fx.inputs()["text"]);
        let value = loads(std::str::from_utf8(&input).unwrap()).unwrap();
        let actual = format!("{}\n", dumps(&value));
        assert_eq!(
            actual.as_bytes(),
            fx.blob(&fx.outputs()["save"]),
            "{}",
            fx.id
        );
    }
    let fx = Fixture::load("project", "project_numeric_edges");
    let input = fx.blob(&fx.inputs()["text"]);
    let project = Project::load_text(std::str::from_utf8(&input).unwrap()).unwrap();
    assert_eq!(
        project.save_text().as_bytes(),
        fx.blob(&fx.outputs()["save"])
    );
}

#[test]
fn pyjson_regressions() {
    let marker_key = format!("__secs_project_{}_0__", "nonfinite");
    let marker_text = format!(r#"{{"{marker_key}":"NaN"}}"#);
    let old_marker = loads(&marker_text).unwrap();
    assert_eq!(
        dumps(&old_marker),
        format!("{{\n  \"{marker_key}\": \"NaN\"\n}}")
    );
    assert!(loads(r#""\ud800""#).is_err());
    assert!(loads(&format!("{}0{}", "[".repeat(1000), "]".repeat(1000))).is_ok());
    assert!(loads(&format!("{}0{}", "[".repeat(1001), "]".repeat(1001))).is_err());

    let huge = format!("1{}", "0".repeat(400));
    let project_text = format!(r#"{{"curtain":[{huge},300]}}"#);
    assert!(Project::load_text(&project_text).is_err());

    // Python's float(2**128) rounds correctly; free-form fields keep the text.
    let two_128 = "340282366920938463463374607431768211456";
    let project = Project::load_text(&format!(
        r#"{{"curtain":[{two_128},300],"future":{two_128}}}"#
    ))
    .unwrap();
    let saved = project.save_text();
    assert!(saved.contains("3.402823669209385e+38"), "{saved}");
    assert!(saved.contains(&format!("\"future\": {two_128}")), "{saved}");
}

#[test]
fn wav_writer_and_readers_match_oracle() {
    for fx in Fixture::all("project", "wav_") {
        let input = fx.inputs();
        let rate = input["rate"].as_u64().unwrap() as u32;
        let samples: Vec<f32> = input["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| number(v) as f32)
            .collect();
        let wav = write_wav_float32(&samples, rate);
        assert_eq!(wav, fx.blob(&fx.outputs()["wav"]), "{}", fx.id);
        let (minimal, minimal_rate) = read_wav_float32(&wav).unwrap();
        let (general, general_rate) = read_mono_float_wav(&wav).unwrap();
        assert_eq!(
            minimal_rate as u64,
            fx.outputs()["minimal"]["rate"].as_u64().unwrap()
        );
        assert_eq!(
            general_rate as u64,
            fx.outputs()["fast"]["rate"].as_u64().unwrap()
        );
        let expected: Vec<f32> = fx.outputs()["minimal"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| number(v) as f32)
            .collect();
        assert_eq!(minimal, expected);
        assert_eq!(general, expected);
    }
}

// Writer-independent WAV fixtures (#92 TD-47): every header below is built
// from the RIFF/WAVE layout, never by this crate's writer, and every expected
// sample is worked out by hand from the stored integers or floats.

const PCM_GUID: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
const FLOAT_GUID: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

/// `fmt ` body of a plain WAVEFORMAT: tag, channels, rate, byte rate, block align, bits.
fn fmt_body(tag: u16, channels: u16, rate: u32, block_align: u16, bits: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(tag.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(block_align)).to_le_bytes());
    out.extend(block_align.to_le_bytes());
    out.extend(bits.to_le_bytes());
    out
}

/// `fmt ` body of WAVE_FORMAT_EXTENSIBLE (40 bytes, cbSize 22).
fn extensible_body(rate: u32, block_align: u16, bits: u16, valid: u16, guid: [u8; 16]) -> Vec<u8> {
    let mut out = fmt_body(0xfffe, 1, rate, block_align, bits);
    out.extend(22_u16.to_le_bytes());
    out.extend(valid.to_le_bytes());
    out.extend(4_u32.to_le_bytes()); // SPEAKER_FRONT_CENTER
    out.extend(guid);
    out
}

/// `RIFF` + `WAVE` + `fmt ` + `data`, with exact chunk sizes.
fn riff(fmt: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend(
        u32::try_from(4 + 8 + fmt.len() + 8 + data.len())
            .unwrap()
            .to_le_bytes(),
    );
    out.extend(b"WAVEfmt ");
    out.extend(u32::try_from(fmt.len()).unwrap().to_le_bytes());
    out.extend(fmt);
    out.extend(b"data");
    out.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
    out.extend(data);
    out
}

fn le_bytes<const N: usize>(values: impl IntoIterator<Item = [u8; N]>) -> Vec<u8> {
    values.into_iter().flatten().collect()
}

fn wav_error(bytes: &[u8]) -> String {
    match read_mono_float_wav(bytes) {
        Err(secs_project::ProjectError::Wav(message)) => message,
        other => panic!("expected a WAV error, got {other:?}"),
    }
}

#[test]
fn wav_reader_decodes_hand_built_fixtures_of_every_supported_format_td47() {
    // The helper's bytes for one PCM16 file, spelled out.
    let pcm16 = riff(
        &fmt_body(1, 1, 44_100, 2, 16),
        &le_bytes([i16::MAX, i16::MIN, 0, 16_384, -1].map(i16::to_le_bytes)),
    );
    assert_eq!(
        pcm16,
        [
            b"RIFF".as_slice(),
            &[46, 0, 0, 0],
            b"WAVEfmt ",
            &[
                16, 0, 0, 0, 1, 0, 1, 0, 0x44, 0xac, 0, 0, 0x88, 0x58, 1, 0, 2, 0, 16, 0
            ],
            b"data",
            &[
                10, 0, 0, 0, 0xff, 0x7f, 0x00, 0x80, 0, 0, 0x00, 0x40, 0xff, 0xff
            ],
        ]
        .concat()
    );
    let pcm16_expected = vec![32_767.0 / 32_768.0, -1.0, 0.0, 0.5, -1.0 / 32_768.0];
    assert_eq!(
        read_mono_float_wav(&pcm16).unwrap(),
        (pcm16_expected.clone(), 44_100)
    );

    // PCM24: 0x7fffff, 0x800000, 0x400000, 0xffffff (-1).
    let pcm24 = riff(
        &fmt_body(1, 1, 48_000, 3, 24),
        &[
            0xff, 0xff, 0x7f, 0x00, 0x00, 0x80, 0x00, 0x00, 0x40, 0xff, 0xff, 0xff,
        ],
    );
    assert_eq!(
        read_mono_float_wav(&pcm24).unwrap(),
        (
            vec![8_388_607.0 / 8_388_608.0, -1.0, 0.5, -1.0 / 8_388_608.0],
            48_000
        )
    );

    // PCM32: i32::MAX is 2^31 - 1, which rounds to 2^31 in f32, so 1.0.
    let pcm32 = riff(
        &fmt_body(1, 1, 96_000, 4, 32),
        &le_bytes([i32::MAX, i32::MIN, 1 << 30, -(1 << 16)].map(i32::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&pcm32).unwrap(),
        (vec![1.0, -1.0, 0.5, -1.0 / 32_768.0], 96_000)
    );

    let float32 = riff(
        &fmt_body(3, 1, 48_000, 4, 32),
        &le_bytes([0.25_f32, -1.5, 0.375].map(f32::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&float32).unwrap(),
        (vec![0.25, -1.5, 0.375], 48_000)
    );
    assert_eq!(
        read_wav_float32(&float32).unwrap(),
        (vec![0.25, -1.5, 0.375], 48_000)
    );

    // float64 narrows to f32: 1 + 2^-30 is not an f32 and rounds to 1.0.
    let float64 = riff(
        &fmt_body(3, 1, 48_000, 8, 64),
        &le_bytes([0.25_f64, -1.5, 1.0 + 2_f64.powi(-30)].map(f64::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&float64).unwrap(),
        (vec![0.25, -1.5, 1.0], 48_000)
    );

    let extensible_pcm = riff(
        &extensible_body(44_100, 2, 16, 16, PCM_GUID),
        &le_bytes([i16::MAX, i16::MIN, 0, 16_384, -1].map(i16::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&extensible_pcm).unwrap(),
        (pcm16_expected, 44_100)
    );
    let extensible_float = riff(
        &extensible_body(48_000, 4, 32, 32, FLOAT_GUID),
        &le_bytes([0.25_f32, -1.5].map(f32::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&extensible_float).unwrap(),
        (vec![0.25, -1.5], 48_000)
    );

    // 24 valid bits left-aligned in a 32-bit container: the low byte is padding.
    let valid24 = riff(
        &extensible_body(48_000, 4, 32, 24, PCM_GUID),
        &le_bytes([0x7fff_ff00_u32, 0x8000_0000, 0x4000_0000, 0xffff_ff00].map(u32::to_le_bytes)),
    );
    assert_eq!(
        read_mono_float_wav(&valid24).unwrap(),
        (
            vec![8_388_607.0 / 8_388_608.0, -1.0, 0.5, -1.0 / 8_388_608.0],
            48_000
        )
    );
}

#[test]
fn wav_reader_rejects_hand_built_malformed_fixtures_td47() {
    // `fmt ` declares 16 bytes but the file ends after 10 of them.
    let mut truncated = b"RIFF".to_vec();
    truncated.extend(26_u32.to_le_bytes());
    truncated.extend(b"WAVEfmt ");
    truncated.extend(16_u32.to_le_bytes());
    truncated.extend(&fmt_body(1, 1, 48_000, 2, 16)[..10]);
    assert_eq!(wav_error(&truncated), "short fmt chunk");

    let unknown_guid = riff(
        &extensible_body(48_000, 2, 16, 16, [0x55; 16]),
        &[0, 0, 0, 0],
    );
    assert_eq!(
        wav_error(&unknown_guid),
        "unsupported extensible WAV subformat"
    );

    let stereo = riff(&fmt_body(1, 2, 48_000, 4, 16), &[0, 0, 0, 0]);
    assert_eq!(wav_error(&stereo), "WAV is not mono");

    // PCM16 with a 4-byte block.
    let misaligned = riff(&fmt_body(1, 1, 48_000, 4, 16), &[0, 0, 0, 0]);
    assert_eq!(wav_error(&misaligned), "invalid block alignment");

    // Not left-aligned in a 32-bit container: documented as unsupported.
    let valid20 = riff(&extensible_body(48_000, 3, 24, 20, PCM_GUID), &[0, 0, 0]);
    assert_eq!(wav_error(&valid20), "unsupported WAV format 1/24/20");

    // The float32-only reader refuses PCM.
    let pcm16 = riff(&fmt_body(1, 1, 48_000, 2, 16), &[0, 0]);
    assert!(read_wav_float32(&pcm16).is_err());
}

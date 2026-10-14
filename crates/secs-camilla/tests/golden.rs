#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use secs_camilla::pyfmt::{format_fixed, format_g, repr_f64, round_ndigits};
use secs_camilla::{
    CamillaError, Device, DeviceValue, Options, RunLabels, export, make_config, write_config,
};
use secs_oracle::{Fixture, goldens};
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("secs_camilla_{label}_{}_{id}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
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

fn band(value: &Value) -> BandType {
    BandType::from_name(value.as_str().unwrap())
}

fn mimo_paths(fixture: &Fixture) -> MimoPaths {
    let value = &fixture.outputs()["paths"];
    MimoPaths {
        fsiso: number(&value["fsiso"]),
        paths: value["paths"]
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
    }
}

fn chain(fixture: &Fixture) -> SignalChain {
    let value = &fixture.inputs()["chain"];
    let mains = value["mains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|main| MainChannel {
            role: main["role"].as_str().unwrap().into(),
            input_peq: peqs(&main["input_peq"]),
            fir: fixture.f64s(&main["fir"]),
            gain_db: number(&main["gain_db"]),
            delay_ms: number(&main["delay_ms"]),
            xo_hz: main["xo_hz"].as_f64(),
            top_peq: peqs(&main["top_peq"]),
            align_delay_ms: number(&main["align_delay_ms"]),
        })
        .collect();
    let subs = value["subs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sub| SubChannel {
            role: sub["role"].as_str().unwrap().into(),
            gain_db: number(&sub["gain_db"]),
            delay_ms: number(&sub["delay_ms"]),
            polarity: sub["polarity"].as_i64().unwrap() as i8,
            allpass: allpasses(&sub["allpass"]),
            hp_hz: number(&sub["hp_hz"]),
            hp_type: band(&sub["hp_type"]),
            lp_hz: number(&sub["lp_hz"]),
            lp_type: band(&sub["lp_type"]),
            peq: peqs(&sub["peq"]),
            mode: sub["mode"].as_str().unwrap().into(),
        })
        .collect();
    SignalChain {
        sample_rate: value["sample_rate"].as_u64().unwrap() as u32,
        mains,
        subs,
        sub_bus_peq: peqs(&value["sub_bus_peq"]),
        main_extra_ms: number(&value["main_extra_ms"]),
        lift_ms: number(&value["lift_ms"]),
        latency_ms: number(&value["latency_ms"]),
    }
}

fn device(value: &Value) -> Device {
    Device(
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                let value = if let Some(value) = value.as_str() {
                    DeviceValue::Str(value.into())
                } else if let Some(value) = value.as_bool() {
                    DeviceValue::Bool(value)
                } else if let Some(value) = value.as_i64() {
                    DeviceValue::Int(value)
                } else {
                    DeviceValue::Float(value.as_f64().unwrap())
                };
                (key.clone(), value)
            })
            .collect(),
    )
}

fn direct_options(fixture: &Fixture) -> Options {
    let values = &fixture.inputs()["options"];
    let mut options = Options::default();
    if let Some(value) = values.get("capture") {
        options.capture = Some(device(value));
    } else {
        options.capture = Some(Device(vec![
            ("type".into(), DeviceValue::Str("RawFile".into())),
            (
                "channels".into(),
                DeviceValue::Int(
                    fixture.inputs()["chain"]["mains"].as_array().unwrap().len() as i64
                ),
            ),
        ]));
    }
    if let Some(value) = values.get("playback") {
        options.playback = Some(device(value));
    } else {
        let chain = &fixture.inputs()["chain"];
        options.playback = Some(Device(vec![
            ("type".into(), DeviceValue::Str("File".into())),
            (
                "channels".into(),
                DeviceValue::Int(
                    (chain["mains"].as_array().unwrap().len()
                        + chain["subs"].as_array().unwrap().len()) as i64,
                ),
            ),
        ]));
    }
    if let Some(value) = values.get("chunksize") {
        options.chunksize = value.as_u64().unwrap() as u32;
    }
    if let Some(value) = values.get("note") {
        options.note = value.as_str().unwrap().into();
    }
    if let Some(value) = values.get("latency_ms") {
        options.latency_ms = Some(number(value));
    }
    if let Some(value) = values.get("mode") {
        options.mode = value.as_str().unwrap().into();
    }
    if values.get("lfe_gain_db").is_some_and(Value::is_null) {
        options.lfe_gain_db = None;
    } else if let Some(value) = values.get("lfe_gain_db") {
        options.lfe_gain_db = Some(number(value));
    }
    if let Some(value) = values.get("lfe_lp_hz") {
        options.lfe_lp_hz = number(value);
    }
    options
}

fn labels(fixture: &Fixture) -> RunLabels {
    let labels = &fixture.inputs()["labels"];
    RunLabels {
        mode: labels["mode"].as_str().unwrap().into(),
        ultra_ms: number(&labels["ultra_ms"]),
        latency_ms: number(&labels["latency_ms"]),
        rc_hi_hz: number(&labels["rc_hi_hz"]),
        mimo_max_supports: labels["mimo_max_supports"].as_u64().unwrap_or(0) as usize,
    }
}

fn expected_blob(fixture: &Fixture, descriptor: &Value) -> Vec<u8> {
    let name = descriptor["blob"].as_str().unwrap();
    fs::read(goldens().join("camilla").join(name))
        .unwrap_or_else(|error| panic!("{} {name}: {error}", fixture.id))
}

fn replace_folder(text: &str, folder: &Path) -> String {
    text.replace(&folder.to_string_lossy().replace('\\', "/"), "{OUT}")
}

#[test]
fn pyfmt_matches_oracle_edges() {
    let fixture = Fixture::load("camilla", "pyfmt");
    for (value, expected) in fixture.inputs()["values"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture.outputs().as_array().unwrap())
    {
        let value = number(value);
        assert_eq!(repr_f64(value), expected["repr"].as_str().unwrap());
        assert_eq!(
            repr_f64(round_ndigits(value, 3)),
            expected["round3"].as_str().unwrap()
        );
        assert_eq!(
            repr_f64(round_ndigits(value, 4)),
            expected["round4"].as_str().unwrap()
        );
        assert_eq!(
            repr_f64(round_ndigits(value, 5)),
            expected["round5"].as_str().unwrap()
        );
        assert_eq!(format_g(value, true), expected["plus_g"].as_str().unwrap());
        assert_eq!(format_g(value, false), expected["g"].as_str().unwrap());
        assert_eq!(
            format_fixed(value, 1, false),
            expected["dot1"].as_str().unwrap()
        );
        assert_eq!(
            format_fixed(value, 3, false),
            expected["dot3"].as_str().unwrap()
        );
        assert_eq!(
            format_fixed(value, 2, true),
            expected["plus2"].as_str().unwrap()
        );
        assert_eq!(
            format_fixed(value, 3, true),
            expected["plus3"].as_str().unwrap()
        );
    }
}

#[test]
fn make_config_matches_oracle() {
    for fixture in Fixture::all("camilla", "direct_") {
        let temp = TempDir::new(&fixture.id.replace('/', "_"));
        let options = direct_options(&fixture);
        let config_dir = temp.0.join("where/configs");
        let config = make_config(
            &chain(&fixture),
            &temp.0,
            &config_dir,
            "edge_",
            &options,
            None,
        )
        .unwrap();
        assert_eq!(
            replace_folder(&config.yaml, &temp.0),
            fixture.outputs()["yaml"].as_str().unwrap(),
            "{}",
            fixture.id
        );
        let expected = fixture.outputs()["coeffs"].as_array().unwrap();
        assert_eq!(config.coeffs.len(), expected.len());
        for ((path, bytes), expected) in config.coeffs.iter().zip(expected) {
            assert_eq!(
                path.strip_prefix(&temp.0)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                expected["path"]
            );
            assert_eq!(*bytes, expected_blob(&fixture, &expected["bytes"]));
        }
    }
}

#[test]
fn export_camilladsp_matches_oracle() {
    for fixture in Fixture::all("camilla", "").into_iter().filter(|fixture| {
        !fixture.id.contains("/direct_")
            && !fixture.id.ends_with("/pyfmt")
            && !fixture.id.ends_with("/write_name_nonascii")
    }) {
        let temp = TempDir::new(&fixture.id.replace('/', "_"));
        let labels = labels(&fixture);
        let paths = export(&chain(&fixture), &temp.0, &labels, None).unwrap();
        let got: Vec<_> = paths
            .iter()
            .map(|path| {
                path.strip_prefix(&temp.0)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        let expected: Vec<_> = fixture.outputs()["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(got, expected, "{}", fixture.id);
        assert_eq!(
            replace_folder(&fs::read_to_string(&paths[0]).unwrap(), &temp.0),
            fixture.outputs()["yaml"].as_str().unwrap(),
            "{}",
            fixture.id
        );
        for expected in fixture.outputs()["coeffs"].as_array().unwrap() {
            let path = temp.0.join(expected["path"].as_str().unwrap());
            assert_eq!(
                fs::read(path).unwrap(),
                expected_blob(&fixture, &expected["bytes"]),
                "{}",
                fixture.id
            );
        }
        assert_eq!(
            fs::metadata(temp.0.join("in.raw")).unwrap().len(),
            fixture.outputs()["in_raw_bytes"].as_u64().unwrap()
        );
    }
}

fn wav_f32(path: &Path) -> (u32, Vec<f32>) {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 32);
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let length = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 44 + length);
    let samples = bytes[44..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| f32::from_le_bytes(*value))
        .collect();
    (rate, samples)
}

#[test]
fn mimo_camilladsp_matches_four_oracles() {
    for fixture in Fixture::all("export_mimo", "") {
        let temp = TempDir::new(&fixture.id.replace('/', "_"));
        let mimo = mimo_paths(&fixture);
        let written = export(&chain(&fixture), &temp.0, &labels(&fixture), Some(&mimo)).unwrap();
        assert_eq!(
            replace_folder(&fs::read_to_string(&written[0]).unwrap(), &temp.0),
            fixture.outputs()["yaml"].as_str().unwrap(),
            "{}",
            fixture.id
        );

        for expected in fixture.outputs()["wavs"].as_array().unwrap() {
            let path = temp.0.join(expected["path"].as_str().unwrap());
            let (rate, samples) = wav_f32(&path);
            assert_eq!(rate as u64, expected["sample_rate_hz"].as_u64().unwrap());
            assert_eq!(samples.len() as u64, expected["frames"].as_u64().unwrap());
            let relative = expected["path"].as_str().unwrap();
            let taps = mimo
                .paths
                .iter()
                .find_map(|mimo_path| {
                    if relative.ends_with(&format!(
                        "mimo_{}_{}.wav",
                        mimo_path.input, mimo_path.support
                    )) {
                        Some(match &mimo_path.filter {
                            MimoFilter::Fir { taps_48k } => taps_48k,
                            MimoFilter::Iir { .. } => unreachable!("MIMO WAV for IIR path"),
                        })
                    } else {
                        None
                    }
                })
                .expect("MIMO WAV path");
            assert_eq!(samples.len(), taps.len());
            for (index, (&actual, &expected)) in samples.iter().zip(taps).enumerate() {
                assert_eq!(
                    actual.to_bits(),
                    (expected as f32).to_bits(),
                    "{} {relative} frame {index}",
                    fixture.id
                );
            }
            for sample in expected["samples"].as_array().unwrap() {
                let index = sample[0].as_u64().unwrap() as usize;
                assert_eq!(
                    samples[index].to_bits(),
                    (number(&sample[1]) as f32).to_bits()
                );
            }
        }
    }
}

#[test]
fn empty_mimo_is_byte_identical_to_none() {
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("empty_mimo");
    let options = direct_options(&fixture);
    let empty = MimoPaths {
        fsiso: 150.0,
        paths: Vec::new(),
    };
    let none = make_config(&chain(&fixture), &temp.0, &temp.0, "", &options, None).unwrap();
    let some = make_config(
        &chain(&fixture),
        &temp.0,
        &temp.0,
        "",
        &options,
        Some(&empty),
    )
    .unwrap();
    assert_eq!(some, none);
}

// Upstream `calibration.export_camilladsp` adds ", max N per input" to the
// head note when the run capped supports; the oracle cases run uncapped.
#[test]
fn mimo_note_names_the_support_cap() {
    let fixture = Fixture::all("export_mimo", "")
        .into_iter()
        .next()
        .expect("export_mimo fixture");
    let mimo = mimo_paths(&fixture);
    let n = mimo.paths.len();
    for (cap, tail) in [(0, ")".to_owned()), (4, ", max 4 per input)".to_owned())] {
        let temp = TempDir::new(&format!("mimo_cap_{cap}"));
        let mut labels = labels(&fixture);
        labels.mimo_max_supports = cap;
        let written = export(&chain(&fixture), &temp.0, &labels, Some(&mimo)).unwrap();
        let yaml = fs::read_to_string(&written[0]).unwrap();
        let fsiso = secs_camilla::pyfmt::format_g(mimo.fsiso, false);
        assert!(
            yaml.contains(&format!("support paths {n} (Fsiso {fsiso} Hz{tail}")),
            "cap {cap}"
        );
    }
}

#[test]
fn mimo_yaml_preserves_delay_filter_order_and_fir_shape() {
    for fixture in Fixture::all("export_mimo", "") {
        let temp = TempDir::new(&format!("mimo_shape_{}", fixture.id.replace('/', "_")));
        let mimo = mimo_paths(&fixture);
        let config = make_config(
            &chain(&fixture),
            &temp.0,
            &temp.0.join("configs"),
            "secs_",
            &Options::default(),
            Some(&mimo),
        )
        .unwrap();
        for path in &mimo.paths {
            let tag = format!("mimo_{}_{}", path.input, path.support);
            match &path.filter {
                MimoFilter::Iir { delay_ms, .. } => {
                    let samples = ((path.delay_ms + delay_ms) * 48.0).round() as i64;
                    assert!(config.yaml.contains(&format!(
                        "  {tag}_delay: {{\"type\": \"Delay\", \"parameters\": {{\"delay\": {samples},"
                    )));
                    if let Some(sub) = chain(&fixture)
                        .subs
                        .into_iter()
                        .find(|sub| sub.role == path.support && !sub.peq.is_empty())
                    {
                        assert!(
                            config
                                .yaml
                                .contains(&format!("\"names\": [\"{tag}_gain\", \"{tag}_delay\""))
                        );
                        assert!(config.yaml.contains(&format!(
                            "\"sub_{}_fixed_peq{}\"]}}",
                            sub.role,
                            sub.peq.len() - 1
                        )));
                    }
                }
                MimoFilter::Fir { taps_48k } => {
                    assert_eq!(taps_48k.len(), 8000);
                    assert!(!config.yaml.contains(&format!("{tag}_peq0")));
                    assert!(config.yaml.contains(&format!(
                        "\"names\": [\"{tag}_delay\", \"{tag}_hp\", \"{tag}_fir\""
                    )));
                }
            }
        }
    }
}

#[test]
fn mimo_delay_is_filter_plus_support_alignment_plus_main_extra() {
    let fixture = Fixture::load("export_mimo", "room_dba_iir");
    let mut chain = chain(&fixture);
    chain.main_extra_ms = 1.7;
    let align_sw1 = number(&fixture.inputs()["align_delay_ms"]["SW1"]);
    let mut path = mimo_paths(&fixture)
        .paths
        .into_iter()
        .find(|path| path.input == "L" && path.support == "SW1")
        .unwrap();
    path.delay_ms = align_sw1 + chain.main_extra_ms;
    path.filter = MimoFilter::Iir {
        gain_db: -6.0,
        delay_ms: 3.0,
        polarity: 1,
        peq: Vec::new(),
        allpass: Vec::new(),
    };
    let mimo = MimoPaths {
        fsiso: 120.0,
        paths: vec![path],
    };
    let temp = TempDir::new("mimo_delay_formula");
    let config = make_config(
        &chain,
        &temp.0,
        &temp.0.join("configs"),
        "secs_",
        &Options::default(),
        Some(&mimo),
    )
    .unwrap();
    let expected = ((3.0 + align_sw1 + chain.main_extra_ms) * 48.0).round() as i64;
    assert!(config.yaml.contains(&format!(
        "  mimo_L_SW1_delay: {{\"type\": \"Delay\", \"parameters\": {{\"delay\": {expected},"
    )));
}

#[test]
fn write_config_sanitizes_nonascii_name_like_oracle() {
    let fixture = Fixture::load("camilla", "write_name_nonascii");
    let temp = TempDir::new("nonascii_name");
    let options = Options {
        note: fixture.inputs()["options"]["note"].as_str().unwrap().into(),
        ..Options::default()
    };
    let paths = write_config(
        &chain(&fixture),
        &temp.0,
        fixture.inputs()["name"].as_str().unwrap(),
        &options,
        None,
    )
    .unwrap();
    let got: Vec<_> = paths
        .iter()
        .map(|path| {
            path.strip_prefix(&temp.0)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let expected: Vec<_> = fixture.outputs()["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(got, expected);
    assert_eq!(
        replace_folder(&fs::read_to_string(&paths[0]).unwrap(), &temp.0),
        fixture.outputs()["yaml"].as_str().unwrap()
    );
    for expected in fixture.outputs()["coeffs"].as_array().unwrap() {
        assert_eq!(
            fs::read(temp.0.join(expected["path"].as_str().unwrap())).unwrap(),
            expected_blob(&fixture, &expected["bytes"])
        );
    }
}

#[test]
fn write_config_replaces_existing_files_without_leaving_temporary_files() {
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("replace_existing");
    let first = write_config(
        &chain(&fixture),
        &temp.0,
        "replace",
        &Options::default(),
        None,
    )
    .unwrap();
    fs::write(&first[0], b"old").unwrap();

    let second = write_config(
        &chain(&fixture),
        &temp.0,
        "replace",
        &Options::default(),
        None,
    )
    .unwrap();

    assert_eq!(first, second);
    assert_ne!(fs::read(&second[0]).unwrap(), b"old");
    assert!(
        fs::read_dir(second[0].parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
    );
}

#[test]
fn invalid_values_write_nothing() {
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("errors");
    let mut bad = chain(&fixture);
    bad.mains[0].gain_db = f64::NAN;
    assert!(matches!(
        write_config(&bad, &temp.0, "bad", &Options::default(), None),
        Err(CamillaError::NonFinite(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    let mut bad = chain(&fixture);
    bad.mains[0].role = "왼쪽".into();
    assert!(matches!(
        write_config(&bad, &temp.0, "bad", &Options::default(), None),
        Err(CamillaError::NonAscii(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    let options = Options {
        mode: "bad\nmode".into(),
        ..Options::default()
    };
    assert!(matches!(
        write_config(&chain(&fixture), &temp.0, "bad", &options, None),
        Err(CamillaError::Invalid(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    assert!(matches!(
        write_config(&chain(&fixture), &temp.0, "..", &Options::default(), None,),
        Err(CamillaError::Invalid(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

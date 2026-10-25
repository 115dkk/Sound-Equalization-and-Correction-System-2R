#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod support;

use secs_camilla::pyfmt::{format_fixed, format_g, repr_f64, round_ndigits};
use secs_camilla::{
    CamillaError, Device, DeviceValue, Options, RunLabels, export, make_config, write_config,
    write_config_observed,
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

const FREQUENCIES: [f64; 11] = [
    7.0, 20.0, 31.0, 43.0, 80.0, 120.0, 173.0, 350.0, 1000.0, 8765.0, 20_000.0,
];

fn yaml_head(yaml: &str) -> &str {
    yaml.split_once("\nfilters:\n").unwrap().0
}

fn assert_yaml_transfer(
    id: &str,
    got: &str,
    expected: &str,
    got_wav: &dyn Fn(&str) -> Vec<u8>,
    expected_wav: &dyn Fn(&str) -> Vec<u8>,
) {
    assert_eq!(yaml_head(got), yaml_head(expected), "{id} YAML head");
    let input_count = |yaml: &str| {
        serde_json::from_str::<Value>(
            yaml.lines()
                .find_map(|line| line.strip_prefix("  route_in: "))
                .unwrap(),
        )
        .unwrap()["channels"]["in"]
            .as_u64()
            .unwrap() as usize
    };
    let inputs = input_count(expected);
    assert_eq!(input_count(got), inputs, "{id} input channels");
    let mut max_expected = 0.0_f64;
    let convolution_paths = |yaml: &str| {
        yaml_filters(yaml)
            .into_values()
            .filter(|filter| filter["type"] == "Conv")
            .map(|filter| {
                filter["parameters"]["filename"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        convolution_paths(got),
        convolution_paths(expected),
        "{id} convolution paths"
    );
    for input in 0..inputs {
        for frequency in FREQUENCIES {
            let got = support::camilla_graph::transfer(got, got_wav, input, frequency);
            let expected =
                support::camilla_graph::transfer(expected, expected_wav, input, frequency);
            assert_eq!(got.len(), expected.len(), "{id} input {input}");
            for (output, (got, expected)) in got.iter().zip(&expected).enumerate() {
                max_expected = max_expected.max(expected.norm());
                assert!(
                    (*got - *expected).norm() <= 2e-12 * expected.norm().max(1.0),
                    "{id} input {input} output {output} frequency {frequency}: {got} != {expected}"
                );
            }
        }
    }
    assert!(
        max_expected >= 1e-3,
        "{id}: expected Camilla responses are all silent ({max_expected})"
    );
}

fn yaml_filters(yaml: &str) -> std::collections::BTreeMap<String, Value> {
    yaml_section(yaml, "filters:")
}

fn yaml_mixers(yaml: &str) -> std::collections::BTreeMap<String, Value> {
    yaml_section(yaml, "mixers:")
}

fn yaml_section(yaml: &str, wanted: &str) -> std::collections::BTreeMap<String, Value> {
    let mut section = "";
    let mut result = std::collections::BTreeMap::new();
    for line in yaml.lines() {
        if ["filters:", "mixers:", "pipeline:"].contains(&line) {
            section = line;
        } else if section == wanted
            && let Some(row) = line.strip_prefix("  ")
            && let Some((name, json)) = row.split_once(": ")
        {
            result.insert(name.into(), serde_json::from_str(json).unwrap());
        }
    }
    result
}

fn yaml_pipeline(yaml: &str) -> Vec<Value> {
    let mut section = "";
    let mut result = Vec::new();
    for line in yaml.lines() {
        if ["filters:", "mixers:", "pipeline:"].contains(&line) {
            section = line;
        } else if section == "pipeline:"
            && let Some(row) = line.strip_prefix("  - ")
        {
            result.push(serde_json::from_str(row).unwrap());
        }
    }
    result
}

fn filter_delay(filters: &std::collections::BTreeMap<String, Value>, name: &str) -> Option<i64> {
    filters.get(name).map(|filter| {
        filter["parameters"]["delay"]
            .as_i64()
            .or_else(|| {
                filter["parameters"]["delay"]
                    .as_f64()
                    .map(|value| value as i64)
            })
            .unwrap()
    })
}

fn support_group_delay_for(yaml: &str, tag: &str) -> i64 {
    let filters = yaml_filters(yaml);
    let mixers = yaml_mixers(yaml);
    let pipeline = yaml_pipeline(yaml);
    let path_channel = pipeline
        .iter()
        .find(|step| {
            step.get("names").is_some_and(|names| {
                names
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| name == &format!("{tag}_fir") || name == &format!("{tag}_gain"))
            })
        })
        .unwrap()["channels"][0]
        .as_u64()
        .unwrap();
    let group_channel = mixers["mimo_groups"]["mapping"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["sources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|source| source["channel"].as_u64() == Some(path_channel))
        })
        .unwrap()["dest"]
        .as_u64()
        .unwrap();
    let names = pipeline
        .iter()
        .find(|step| {
            step.get("channels").is_some_and(|channels| {
                channels
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|channel| channel.as_u64() == Some(group_channel))
            }) && step.get("names").is_some_and(|names| {
                names
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| name.as_str().unwrap().starts_with("mimo_group"))
            })
        })
        .unwrap()["names"]
        .as_array()
        .unwrap();
    names
        .iter()
        .filter_map(|name| {
            let name = name.as_str().unwrap();
            name.ends_with("_delay")
                .then(|| filter_delay(&filters, name).unwrap())
        })
        .next()
        .unwrap_or(0)
}

fn config_wav<'a>(coeffs: &'a [(PathBuf, Vec<u8>)]) -> impl Fn(&str) -> Vec<u8> + 'a {
    move |name| {
        coeffs
            .iter()
            .find(|(path, _)| path.file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("missing generated WAV {name}"))
            .1
            .clone()
    }
}

fn fixture_camilla_wav<'a>(fixture: &'a Fixture) -> impl Fn(&str) -> Vec<u8> + 'a {
    move |name| {
        let descriptor = fixture.outputs()["coeffs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| {
                Path::new(item["path"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    == name
            })
            .unwrap_or_else(|| panic!("missing fixture WAV {name}"));
        expected_blob(fixture, &descriptor["bytes"])
    }
}

fn mimo_fixture_wav(fixture: &Fixture, mimo: &MimoPaths, name: &str) -> Vec<u8> {
    if let Some(main) = chain(fixture)
        .mains
        .into_iter()
        .find(|main| name.ends_with(&format!("{}_fir.wav", main.role)))
    {
        return secs_camilla::wav_bytes(&main.fir, 48_000).unwrap();
    }
    let path = mimo
        .paths
        .iter()
        .find(|path| name.ends_with(&format!("mimo_{}_{}.wav", path.input, path.support)))
        .unwrap_or_else(|| panic!("missing MIMO fixture WAV {name}"));
    match &path.filter {
        MimoFilter::Fir { taps_48k } => secs_camilla::wav_bytes(taps_48k, 48_000).unwrap(),
        MimoFilter::FirLowRate { taps_low, fs_low } => {
            secs_camilla::wav_bytes(&secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low), 48_000)
                .unwrap()
        }
        MimoFilter::Iir { .. } => panic!("IIR MIMO path has no WAV {name}"),
    }
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
        let got_yaml = replace_folder(&config.yaml, &temp.0);
        let expected_yaml = fixture.outputs()["yaml"].as_str().unwrap();
        assert_yaml_transfer(
            &fixture.id,
            &got_yaml,
            expected_yaml,
            &config_wav(&config.coeffs),
            &fixture_camilla_wav(&fixture),
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
        let got_yaml = replace_folder(&fs::read_to_string(&paths[0]).unwrap(), &temp.0);
        let expected_yaml = fixture.outputs()["yaml"].as_str().unwrap();
        assert_yaml_transfer(
            &fixture.id,
            &got_yaml,
            expected_yaml,
            &|name| fs::read(temp.0.join("coeffs").join(name)).unwrap(),
            &fixture_camilla_wav(&fixture),
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
        let got_yaml = replace_folder(&fs::read_to_string(&written[0]).unwrap(), &temp.0);
        let expected_yaml = fixture.outputs()["yaml"].as_str().unwrap();
        assert_yaml_transfer(
            &fixture.id,
            &got_yaml,
            expected_yaml,
            &|name| fs::read(temp.0.join("coeffs").join(name)).unwrap(),
            &|name| mimo_fixture_wav(&fixture, &mimo, name),
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
                            MimoFilter::FirLowRate { .. } => {
                                unreachable!("legacy golden fixtures store full-rate FIRs")
                            }
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
        let filters = yaml_filters(&config.yaml);
        for path in &mimo.paths {
            let tag = format!("mimo_{}_{}", path.input, path.support);
            let extra = match &path.filter {
                MimoFilter::Iir { delay_ms, .. } => *delay_ms,
                _ => 0.0,
            };
            let expected = secs_camilla::round_ties_even_i64((path.delay_ms + extra) * 48.0);
            let residual = filter_delay(&filters, &format!("{tag}_delay")).unwrap_or(0);
            let group_delay = support_group_delay_for(&config.yaml, &tag);
            assert_eq!(group_delay + residual, expected, "{} {tag}", fixture.id);
            match &path.filter {
                MimoFilter::Iir { .. } => {
                    assert!(!filters.contains_key(&format!("{tag}_hp")));
                    assert!(!filters.contains_key(&format!("{tag}_lp")));
                }
                MimoFilter::FirLowRate { .. } => {
                    unreachable!("legacy golden fixtures store full-rate FIRs")
                }
                MimoFilter::Fir { taps_48k } => {
                    assert_eq!(taps_48k.len(), 8000);
                    assert!(
                        !filters
                            .keys()
                            .any(|name| name.starts_with(&format!("{tag}_peq")))
                    );
                    assert!(filters.contains_key(&format!("{tag}_fir")));
                    assert!(!filters.contains_key(&format!("{tag}_hp")));
                }
            }
        }
        for sub in &chain(&fixture).subs {
            for name in [
                format!("sub_{}_fixed_hp", sub.role),
                format!("sub_{}_fixed_lp", sub.role),
            ] {
                if filters.contains_key(&name) {
                    assert_eq!(config.yaml.matches(&format!("\"{name}\"")).count(), 1);
                }
            }
            for index in 0..sub.peq.len() {
                let name = format!("sub_{}_fixed_peq{index}", sub.role);
                assert_eq!(config.yaml.matches(&format!("\"{name}\"")).count(), 1);
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
    let expected =
        secs_camilla::round_ties_even_i64((3.0 + align_sw1 + chain.main_extra_ms) * 48.0);
    let filters = yaml_filters(&config.yaml);
    let residual = filter_delay(&filters, "mimo_L_SW1_delay").unwrap_or(0);
    assert_eq!(
        support_group_delay_for(&config.yaml, "mimo_L_SW1") + residual,
        expected
    );
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
    let got_yaml = replace_folder(&fs::read_to_string(&paths[0]).unwrap(), &temp.0);
    let expected_yaml = fixture.outputs()["yaml"].as_str().unwrap();
    assert_yaml_transfer(
        &fixture.id,
        &got_yaml,
        expected_yaml,
        &|name| fs::read(temp.0.join("coeffs").join(name)).unwrap(),
        &fixture_camilla_wav(&fixture),
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
fn failed_reexport_keeps_the_previous_camilla_bundle_td22() {
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("failed_reexport");
    let chain = chain(&fixture);
    write_config(&chain, &temp.0, "replace", &Options::default(), None).unwrap();
    let config = temp.0.join("configs/replace.yml");
    let before = fs::read(&config).unwrap();

    let mut changed = chain.clone();
    changed.mains[0].fir[0] += 0.5;
    let error = write_config_observed(
        &changed,
        &temp.0,
        "replace",
        &Options::default(),
        None,
        &|index, _| {
            if index == 1 {
                Err(std::io::Error::other("injected second-file failure"))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected second-file failure"));
    assert_eq!(fs::read(config).unwrap(), before);
    let stage_prefix = format!(".{}.stage-", temp.0.file_name().unwrap().to_string_lossy());
    assert!(
        fs::read_dir(temp.0.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&stage_prefix))
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

    let mut bad = chain(&fixture);
    bad.mains[0].role = "../x".into();
    assert!(matches!(
        write_config(&bad, &temp.0, "bad", &Options::default(), None),
        Err(CamillaError::Invalid(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
    assert!(!temp.0.parent().unwrap().join("x_fir.wav").exists());

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

#[test]
fn fir_taps_that_overflow_f32_write_nothing_td45() {
    // 1e100 is a finite f64 but becomes infinity in the f32 WAV.
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("f32_overflow");
    let mut bad = chain(&fixture);
    bad.mains[0].fir = vec![1.0, 1e100];
    let error = write_config(&bad, &temp.0, "bad", &Options::default(), None).unwrap_err();
    assert!(matches!(error, CamillaError::NonFinite(_)), "{error}");
    assert!(error.to_string().contains("main.L.fir[1]"), "{error}");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    // A low-rate FIR is checked after its expansion to 48 kHz.
    let fixture = Fixture::load("export_mimo", "room_dba_iir");
    let mut path = mimo_paths(&fixture)
        .paths
        .into_iter()
        .find(|path| path.input == "L" && path.support == "SW1")
        .unwrap();
    path.filter = MimoFilter::FirLowRate {
        taps_low: vec![1e100, 0.5, 0.25, 0.125],
        fs_low: 1000.0,
    };
    let mimo = MimoPaths {
        fsiso: 120.0,
        paths: vec![path],
    };
    let error = write_config(
        &chain(&fixture),
        &temp.0,
        "bad",
        &Options::default(),
        Some(&mimo),
    )
    .unwrap_err();
    assert!(matches!(error, CamillaError::NonFinite(_)), "{error}");
    assert!(error.to_string().contains("mimo.paths[0]"), "{error}");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn duplicate_or_reserved_roles_write_nothing_td46() {
    let temp = TempDir::new("duplicate_roles");
    let reject = |chain: &SignalChain, role: &str| {
        let error = write_config(chain, &temp.0, "bad", &Options::default(), None).unwrap_err();
        assert!(matches!(error, CamillaError::Invalid(_)), "{error}");
        assert!(
            error
                .to_string()
                .contains(&format!("duplicate or reserved role {role:?}")),
            "{error}"
        );
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
    };

    let fixture = Fixture::load("camilla", "direct_no_subs");
    let mut two_mains = chain(&fixture);
    two_mains.mains.push(two_mains.mains[0].clone());
    reject(&two_mains, "L");
    let mut reserved = chain(&fixture);
    reserved.mains[0].role = "LFE".into();
    reject(&reserved, "LFE");

    let fixture = Fixture::load("export_mimo", "room_dba_iir");
    let mut sub_as_main = chain(&fixture);
    sub_as_main.subs[0].role = sub_as_main.mains[0].role.clone();
    let role = sub_as_main.mains[0].role.clone();
    reject(&sub_as_main, &role);
    let mut two_subs = chain(&fixture);
    two_subs.subs.push(two_subs.subs[0].clone());
    let role = two_subs.subs[0].role.clone();
    reject(&two_subs, &role);
    let mut reserved_sub = chain(&fixture);
    reserved_sub.subs[0].role = "LFE".into();
    reject(&reserved_sub, "LFE");
}

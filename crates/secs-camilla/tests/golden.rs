#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use secs_camilla::pyfmt::{format_fixed, format_g, repr_f64, round_ndigits};
use secs_camilla::{
    CamillaError, Device, DeviceValue, Options, RunLabels, export, make_config, write_config,
};
use secs_oracle::{Fixture, goldens};
use secs_types::chain::{BandType, MainChannel, SignalChain, SubChannel};
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
        let config =
            make_config(&chain(&fixture), &temp.0, &config_dir, "edge_", &options).unwrap();
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
        let labels = &fixture.inputs()["labels"];
        let labels = RunLabels {
            mode: labels["mode"].as_str().unwrap().into(),
            ultra_ms: number(&labels["ultra_ms"]),
            latency_ms: number(&labels["latency_ms"]),
            rc_hi_hz: number(&labels["rc_hi_hz"]),
        };
        let paths = export(&chain(&fixture), &temp.0, &labels).unwrap();
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
fn invalid_values_write_nothing() {
    let fixture = Fixture::load("camilla", "direct_no_subs");
    let temp = TempDir::new("errors");
    let mut bad = chain(&fixture);
    bad.mains[0].gain_db = f64::NAN;
    assert!(matches!(
        write_config(&bad, &temp.0, "bad", &Options::default()),
        Err(CamillaError::NonFinite(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    let mut bad = chain(&fixture);
    bad.mains[0].role = "왼쪽".into();
    assert!(matches!(
        write_config(&bad, &temp.0, "bad", &Options::default()),
        Err(CamillaError::NonAscii(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    let options = Options {
        mode: "bad\nmode".into(),
        ..Options::default()
    };
    assert!(matches!(
        write_config(&chain(&fixture), &temp.0, "bad", &options),
        Err(CamillaError::Invalid(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);

    assert!(matches!(
        write_config(&chain(&fixture), &temp.0, "..", &Options::default()),
        Err(CamillaError::Invalid(_))
    ));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

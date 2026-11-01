#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod support;

use secs_filters::{
    CONVOLUTION_GUIDE, FilterPlan, FilterRun, FiltersError, MimoFeed, Op, Options, SECTION_Q,
    SubFeed, eqapo_config, eqapo_filters, eqapo_guide, export_convolution,
    export_convolution_with_options, export_eqapo, export_eqapo_with_options, export_filter_text,
    export_filter_text_with_options, filter_text, headroom_db, make_plan, make_plan_with_lfe,
    plan_responses, render_paths, windows_channels, write_eqapo, write_eqapo_observed, write_text,
    write_text_observed,
};
use secs_oracle::{Fixture, assert_close, goldens, rtol};
use secs_types::calibration::RcBand;
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};
use secs_types::ipc::ExportBlock;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);
const RESPONSE_ATOL: f64 = 8e-14;
const CASES: [&str; 7] = [
    "rc_mixed",
    "rc_zero",
    "ba_edges",
    "no_lfe_empty_source",
    "ultra_name",
    "mimo_fir",
    "mimo_iir",
];
const COMPUTED_CASES: [&str; 5] = [
    "computed_dolby",
    "computed_dolby_infra",
    "computed_dolby_ultra",
    "computed_room_rc_mixed",
    "computed_room_rc_zero",
];

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("secs_filters_{label}_{}_{id}", std::process::id()));
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
    SignalChain {
        input_hp_hz: None,
        sample_rate: value["sample_rate"].as_u64().unwrap() as u32,
        mains: value["mains"]
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
            .collect(),
        subs: value["subs"]
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
            .collect(),
        sub_bus_peq: peqs(&value["sub_bus_peq"]),
        main_extra_ms: number(&value["main_extra_ms"]),
        lift_ms: number(&value["lift_ms"]),
        latency_ms: number(&value["latency_ms"]),
    }
}

fn mimo(fixture: &Fixture) -> Option<MimoPaths> {
    let value = fixture.inputs().get("mimo")?;
    if value.is_null() {
        return None;
    }
    Some(MimoPaths {
        fsiso: number(&value["fsiso"]),
        paths: value["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| {
                let value = &path["filter"];
                let filter = if let Some(iir) = value.get("Iir") {
                    MimoFilter::Iir {
                        gain_db: number(&iir["gain_db"]),
                        delay_ms: number(&iir["delay_ms"]),
                        polarity: iir["polarity"].as_i64().unwrap() as i8,
                        peq: peqs(&iir["peq"]),
                        allpass: allpasses(&iir["allpass"]),
                    }
                } else {
                    MimoFilter::Fir {
                        taps_48k: fixture.f64s(&value["Fir"]["taps_48k"]),
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
    })
}

fn plan(fixture: &Fixture, chain: &SignalChain, mimo: Option<&MimoPaths>) -> FilterPlan {
    make_plan_with_lfe(
        chain,
        mimo,
        fixture.inputs()["lfe_gain_db"].as_f64(),
        number(&fixture.inputs()["lfe_lp_hz"]),
    )
    .unwrap()
}

fn assert_op(fixture: &Fixture, id: &str, got: &Op, expected: &Value) {
    assert_eq!(got.fixed(), expected["fixed"].as_bool().unwrap(), "{id}");
    match (got, expected["kind"].as_str().unwrap()) {
        (
            Op::Peq {
                frequency,
                q,
                gain_db,
                ..
            },
            "peq",
        ) => {
            assert_eq!(*frequency, number(&expected["frequency"]), "{id}.frequency");
            assert_eq!(*q, number(&expected["q"]), "{id}.q");
            assert_eq!(*gain_db, number(&expected["gain_db"]), "{id}.gain");
        }
        (Op::Fir { name, taps, .. }, "fir") => {
            assert_eq!(name, expected["name"].as_str().unwrap(), "{id}.name");
            assert_eq!(taps, &fixture.f64s(&expected["taps"]), "{id}.taps");
        }
        (Op::Gain { db, .. }, "gain") => assert_eq!(*db, number(&expected["db"]), "{id}.db"),
        (Op::Invert { .. }, "invert") => {}
        (
            Op::Hp {
                frequency, kind, ..
            },
            "hp",
        )
        | (
            Op::Lp {
                frequency, kind, ..
            },
            "lp",
        ) => {
            assert_eq!(*frequency, number(&expected["frequency"]), "{id}.frequency");
            assert_eq!(kind.name(), expected["type"].as_str().unwrap(), "{id}.type");
        }
        (Op::Ap { frequency, q, .. }, "ap") => {
            assert_eq!(*frequency, number(&expected["frequency"]), "{id}.frequency");
            assert_eq!(*q, number(&expected["q"]), "{id}.q");
        }
        (Op::Delay { ms, .. }, "delay") => assert_eq!(*ms, number(&expected["ms"]), "{id}.ms"),
        _ => panic!("{id}: operation mismatch: {got:?} / {expected}"),
    }
}

fn assert_ops(fixture: &Fixture, id: &str, got: &[Op], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(got.len(), expected.len(), "{id}.length");
    for (index, (got, expected)) in got.iter().zip(expected).enumerate() {
        assert_op(fixture, &format!("{id}[{index}]"), got, expected);
    }
}

fn assert_pairs(fixture: &Fixture, id: &str, got: &[(String, Vec<Op>)], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(got.len(), expected.len(), "{id}.length");
    for (index, ((role, ops), item)) in got.iter().zip(expected).enumerate() {
        assert_eq!(role, item[0].as_str().unwrap(), "{id}[{index}].role");
        assert_ops(fixture, &format!("{id}[{index}]"), ops, &item[1]);
    }
}

fn assert_plan(fixture: &Fixture, got: &FilterPlan) {
    let expected = &fixture.outputs()["plan"];
    let strings = |value: &Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        got.inputs,
        strings(&expected["inputs"]),
        "{}.inputs",
        fixture.id
    );
    assert_eq!(
        got.outputs,
        strings(&expected["outputs"]),
        "{}.outputs",
        fixture.id
    );
    assert_pairs(fixture, "input_ops", &got.input_ops, &expected["input_ops"]);
    assert_pairs(fixture, "main_ops", &got.main_ops, &expected["main_ops"]);
    assert_pairs(fixture, "send_ops", &got.send_ops, &expected["send_ops"]);
    assert_ops(fixture, "lfe_ops", &got.lfe_ops, &expected["lfe_ops"]);
    let subs = expected["subs"].as_array().unwrap();
    assert_eq!(got.subs.len(), subs.len(), "{}.subs", fixture.id);
    for (index, ((role, feed), item)) in got.subs.iter().zip(subs).enumerate() {
        let expected = &item[1];
        assert_eq!(role, item[0].as_str().unwrap(), "sub[{index}].role");
        assert_eq!(
            feed.sources,
            strings(&expected["sources"]),
            "sub[{index}].sources"
        );
        assert_eq!(
            feed.gain_db,
            number(&expected["gain_db"]),
            "sub[{index}].gain"
        );
        assert_eq!(
            i64::from(feed.polarity),
            expected["polarity"].as_i64().unwrap(),
            "sub[{index}].polarity"
        );
        assert_ops(
            fixture,
            &format!("sub[{index}].ops"),
            &feed.ops,
            &expected["ops"],
        );
    }
    let paths = expected["mimo"].as_array().unwrap();
    assert_eq!(got.mimo.len(), paths.len(), "{}.mimo", fixture.id);
    for (index, (feed, expected)) in got.mimo.iter().zip(paths).enumerate() {
        assert_eq!(
            feed.input,
            expected["input"].as_str().unwrap(),
            "mimo[{index}].input"
        );
        assert_eq!(
            feed.support,
            expected["support"].as_str().unwrap(),
            "mimo[{index}].support"
        );
        assert_ops(
            fixture,
            &format!("mimo[{index}].ops"),
            &feed.ops,
            &expected["ops"],
        );
    }
}

fn fir_names(plan: &FilterPlan, prefix: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let ops = plan
        .input_ops
        .iter()
        .map(|(_, ops)| ops)
        .chain(plan.mimo.iter().map(|feed| &feed.ops));
    for ops in ops {
        for op in ops {
            if let Op::Fir { name, .. } = op {
                let pair = (name.clone(), format!("{prefix}{name}.wav"));
                if let Some(found) = result.iter_mut().find(|(key, _)| key == name) {
                    *found = pair;
                } else {
                    result.push(pair);
                }
            }
        }
    }
    result
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = INITIAL;
    for chunk in padded.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*word);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn relative_paths(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn replace_once(text: &str, old: &str, new: &str) -> String {
    assert_eq!(text.matches(old).count(), 1, "F11 fixture text: {old}");
    text.replacen(old, new, 1)
}

// Keep the upstream fixtures intact. Only F11's Eval syntax and its editing
// instructions differ; every other character, filter body and WAV stays exact.
fn f11_config(expected: &str) -> String {
    let expected = replace_once(
        expected,
        "# ==== EDIT ONLY THESE TWO LINES:",
        "# ==== EDIT ONLY THE CHANNEL NUMBERS BELOW:",
    );
    let mut groups = 0;
    let mut result = String::new();
    for line in expected.split_inclusive('\n') {
        if let Some(assignments) = line.strip_prefix("Eval: ") {
            let prefix = match groups {
                0 => "in",
                1 => "out",
                _ => panic!("unexpected upstream Eval group"),
            };
            groups += 1;
            for assignment in assignments.strip_suffix('\n').unwrap().split("; ") {
                let (variable, number) = assignment.split_once(" = ").unwrap();
                assert!(variable.starts_with(prefix));
                assert!(variable.chars().all(|ch| ch.is_ascii_alphanumeric()));
                assert!(!number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()));
                result.push_str(&format!("Eval: {assignment}\n"));
            }
        } else {
            result.push_str(line);
        }
    }
    assert_eq!(groups, 2);
    result
}

fn f11_guide(expected: &str) -> String {
    let expected = replace_once(
        expected,
        "config.txt 맨 위 Eval 두 줄의 숫자만",
        "config.txt 맨 위 입력·출력 Eval 줄의 숫자만",
    );
    let expected = replace_once(&expected, "\"Eval: inL = 1; …\"", "\"Eval: inL = 1\"");
    replace_once(&expected, "\"Eval: outL = 1; …\"", "\"Eval: outL = 1\"")
}

fn f11_expected_file(kind: &str, path: &str, expected: Vec<u8>) -> Vec<u8> {
    match (kind, path) {
        ("eqapo", "config.txt") => f11_config(std::str::from_utf8(&expected).unwrap()).into_bytes(),
        ("eqapo", secs_filters::EQAPO_GUIDE) => {
            f11_guide(std::str::from_utf8(&expected).unwrap()).into_bytes()
        }
        _ => expected,
    }
}

const TRANSFER_FREQUENCIES: [f64; 11] = [
    7.0, 20.0, 31.0, 43.0, 80.0, 120.0, 173.0, 350.0, 1000.0, 8765.0, 20_000.0,
];

fn eqapo_body_path(config: &str) -> String {
    config
        .lines()
        .find_map(|line| line.strip_prefix("Include: "))
        .unwrap()
        .replace('\\', "/")
}

fn compare_apo_graph_outside(
    expected_config: &str,
    got_config: &str,
    expected_body: &str,
    got_body: &str,
) -> Result<(), String> {
    let expected_lines = expected_config.split_inclusive('\n').collect::<Vec<_>>();
    let got_lines = got_config.split_inclusive('\n').collect::<Vec<_>>();
    if expected_lines.len() != got_lines.len() {
        return Err(format!(
            "config.txt line count: got {}, expected {}",
            got_lines.len(),
            expected_lines.len()
        ));
    }
    for (index, (expected, got)) in expected_lines.iter().zip(&got_lines).enumerate() {
        let expected_copy = expected.trim_start().starts_with("Copy:");
        let got_copy = got.trim_start().starts_with("Copy:");
        if expected_copy != got_copy {
            return Err(format!("config.txt line {} command changed", index + 1));
        }
        if !expected_copy && expected != got {
            return Err(format!(
                "config.txt line {} differs: {got:?} != {expected:?}",
                index + 1
            ));
        }
    }
    fn head(body: &str) -> &str {
        body.split_once("\n\n").map_or(body, |(head, _)| head)
    }
    if head(expected_body) != head(got_body) {
        return Err(format!(
            "filter body head differs: {:?} != {:?}",
            head(got_body),
            head(expected_body)
        ));
    }
    Ok(())
}

fn assert_apo_transfer(
    id: &str,
    got_config: &str,
    got_body: &str,
    got_wav: &dyn Fn(&str) -> Vec<u8>,
    expected_config: &str,
    expected_body: &str,
    expected_wav: &dyn Fn(&str) -> Vec<u8>,
) {
    compare_apo_graph_outside(expected_config, got_config, expected_body, got_body)
        .unwrap_or_else(|error| panic!("{id}: {error}"));
    let got_body_path = eqapo_body_path(got_config);
    let expected_body_path = eqapo_body_path(expected_config);
    let mut max_expected = 0.0_f64;
    for input in 0..16 {
        for frequency in TRANSFER_FREQUENCIES {
            let got = support::apo_graph::transfer(
                got_config,
                &|path| {
                    assert_eq!(path.replace('\\', "/"), got_body_path, "{id} Include");
                    got_body.into()
                },
                got_wav,
                input,
                frequency,
            );
            let expected = support::apo_graph::transfer(
                expected_config,
                &|path| {
                    assert_eq!(
                        path.replace('\\', "/"),
                        expected_body_path,
                        "{id} oracle Include"
                    );
                    expected_body.into()
                },
                expected_wav,
                input,
                frequency,
            );
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
        "{id}: expected APO responses are all silent ({max_expected})"
    );
}

#[test]
fn apo_graph_outside_comparison_rejects_device_format_changes() {
    let expected = "# Device format: 48000 Hz\nCopy: 1=V1\n";
    let changed = "# Device format: 44100 Hz\nCopy: 1=V2\n";
    assert!(
        compare_apo_graph_outside(expected, changed, "# note\n\nold", "# note\n\nnew").is_err()
    );
}

fn assert_written(fixture: &Fixture, kind: &str, root: &Path, paths: &[PathBuf]) {
    let expected = &fixture.outputs()["written"][kind];
    assert_eq!(relative_paths(root, paths), strings(&expected["paths"]));
    if kind == "eqapo" {
        let files = expected["files"].as_array().unwrap();
        let expected_config_item = files
            .iter()
            .find(|item| item["path"] == "config.txt")
            .unwrap();
        let expected_config =
            f11_config(std::str::from_utf8(&fixture.blob(&expected_config_item["bytes"])).unwrap());
        let body_path = eqapo_body_path(&expected_config);
        let expected_body_item = files.iter().find(|item| item["path"] == body_path).unwrap();
        let expected_body = String::from_utf8(fixture.blob(&expected_body_item["bytes"])).unwrap();
        let got_config = fs::read_to_string(root.join("config.txt")).unwrap();
        assert_eq!(
            eqapo_body_path(&got_config),
            body_path,
            "{} Include",
            fixture.id
        );
        let got_body = fs::read_to_string(root.join(&body_path)).unwrap();
        assert_apo_transfer(
            &fixture.id,
            &got_config,
            &got_body,
            &|name| {
                fs::read(
                    root.join(Path::new(&body_path).parent().unwrap())
                        .join(name),
                )
                .unwrap()
            },
            &expected_config,
            &expected_body,
            &|name| {
                let item = files
                    .iter()
                    .find(|item| {
                        Path::new(item["path"].as_str().unwrap())
                            .file_name()
                            .unwrap()
                            == name
                    })
                    .unwrap();
                fixture.blob(&item["bytes"])
            },
        );
    }
    for item in expected["files"].as_array().unwrap() {
        let path = item["path"].as_str().unwrap();
        if kind == "eqapo"
            && (path == "config.txt"
                || path == eqapo_body_path(&fs::read_to_string(root.join("config.txt")).unwrap()))
        {
            continue;
        }
        assert_eq!(
            fs::read(root.join(path)).unwrap(),
            f11_expected_file(kind, path, fixture.blob(&item["bytes"])),
            "{} {kind} {path}",
            fixture.id,
        );
    }
}

fn assert_computed_written(fixture: &Fixture, kind: &str, root: &Path, paths: &[PathBuf]) {
    let expected = &fixture.outputs()["written"][kind];
    assert_eq!(relative_paths(root, paths), strings(&expected["paths"]));
    let config = (kind == "eqapo").then(|| fs::read_to_string(root.join("config.txt")).unwrap());
    let body_path = config.as_deref().map(eqapo_body_path);
    if let (Some(got_config), Some(body_path)) = (&config, &body_path) {
        let files = expected["files"].as_array().unwrap();
        let config_item = files
            .iter()
            .find(|item| item["path"] == "config.txt")
            .unwrap();
        let expected_config =
            f11_config(std::str::from_utf8(&fixture.blob(&config_item["bytes"])).unwrap());
        let body_item = files
            .iter()
            .find(|item| item["path"] == *body_path)
            .unwrap();
        let expected_body = String::from_utf8(fixture.blob(&body_item["bytes"])).unwrap();
        let got_body = fs::read_to_string(root.join(body_path)).unwrap();
        let wav = |name: &str| {
            fs::read(root.join(Path::new(body_path).parent().unwrap()).join(name)).unwrap()
        };
        assert_apo_transfer(
            &fixture.id,
            got_config,
            &got_body,
            &wav,
            &expected_config,
            &expected_body,
            &wav,
        );
    }
    for item in expected["files"].as_array().unwrap() {
        let path = item["path"].as_str().unwrap();
        let bytes = fs::read(root.join(path)).unwrap();
        if kind == "eqapo" && (path == "config.txt" || Some(path) == body_path.as_deref()) {
            continue;
        }
        if kind == "eqapo" && path == secs_filters::EQAPO_GUIDE {
            let original = fixture.blob(&item["bytes"]);
            assert_eq!(original.len() as u64, item["length"].as_u64().unwrap());
            assert_eq!(sha256_hex(&original), item["sha256"].as_str().unwrap());
            assert_eq!(bytes, f11_expected_file(kind, path, original));
            continue;
        }
        assert_eq!(bytes.len() as u64, item["length"].as_u64().unwrap());
        assert_eq!(sha256_hex(&bytes), item["sha256"].as_str().unwrap());
        if let Some(blob) = item.get("bytes") {
            assert_eq!(bytes, fixture.blob(blob), "{} {kind} {path}", fixture.id);
        }
    }
}

fn run_parts(fixture: &Fixture) -> (String, Vec<String>, String) {
    let value = &fixture.inputs()["run"];
    (
        value["label"].as_str().unwrap().into(),
        strings(&value["rc_pieces"]),
        value["stamp"].as_str().unwrap().into(),
    )
}

fn preamp_line(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .find(|line| line.starts_with("Preamp: "))
}

fn report_preamp_difference(fixture: &Fixture, root: &Path) {
    let expected = fixture.outputs()["written"]["eqapo"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == "config.txt")
        .unwrap();
    let expected = fixture.blob(&expected["bytes"]);
    let got = fs::read(root.join("config.txt")).unwrap();
    if preamp_line(&got) != preamp_line(&expected) {
        println!(
            "MEASURE {} headroom_preamp got={:?} expected={:?}",
            fixture.id,
            preamp_line(&got),
            preamp_line(&expected)
        );
    }
}

// Opt in to retaining actual Rust writer outputs for an external APO engine.
// Normal test runs only use TempDir; no repository or persistent output changes.
fn dump_f11_export(case: &str, root: &Path, paths: &[PathBuf]) {
    let Some(destination) = std::env::var_os("SECS_EQAPO_F11_DUMP_DIR") else {
        return;
    };
    let destination = PathBuf::from(destination);
    assert!(
        destination.is_absolute(),
        "F11 dump directory must be absolute"
    );
    for path in paths {
        let target = destination
            .join(case)
            .join(path.strip_prefix(root).unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(path, target).unwrap();
    }
}

fn f11_synthetic_chain(stereo: bool, fir: bool, subwoofer: bool) -> SignalChain {
    let fixture = Fixture::load("filters", "rc_mixed");
    let mut result = chain(&fixture);
    if !stereo {
        result.mains.truncate(1);
    }
    for (index, main) in result.mains.iter_mut().enumerate() {
        main.input_peq.clear();
        main.top_peq.clear();
        main.fir = if fir {
            if index == 0 {
                vec![0.5, -0.25, 0.125]
            } else {
                vec![0.25, 0.125, -0.0625]
            }
        } else {
            vec![1.0]
        };
        main.gain_db = if fir { 0.0 } else { -6.0 * (index + 1) as f64 };
        main.delay_ms = 0.0;
        main.align_delay_ms = 0.0;
        main.xo_hz = subwoofer.then_some(80.0);
    }
    result.subs = if subwoofer {
        vec![SubChannel {
            role: "SW1".into(),
            gain_db: -6.0,
            delay_ms: 0.0,
            polarity: 1,
            allpass: vec![],
            hp_hz: 0.0,
            hp_type: BandType::Lr24,
            lp_hz: 0.0,
            lp_type: BandType::Lr24,
            peq: vec![],
            mode: "auto".into(),
        }]
    } else {
        vec![]
    };
    result.sub_bus_peq.clear();
    result.main_extra_ms = 0.0;
    result.lift_ms = 0.0;
    result.latency_ms = 0.0;
    result
}

fn assert_f11_assignments(config: &str, expected: &[&str]) {
    let assignments: Vec<_> = config
        .lines()
        .filter(|line| line.starts_with("Eval: "))
        .collect();
    assert_eq!(assignments, expected);
    let first_routing = config.find("Copy: ").unwrap();
    let first_condition = config.find("If: ").unwrap();
    for assignment in assignments {
        assert_eq!(assignment.matches('=').count(), 1);
        assert!(!assignment.contains(';'));
        assert!(config.find(assignment).unwrap() < first_routing);
        assert!(config.find(assignment).unwrap() < first_condition);
    }
    assert!(!config.contains("TWO LINES"));
}

#[test]
fn eqapo_public_exports_use_single_assignments_f11() {
    let root = TempDir::new("f11_interop");
    for (case, stereo, fir, subwoofer, expected) in [
        (
            "mono_fir",
            false,
            true,
            false,
            vec!["Eval: inL = 1", "Eval: outL = 1"],
        ),
        (
            "stereo_fir",
            true,
            true,
            false,
            vec![
                "Eval: inL = 1",
                "Eval: inR = 2",
                "Eval: outL = 1",
                "Eval: outR = 2",
            ],
        ),
        (
            "stereo_gain_only",
            true,
            false,
            false,
            vec![
                "Eval: inL = 1",
                "Eval: inR = 2",
                "Eval: outL = 1",
                "Eval: outR = 2",
            ],
        ),
        (
            "stereo_2_1_fir",
            true,
            true,
            true,
            vec![
                "Eval: inL = 1",
                "Eval: inR = 2",
                "Eval: inLFE = 3",
                "Eval: outL = 1",
                "Eval: outR = 2",
                "Eval: outSW1 = 3",
            ],
        ),
    ] {
        let chain = f11_synthetic_chain(stereo, fir, subwoofer);
        let run = FilterRun {
            chain: &chain,
            mimo: None,
            mode: if fir { "mixed" } else { "zero" },
            note: &export_note(if fir { "mixed" } else { "zero" }, 0.0, 100.0, None),
            label: "Interop synthetic",
            latency_ms: 0.0,
            rc_pieces: &[],
            stamp: "2026-10-21 00:00",
        };
        let folder = root.0.join(case);
        let paths = export_eqapo(&run, &folder).unwrap();
        let config = fs::read_to_string(&paths[0]).unwrap();
        assert_f11_assignments(&config, &expected);
        let guide = fs::read_to_string(folder.join(secs_filters::EQAPO_GUIDE)).unwrap();
        assert!(guide.contains("입력·출력 Eval 줄의 숫자만"));
        assert!(guide.contains("\"Eval: inL = 1\""));
        assert!(guide.contains("\"Eval: outL = 1\""));
        assert!(!guide.contains("두 줄") && !guide.contains("; …"));
        dump_f11_export(case, &folder, &paths);
    }
}

#[test]
fn output_rates_change_eqapo_text_and_convolution_bundles() {
    let mut chain = f11_synthetic_chain(true, true, false);
    chain.mains[0].delay_ms = 1.25;
    let run = FilterRun {
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: "output rate test",
        label: "output rate test",
        latency_ms: 0.0,
        rc_pieces: &[],
        stamp: "2026-10-29 00:00",
    };
    let options = Options {
        output_rate: 96_000,
        extra_rates: Vec::new(),
    };
    let root = TempDir::new("output_rates");

    let eqapo = root.0.join("eqapo");
    let eqapo_files = export_eqapo_with_options(&run, &eqapo, &options).unwrap();
    let config = fs::read_to_string(eqapo.join("config.txt")).unwrap();
    assert!(config.contains("# Device format: 96000 Hz"));
    for path in eqapo_files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
    {
        let bytes = fs::read(path).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            96_000
        );
        assert_eq!((bytes.len() - 44) / 4, 6);
    }

    let text_folder = root.0.join("text");
    let text_files = export_filter_text_with_options(&run, &text_folder, &options).unwrap();
    let text_path = text_files
        .iter()
        .find(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .unwrap();
    let text = String::from_utf8_lossy(&fs::read(text_path).unwrap()).into_owned();
    assert!(text.contains("샘플레이트 96000 Hz"));
    assert!(text.contains("(6 탭, 96 kHz)"));
    assert!(text.contains("1.250 ms (120 샘플 @ 96 kHz)"));

    let convolution = root.0.join("convolution");
    let convolution_options = Options {
        output_rate: 96_000,
        extra_rates: vec![44_100],
    };
    export_convolution_with_options(&run, &convolution, &convolution_options).unwrap();
    for (prefix, rate) in [(PathBuf::new(), 96_000), (PathBuf::from("44100Hz"), 44_100)] {
        let routing: Value = serde_json::from_slice(
            &fs::read(convolution.join(&prefix).join("routing.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(routing["sample_rate"], rate);
        assert!(
            convolution
                .join(&prefix)
                .join("secs2r_reaper.lua")
                .is_file()
        );
        let route = &routing["paths"][0];
        let wav = fs::read(
            convolution
                .join(&prefix)
                .join(route["wav"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), rate);
        assert_eq!(
            (wav.len() - 44) / 4,
            routing["length"].as_u64().unwrap() as usize
        );
    }
    let guide = fs::read_to_string(convolution.join(CONVOLUTION_GUIDE)).unwrap();
    assert!(guide.contains("추가 샘플레이트 폴더: 44100Hz/"));

    for invalid in [
        Options {
            output_rate: 96_000,
            extra_rates: vec![96_000],
        },
        Options {
            output_rate: 96_000,
            extra_rates: vec![32_000],
        },
        Options {
            output_rate: 96_000,
            extra_rates: vec![44_100, 44_100],
        },
    ] {
        assert!(export_convolution_with_options(&run, &root.0.join("invalid"), &invalid).is_err());
    }
}

#[test]
fn eqapo_disabled_and_case_sensitive_mapping_f11() {
    let mut chain = f11_synthetic_chain(true, true, false);
    // Deliberately differ from both alphabetical order and the mapping vectors.
    chain.mains[0].role = "Tmr".into();
    chain.mains[1].role = "Tml".into();
    let plan = secs_filters::make_plan(&chain, None).unwrap();
    let config = eqapo_config(
        &plan,
        "secs_mixed\\secs_mixed.txt",
        0.0,
        &[("Tml".into(), 0), ("Tmr".into(), 12)],
        &[("Tml".into(), 11), ("Tmr".into(), 0)],
        None,
    );
    assert_f11_assignments(
        &config,
        &[
            "Eval: inTmr = 12",
            "Eval: inTml = 0",
            "Eval: outTmr = 0",
            "Eval: outTml = 11",
        ],
    );
    assert!(
        config.contains(
            "If: inTml >= 1 && inTml <= inputChannelCount\n  Copy: VMTML=`inTml`\nEndIf:"
        )
    );
    assert!(config.contains(
        "If: outTmr >= 1 && outTmr <= outputChannelCount\n  Copy: `outTmr`=VMTMR\nEndIf:"
    ));
}

#[test]
fn plans_and_windows_channels_match_oracle() {
    for case in CASES {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let plan = plan(&fixture, &chain, mimo.as_ref());
        assert_plan(&fixture, &plan);
        let (inputs, outputs) = windows_channels(&plan);
        let expected = &fixture.outputs()["windows_channels"];
        let pairs = |value: &Value| {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_str().unwrap().to_owned(),
                        pair[1].as_u64().unwrap() as usize,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(inputs, pairs(&expected["inputs"]), "{case} inputs");
        assert_eq!(outputs, pairs(&expected["outputs"]), "{case} outputs");
    }
}

#[test]
fn serializers_match_oracle_text_exactly() {
    for case in CASES {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let plan = plan(&fixture, &chain, mimo.as_ref());
        let direct = &fixture.outputs()["direct"];
        let firs = fir_names(&plan, "edge_");
        let (inputs, outputs) = windows_channels(&plan);
        let preamp = (headroom_db(&plan).unwrap() * 2.0).ceil() / 2.0;
        let run = &fixture.inputs()["run"];
        let got_body = eqapo_filters(&plan, &firs, "ASCII before / 한글 / after");
        let expected_body = direct["eqapo_filters"].as_str().unwrap();
        let got_config = eqapo_config(
            &plan,
            "edge\\edge.txt",
            preamp,
            &inputs,
            &outputs,
            Some(number(&run["latency_ms"])),
        );
        let expected_config = f11_config(direct["eqapo_config"].as_str().unwrap());
        let wav = |name: &str| {
            let key = firs
                .iter()
                .find(|(_, path)| path == name)
                .unwrap()
                .0
                .as_str();
            let op = plan
                .input_ops
                .iter()
                .flat_map(|(_, ops)| ops)
                .chain(plan.mimo.iter().flat_map(|feed| &feed.ops))
                .find(|op| match op {
                    Op::Fir { name, .. } | Op::FirLowRate { name, .. } => name == key,
                    _ => false,
                })
                .unwrap();
            let taps = match op {
                Op::Fir { taps, .. } => taps.clone(),
                Op::FirLowRate {
                    taps_low, fs_low, ..
                } => secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low),
                _ => unreachable!(),
            };
            secs_camilla::wav_bytes(&taps, 48_000).unwrap()
        };
        assert_apo_transfer(
            case,
            &got_config,
            &got_body,
            &wav,
            &expected_config,
            expected_body,
            &wav,
        );
        assert_eq!(
            eqapo_guide(
                &plan,
                &inputs,
                &outputs,
                "edge",
                run["label"].as_str().unwrap(),
                Some(number(&run["latency_ms"])),
                preamp,
            ),
            f11_guide(direct["eqapo_guide"].as_str().unwrap()),
            "{case} guide"
        );
        let info = vec![
            format!(
                "만든 때 {} · 전체 지연 {:.1} ms · RC 상한 {:.0} Hz",
                run["stamp"].as_str().unwrap(),
                number(&run["latency_ms"]),
                number(&run["rc_hi_hz"])
            ),
            "샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)".into(),
        ];
        assert_eq!(
            filter_text(
                &plan,
                &firs,
                &format!("SECS 2 보정 필터 — {}", run["label"].as_str().unwrap()),
                &info,
            ),
            direct["filter_text"].as_str().unwrap(),
            "{case} text"
        );
    }
}

#[test]
fn responses_and_headroom_match_oracle() {
    for case in CASES {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let plan = plan(&fixture, &chain, mimo.as_ref());
        let response = &fixture.outputs()["response"];
        let indices = fixture.f64s(&response["indices"]);
        let frequencies = (0..=32_768)
            .map(|index| index as f64 * 48_000.0 / 65_536.0)
            .collect::<Vec<_>>();
        for item in response["responses"].as_array().unwrap() {
            let source = item[0].as_str().unwrap();
            let got = plan_responses(&plan, &frequencies, source, false).unwrap();
            let expected = item[1].as_array().unwrap();
            assert_eq!(got.len(), expected.len(), "{case}/{source} outputs");
            for ((role, values), expected) in got.iter().zip(expected) {
                assert_eq!(role, expected[0].as_str().unwrap(), "{case}/{source} role");
                let sampled = indices
                    .iter()
                    .map(|index| values[*index as usize])
                    .collect::<Vec<_>>();
                let expected = fixture.c128s(&expected[1]);
                let got_re = sampled.iter().map(|value| value.re).collect::<Vec<_>>();
                let got_im = sampled.iter().map(|value| value.im).collect::<Vec<_>>();
                let expected_re = expected.iter().map(|value| value.0).collect::<Vec<_>>();
                let expected_im = expected.iter().map(|value| value.1).collect::<Vec<_>>();
                assert_close(
                    &format!("filters/{case}/{source}/{role}/re"),
                    &got_re,
                    &expected_re,
                    0.0,
                    rtol(RESPONSE_ATOL),
                );
                assert_close(
                    &format!("filters/{case}/{source}/{role}/im"),
                    &got_im,
                    &expected_im,
                    0.0,
                    rtol(RESPONSE_ATOL),
                );
            }
        }
        assert_close(
            &format!("filters/{case}/headroom"),
            &[headroom_db(&plan).unwrap()],
            &[number(&fixture.outputs()["headroom_db"])],
            0.0,
            rtol(RESPONSE_ATOL),
        );
    }
}

#[test]
fn filter_text_files_match_oracle_bytes() {
    for case in ["rc_mixed", "rc_zero", "ultra_name"] {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let plan = plan(&fixture, &chain, None);
        let root = TempDir::new(case);
        let run = &fixture.inputs()["run"];
        let mode = run["mode"].as_str().unwrap();
        let name_mode = if mode == "ultra" {
            format!("ultra{}", run["ultra_ms"].as_f64().unwrap())
        } else {
            mode.into()
        };
        let info = vec![
            format!(
                "만든 때 {} · 전체 지연 {:.1} ms · {}",
                run["stamp"].as_str().unwrap(),
                number(&run["latency_ms"]),
                strings(&run["rc_pieces"]).join(" · ")
            ),
            "샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)".into(),
        ];
        let paths = write_text(
            &plan,
            &root.0,
            &format!("SECS 필터 {}.txt", run["label"].as_str().unwrap()),
            &format!("secs_{name_mode}"),
            &format!("SECS 2 보정 필터 — {}", run["label"].as_str().unwrap()),
            &info,
        )
        .unwrap();
        assert_written(&fixture, "text", &root.0, &paths);
    }
}

#[test]
fn equalizer_apo_files_match_oracle_bytes() {
    for case in [
        "rc_mixed",
        "rc_zero",
        "ba_edges",
        "no_lfe_empty_source",
        "ultra_name",
    ] {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let plan = plan(&fixture, &chain, None);
        let root = TempDir::new(case);
        let run = &fixture.inputs()["run"];
        let mode = run["mode"].as_str().unwrap();
        let name_mode = if mode == "ultra" {
            format!("ultra{}", run["ultra_ms"].as_f64().unwrap())
        } else {
            mode.into()
        };
        let paths = write_eqapo(
            &plan,
            &root.0,
            &format!("secs_{name_mode}"),
            run["label"].as_str().unwrap(),
            &format!(
                "mode {name_mode}, total latency {:.1} ms, RC upper {:.0} Hz; no MIMO",
                number(&run["latency_ms"]),
                number(&run["rc_hi_hz"])
            ),
            Some(number(&run["latency_ms"])),
        )
        .unwrap();
        assert_written(&fixture, "eqapo", &root.0, &paths);
    }
}

#[test]
fn public_wrappers_match_oracle_and_eqapo_carries_mimo_f12() {
    let fixture = Fixture::load("filters", "rc_mixed");
    let rc_chain = chain(&fixture);
    let run_value = &fixture.inputs()["run"];
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let run = FilterRun {
        chain: &rc_chain,
        mimo: None,
        mode: &export_mode(run_value),
        note: &export_note(
            &export_mode(run_value),
            number(&run_value["latency_ms"]),
            number(&run_value["rc_hi_hz"]),
            None,
        ),
        label: &label,
        latency_ms: number(&run_value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    let root = TempDir::new("wrappers");
    let text = export_filter_text(&run, &root.0.join("text")).unwrap();
    assert_written(&fixture, "text", &root.0.join("text"), &text);
    let eqapo = export_eqapo(&run, &root.0.join("eqapo")).unwrap();
    assert_written(&fixture, "eqapo", &root.0.join("eqapo"), &eqapo);

    let fixture = Fixture::load("filters", "ba_edges");
    let ba_chain = chain(&fixture);
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let value = &fixture.inputs()["run"];
    let run = FilterRun {
        chain: &ba_chain,
        mimo: None,
        mode: &export_mode(value),
        note: &export_note(
            &export_mode(value),
            number(&value["latency_ms"]),
            number(&value["rc_hi_hz"]),
            None,
        ),
        label: &label,
        latency_ms: number(&value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    let blocked = root.0.join("blocked_subs");
    assert!(matches!(
        export_filter_text(&run, &blocked),
        Err(FiltersError::Scope(ExportBlock::Subs))
    ));
    assert!(!blocked.exists());

    let fixture = Fixture::load("filters", "mimo_iir");
    let chain = chain(&fixture);
    let mimo = mimo(&fixture).unwrap();
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let value = &fixture.inputs()["run"];
    let run = FilterRun {
        chain: &chain,
        mimo: Some(&mimo),
        mode: &export_mode(value),
        note: &export_note(
            &export_mode(value),
            number(&value["latency_ms"]),
            number(&value["rc_hi_hz"]),
            Some(&mimo),
        ),
        label: &label,
        latency_ms: number(&value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    let blocked_text = root.0.join("blocked_mimo_text");
    assert!(matches!(
        export_filter_text(&run, &blocked_text),
        Err(FiltersError::Scope(ExportBlock::Subs))
    ));
    assert!(!blocked_text.exists());
    let eqapo = root.0.join("mimo_eqapo");
    let written = export_eqapo(&run, &eqapo).unwrap();
    assert!(written.iter().all(|path| path.exists()));
    let config = fs::read_to_string(eqapo.join("config.txt")).unwrap();
    assert_eq!(eqapo_body_path(&config), "secs_mixed/secs_mixed.txt");
    let body = fs::read_to_string(eqapo.join("secs_mixed/secs_mixed.txt")).unwrap();
    assert!(body.contains("MIMO IIR support paths"));
    let plan = secs_filters::make_plan(&chain, Some(&mimo)).unwrap();
    let (_, outputs) = windows_channels(&plan);
    // F13 preserves the serialized APO transfer, not the unrounded analog
    // response from plan_responses. The direct fixture retains the unfactored
    // MIMO body, including the fixed sub filter on both contributing paths.
    let direct = &fixture.outputs()["direct"];
    let expected_config = f11_config(direct["eqapo_config"].as_str().unwrap());
    let expected_config = replace_once(
        &expected_config,
        "Include: edge\\edge.txt",
        "Include: secs_mixed\\secs_mixed.txt",
    );
    let expected_body = direct["eqapo_filters"].as_str().unwrap();
    let expected_body = replace_once(
        expected_body,
        "# ASCII before /  / after",
        "# mode mixed, total latency 12.2 ms, RC upper 321 Hz; MIMO IIR support paths 1 (Fsiso 150 Hz)",
    );
    assert_apo_transfer(
        &fixture.id,
        &config,
        &body,
        &|name| fs::read(eqapo.join("secs_mixed").join(name)).unwrap(),
        &expected_config,
        &expected_body,
        &|name| panic!("unexpected MIMO IIR fixture WAV: {name}"),
    );
    for (role, output) in outputs {
        assert!(output <= 16, "MIMO output {role} channel {output}");
    }
}

#[test]
fn computed_public_wrappers_match_oracle_files() {
    for case in COMPUTED_CASES {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let value = &fixture.inputs()["run"];
        let (label, rc_pieces, stamp) = run_parts(&fixture);
        let run = FilterRun {
            chain: &chain,
            mimo: None,
            mode: &export_mode(value),
            note: &export_note(
                &export_mode(value),
                number(&value["latency_ms"]),
                number(&value["rc_hi_hz"]),
                None,
            ),
            label: &label,
            latency_ms: number(&value["latency_ms"]),
            rc_pieces: &rc_pieces,
            stamp: &stamp,
        };
        let root = TempDir::new(case);
        let eqapo_root = root.0.join("eqapo");
        let paths = export_eqapo(&run, &eqapo_root).unwrap();
        report_preamp_difference(&fixture, &eqapo_root);
        assert_computed_written(&fixture, "eqapo", &eqapo_root, &paths);
        dump_f11_export(case, &eqapo_root, &paths);
        if fixture.outputs()["written"].get("text").is_some() {
            let text_root = root.0.join("text");
            let paths = export_filter_text(&run, &text_root).unwrap();
            assert_computed_written(&fixture, "text", &text_root, &paths);
        }
    }
}

#[test]
fn computed_subwoofer_text_scope_matches_oracle() {
    let fixture = Fixture::load("filters", "computed_dolby");
    let chain = chain(&fixture);
    let value = &fixture.inputs()["run"];
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let run = FilterRun {
        chain: &chain,
        mimo: None,
        mode: &export_mode(value),
        note: &export_note(
            &export_mode(value),
            number(&value["latency_ms"]),
            number(&value["rc_hi_hz"]),
            None,
        ),
        label: &label,
        latency_ms: number(&value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    let root = TempDir::new("computed_scope");
    let blocked = root.0.join("blocked");
    assert!(matches!(
        export_filter_text(&run, &blocked),
        Err(FiltersError::Scope(ExportBlock::Subs))
    ));
    assert!(!blocked.exists());
    assert_eq!(
        fixture.outputs()["filter_text_error"]["type"],
        "CalibrationError"
    );
    assert_eq!(
        fixture.outputs()["filter_text_error"]["folder_exists"],
        false
    );
}

#[test]
fn invalid_values_write_nothing_and_existing_files_are_replaced_atomically() {
    let fixture = Fixture::load("filters", "rc_mixed");
    let mut bad_chain = chain(&fixture);
    let root = TempDir::new("errors");
    bad_chain.mains[0].gain_db = f64::NAN;
    let invalid = root.0.join("invalid");
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let value = &fixture.inputs()["run"];
    let bad_run = FilterRun {
        chain: &bad_chain,
        mimo: None,
        mode: &export_mode(value),
        note: &export_note(
            &export_mode(value),
            number(&value["latency_ms"]),
            number(&value["rc_hi_hz"]),
            None,
        ),
        label: &label,
        latency_ms: number(&value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    assert!(matches!(
        export_filter_text(&bad_run, &invalid),
        Err(FiltersError::Invalid(_))
    ));
    assert!(!invalid.exists());

    let mut unsafe_chain = chain(&fixture);
    unsafe_chain.mains[0].role = "../L".into();
    let unsafe_folder = root.0.join("unsafe_role");
    let unsafe_run = FilterRun {
        chain: &unsafe_chain,
        ..bad_run
    };
    assert!(matches!(
        export_eqapo(&unsafe_run, &unsafe_folder),
        Err(FiltersError::Invalid(_))
    ));
    assert!(!unsafe_folder.exists());

    let chain = chain(&fixture);
    let plan = plan(&fixture, &chain, None);
    let replace = root.0.join("replace");
    let first = write_eqapo(
        &plan,
        &replace,
        "secs_mixed",
        "혼합 위상 모드",
        "note",
        Some(12.25),
    )
    .unwrap();
    fs::write(&first[0], b"old").unwrap();
    let second = write_eqapo(
        &plan,
        &replace,
        "secs_mixed",
        "혼합 위상 모드",
        "note",
        Some(12.25),
    )
    .unwrap();
    assert_eq!(first, second);
    assert_ne!(fs::read(&second[0]).unwrap(), b"old");
    let previous = fs::read(&second[0]).unwrap();
    let error = write_eqapo_observed(
        &plan,
        &replace,
        "secs_mixed",
        "혼합 위상 모드",
        "changed note",
        Some(12.25),
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
    assert_eq!(fs::read(&second[0]).unwrap(), previous);

    let text = root.0.join("text_replace");
    let text_paths = write_text(&plan, &text, "filters.txt", "secs", "first", &[]).unwrap();
    let previous = fs::read(&text_paths[0]).unwrap();
    let error = write_text_observed(
        &plan,
        &text,
        "filters.txt",
        "secs",
        "changed",
        &[],
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
    assert_eq!(fs::read(&text_paths[0]).unwrap(), previous);
    for directory in [&replace, &replace.join("secs_mixed")] {
        assert!(fs::read_dir(directory).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
}

trait FixedMarker {
    fn fixed(&self) -> bool;
}

impl FixedMarker for Op {
    fn fixed(&self) -> bool {
        match self {
            Op::Peq { fixed, .. }
            | Op::Fir { fixed, .. }
            | Op::FirLowRate { fixed, .. }
            | Op::Gain { fixed, .. }
            | Op::Invert { fixed }
            | Op::Hp { fixed, .. }
            | Op::Lp { fixed, .. }
            | Op::Ap { fixed, .. }
            | Op::Delay { fixed, .. } => *fixed,
        }
    }
}

#[allow(dead_code)]
#[test]
fn fir_taps_that_overflow_f32_write_nothing_td45() {
    // 1e100 is a finite f64 but becomes infinity in the f32 WAV.
    let root = TempDir::new("f32_overflow");
    let mut bad = f11_synthetic_chain(true, true, false);
    bad.mains[0].fir[1] = 1e100;
    let plan = make_plan_with_lfe(&bad, None, None, 120.0).unwrap();
    let folder = root.0.join("eqapo");
    let error = write_eqapo(&plan, &folder, "secs_mixed", "mode", "note", None).unwrap_err();
    assert!(matches!(error, FiltersError::Invalid(_)), "{error}");
    assert!(!folder.exists());
    let folder = root.0.join("text");
    let error = write_text(&plan, &folder, "filters.txt", "secs", "title", &[]).unwrap_err();
    assert!(matches!(error, FiltersError::Invalid(_)), "{error}");
    assert!(!folder.exists());

    // A low-rate FIR is checked after its expansion to 48 kHz.
    let chain = f11_synthetic_chain(true, false, true);
    let mimo = MimoPaths {
        fsiso: 120.0,
        paths: vec![MimoPath {
            input: "L".into(),
            support: "SW1".into(),
            filter: MimoFilter::FirLowRate {
                taps_low: vec![1e100, 0.5, 0.25, 0.125],
                fs_low: 1000.0,
            },
            low_hz: 30.0,
            delay_ms: 0.0,
        }],
    };
    let plan = make_plan_with_lfe(&chain, Some(&mimo), None, 120.0).unwrap();
    let folder = root.0.join("eqapo_lowrate");
    let error = write_eqapo(&plan, &folder, "secs_mixed", "mode", "note", None).unwrap_err();
    assert!(matches!(error, FiltersError::Invalid(_)), "{error}");
    assert!(!folder.exists());
}

#[test]
fn negative_delays_write_nothing_td94() {
    let root = TempDir::new("negative_delay");
    // A chain with a negative channel delay is refused when the plan is made.
    let mut bad = f11_synthetic_chain(true, false, true);
    bad.mains[0].delay_ms = -1.0;
    let error = make_plan_with_lfe(&bad, None, None, 120.0).unwrap_err();
    assert!(error.to_string().contains("\"L\""), "{error}");
    let mut bad = f11_synthetic_chain(true, false, true);
    bad.subs[0].delay_ms = -1.0;
    let error = make_plan_with_lfe(&bad, None, None, 120.0).unwrap_err();
    assert!(error.to_string().contains("\"SW1\""), "{error}");

    // A plan holding `Delay { ms: -1.0 }` is refused by both plan writers.
    let chain = f11_synthetic_chain(true, false, true);
    let mut plan = make_plan_with_lfe(&chain, None, None, 120.0).unwrap();
    for op in &mut plan.main_ops[0].1 {
        if let Op::Delay { ms, .. } = op {
            *ms = -1.0;
        }
    }
    let folder = root.0.join("eqapo");
    let error = write_eqapo(&plan, &folder, "secs_mixed", "mode", "note", None).unwrap_err();
    assert!(matches!(error, FiltersError::Invalid(_)), "{error}");
    assert!(error.to_string().contains("\"L\""), "{error}");
    assert!(!folder.exists());
    let folder = root.0.join("text");
    let error = write_text(&plan, &folder, "filters.txt", "secs", "title", &[]).unwrap_err();
    assert!(matches!(error, FiltersError::Invalid(_)), "{error}");
    assert!(!folder.exists());
}

#[test]
fn fir_responses_require_the_rfft_grid_td94() {
    let uniform = (0..=8)
        .map(|k| f64::from(k) * 48_000.0 / 16.0)
        .collect::<Vec<_>>();
    let mut skewed = uniform.clone();
    skewed[3] += 1.0;
    let fir =
        make_plan_with_lfe(&f11_synthetic_chain(true, true, false), None, None, 120.0).unwrap();
    assert!(plan_responses(&fir, &uniform, "L", false).is_ok());
    assert!(matches!(
        plan_responses(&fir, &skewed, "L", false),
        Err(FiltersError::Invalid(_))
    ));
    // Without a FIR every response is evaluated on `f` itself.
    let iir =
        make_plan_with_lfe(&f11_synthetic_chain(true, false, false), None, None, 120.0).unwrap();
    assert!(plan_responses(&iir, &skewed, "L", false).is_ok());
}

fn _public_shapes(_: &SubFeed, _: &MimoFeed) {
    let _ = goldens();
}

/// The text and Equalizer APO formats keep letters and digits only in role
/// names, although the export graph (and CamillaDSP) also takes `_` and `-`.
#[test]
fn filter_plans_refuse_roles_beyond_letters_and_digits() {
    let fixture = Fixture::load("filters", "rc_mixed");
    for (index, role) in [(0, "L_top"), (1, "R-2")] {
        let mut chain = chain(&fixture);
        chain.mains[index].role = role.into();
        let error = make_plan_with_lfe(&chain, None, Some(10.0), 120.0).unwrap_err();
        assert!(
            matches!(&error, FiltersError::Invalid(message) if message.contains("unsafe main role")),
            "{role}: {error}"
        );
    }
}

// ADR 0024: the extra correction bands join the Equalizer APO note on its one
// ASCII comment line; every line of the filter file's head stays a comment.
#[test]
fn extra_rc_bands_keep_every_eqapo_header_line_a_comment() {
    let fixture = Fixture::load("filters", "rc_mixed");
    let rc_chain = chain(&fixture);
    let run_value = &fixture.inputs()["run"];
    let (label, rc_pieces, stamp) = run_parts(&fixture);
    let mode = export_mode(run_value);
    let note = secs_camilla::note::eqapo_note_with_bands(
        &mode,
        number(&run_value["latency_ms"]),
        number(&run_value["rc_hi_hz"]),
        &[
            RcBand {
                lo_hz: 30.0,
                hi_hz: 60.0,
            },
            RcBand {
                lo_hz: 15_000.0,
                hi_hz: 20_000.0,
            },
        ],
        None,
    );
    let run = FilterRun {
        chain: &rc_chain,
        mimo: None,
        mode: &mode,
        note: &note,
        label: &label,
        latency_ms: number(&run_value["latency_ms"]),
        rc_pieces: &rc_pieces,
        stamp: &stamp,
    };
    let root = TempDir::new("rc_bands_note");
    export_eqapo(&run, &root.0).unwrap();
    let config = fs::read_to_string(root.0.join("config.txt")).unwrap();
    let body = fs::read_to_string(root.0.join(eqapo_body_path(&config))).unwrap();
    let header: Vec<_> = body.lines().take_while(|line| !line.is_empty()).collect();
    assert!(header.len() >= 3, "{header:#?}");
    assert!(
        header.iter().all(|line| line.starts_with('#')),
        "{header:#?}"
    );
    let notes: Vec<_> = header
        .iter()
        .filter(|line| line.contains("RC extra bands"))
        .collect();
    assert_eq!(notes.len(), 1, "{header:#?}");
    assert!(
        notes[0].ends_with("; no MIMO; RC extra bands 30-60 Hz, 15000-20000 Hz"),
        "{}",
        notes[0]
    );
}

fn export_mode(value: &Value) -> String {
    secs_camilla::note::mode_name(
        serde_json::from_value(value["mode"].clone()).unwrap(),
        number(&value["ultra_ms"]),
    )
}

fn export_note(mode: &str, latency: f64, rc_hi: f64, mimo: Option<&MimoPaths>) -> String {
    let mimo = mimo.and_then(|paths| secs_camilla::note::MimoNote::for_paths(paths, 0));
    secs_camilla::note::eqapo_note(mode, latency, rc_hi, mimo.as_ref())
}

/// Issue #106: with Dirac ART on a Denon receiver in Stereo mode, the
/// receiver switches the centre and surround channels off before the filter,
/// so a front's support speakers stay silent. SECS 2R forks a support signal
/// inside the exported filter from the raw input, so a two-channel source
/// (only L and R carry signal) must still reach every support speaker and
/// every bass-managed sub of L and R.
#[test]
fn stereo_source_reaches_every_support_and_sub_of_its_inputs_i106() {
    for case in ["computed_dolby", "mimo_iir"] {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let plan = make_plan(&chain, mimo.as_ref()).unwrap();
        let rendered = render_paths(&plan, 1 << 20).unwrap();
        let stereo = ["L", "R"];
        let energy = |input: &str, output: &str| {
            rendered
                .paths
                .iter()
                .find(|path| path.input == input && path.output == output)
                .map_or(0.0, |path| {
                    path.response.iter().map(|value| value * value).sum::<f64>()
                })
        };
        let mut expected = Vec::new();
        for feed in plan
            .mimo
            .iter()
            .filter(|feed| stereo.contains(&feed.input.as_str()))
        {
            expected.push((feed.input.clone(), feed.support.clone()));
        }
        for (sub, feed) in &plan.subs {
            for input in stereo
                .iter()
                .filter(|input| feed.sources.iter().any(|source| source == **input))
            {
                expected.push(((*input).to_owned(), sub.clone()));
            }
        }
        assert!(
            !expected.is_empty(),
            "{case}: L and R reach no support or sub"
        );
        for (input, output) in &expected {
            assert!(
                energy(input, output) > 0.0,
                "{case}: a stereo source leaves {input} -> {output} silent"
            );
        }
        // What each such output plays with only L and R driven is the sum of
        // those two inputs' responses, and it is not silent.
        for (_, output) in &expected {
            let driven = stereo
                .iter()
                .map(|input| energy(input, output))
                .sum::<f64>();
            assert!(
                driven > 0.0,
                "{case}: {output} is silent for a stereo source"
            );
        }
    }
}

#[test]
fn rendered_oracle_paths_match_plan_responses() {
    const REL: f64 = 1e-2;
    const ABS: f64 = 1e-3;
    let mut largest_ratio = 0.0_f64;
    let mut lengths = Vec::new();
    for case in ["rc_mixed", "computed_dolby", "mimo_iir"] {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let plan = make_plan(&chain, mimo.as_ref()).unwrap();
        let rendered = render_paths(&plan, 1 << 20).unwrap();
        let mut reference_plan = plan.clone();
        let round_delays = |ops: &mut Vec<Op>| {
            for op in ops {
                if let Op::Delay { ms, .. } = op {
                    *ms = secs_graph::round_ties_even_i64(*ms * 48.0) as f64 / 48.0;
                }
            }
        };
        for (_, ops) in &mut reference_plan.input_ops {
            round_delays(ops);
        }
        for (_, ops) in &mut reference_plan.main_ops {
            round_delays(ops);
        }
        for (_, ops) in &mut reference_plan.send_ops {
            round_delays(ops);
        }
        round_delays(&mut reference_plan.lfe_ops);
        for (_, feed) in &mut reference_plan.subs {
            round_delays(&mut feed.ops);
        }
        for feed in &mut reference_plan.mimo {
            round_delays(&mut feed.ops);
        }
        lengths.push((case, rendered.length));
        let frequencies = (0..=rendered.length / 2)
            .map(|index| index as f64 * 48_000.0 / rendered.length as f64)
            .collect::<Vec<_>>();
        let actual_pairs = rendered
            .paths
            .iter()
            .map(|path| (path.input.clone(), path.output.clone()))
            .collect::<std::collections::BTreeSet<_>>();
        let mut expected_pairs = std::collections::BTreeSet::new();
        for input in &plan.inputs {
            let expected = plan_responses(&reference_plan, &frequencies, input, false).unwrap();
            let peak = expected
                .iter()
                .flat_map(|(_, response)| response)
                .map(|value| value.norm())
                .fold(0.0_f64, f64::max);
            for (output, reference) in expected {
                if !reference.iter().any(|value| value.norm() != 0.0) {
                    continue;
                }
                expected_pairs.insert((input.clone(), output.clone()));
                let path = rendered
                    .paths
                    .iter()
                    .find(|path| path.input == *input && path.output == output)
                    .unwrap_or_else(|| panic!("missing rendered path {case}/{input}->{output}"));
                let actual = secs_dsp::fft::rfft(&path.response);
                for (index, (got, expected)) in actual.iter().zip(&reference).enumerate() {
                    let frequency = frequencies[index];
                    if !(10.0..=20_000.0).contains(&frequency) {
                        continue;
                    }
                    let error = (*got - *expected).norm();
                    let bound = REL * expected.norm() + ABS * peak;
                    let ratio = if bound == 0.0 { 0.0 } else { error / bound };
                    largest_ratio = largest_ratio.max(ratio);
                    assert!(
                        ratio <= 1.0,
                        "{case}/{input}->{output} at {frequency} Hz: {error} > {bound} ({ratio})"
                    );
                }
            }
        }
        assert_eq!(actual_pairs, expected_pairs, "{case} path set");
    }
    eprintln!("convolution oracle lengths: {lengths:?}");
    eprintln!("convolution largest error/bound: {largest_ratio:.9}");
}

fn first_coefficient(op: &Op) -> Vec<f64> {
    let normalized = |b0: f64, a0: f64| b0 / a0;
    match op {
        Op::Peq {
            frequency,
            q,
            gain_db,
            ..
        } => {
            let w = 2.0 * std::f64::consts::PI * frequency / 48_000.0;
            let alpha = w.sin() / (2.0 * q);
            let gain = 10_f64.powf(gain_db / 40.0);
            vec![normalized(1.0 + alpha * gain, 1.0 + alpha / gain)]
        }
        Op::Hp {
            frequency, kind, ..
        }
        | Op::Lp {
            frequency, kind, ..
        } => {
            let high_pass = matches!(op, Op::Hp { .. });
            SECTION_Q
                .iter()
                .find(|(name, _)| *name == kind.name())
                .unwrap()
                .1
                .iter()
                .map(|q| {
                    let w = 2.0 * std::f64::consts::PI * frequency / 48_000.0;
                    let alpha = w.sin() / (2.0 * q);
                    let b0 = if high_pass {
                        (1.0 + w.cos()) / 2.0
                    } else {
                        (1.0 - w.cos()) / 2.0
                    };
                    normalized(b0, 1.0 + alpha)
                })
                .collect()
        }
        Op::Ap { frequency, q, .. } => {
            let w = 2.0 * std::f64::consts::PI * frequency / 48_000.0;
            let alpha = w.sin() / (2.0 * q);
            vec![normalized(1.0 - alpha, 1.0 + alpha)]
        }
        _ => Vec::new(),
    }
}

#[test]
fn renderer_applies_every_operation_in_time_domain() {
    let ops = vec![
        Op::Peq {
            frequency: 1_000.0,
            q: 1.2,
            gain_db: 3.0,
            fixed: false,
        },
        Op::Fir {
            name: "full".into(),
            taps: vec![0.5, -0.25],
            fixed: false,
        },
        Op::FirLowRate {
            name: "low".into(),
            taps_low: vec![1.0, 0.0],
            fs_low: 48_000.0,
            fixed: false,
        },
        Op::Gain {
            db: 20.0 * 2.0_f64.log10(),
            fixed: false,
        },
        Op::Invert { fixed: false },
        Op::Hp {
            frequency: 80.0,
            kind: BandType::Lr24,
            fixed: false,
        },
        Op::Lp {
            frequency: 8_000.0,
            kind: BandType::Bw24,
            fixed: false,
        },
        Op::Ap {
            frequency: 250.0,
            q: 0.8,
            fixed: false,
        },
        Op::Delay {
            ms: 2.0 / 48.0,
            fixed: false,
        },
    ];
    let plan = FilterPlan {
        pre_ops: Vec::new(),
        inputs: vec!["X".into()],
        outputs: vec!["X".into()],
        input_ops: vec![("X".into(), ops.clone())],
        main_ops: vec![("X".into(), Vec::new())],
        send_ops: Vec::new(),
        lfe_ops: Vec::new(),
        subs: Vec::new(),
        mimo: Vec::new(),
    };
    let rendered = render_paths(&plan, 1 << 16).unwrap();
    let path = &rendered.paths[0];
    assert_eq!(path.response[0], 0.0);
    assert_eq!(path.response[1], 0.0);
    assert!(
        path.response[2] < 0.0,
        "polarity and gain apply at the delay"
    );
    let materialized = secs_dsp::fir::lowrate_to_48k(&[1.0, 0.0], 48_000.0);
    let sections = ops.iter().flat_map(first_coefficient).collect::<Vec<_>>();
    assert_eq!(sections.len(), 6, "PEQ + LR24 + BW24 + all-pass sections");
    let expected = -2.0 * 0.5 * materialized[0] * sections.iter().product::<f64>();
    assert!((path.response[2] - expected).abs() <= expected.abs() * 1e-13);
    assert_eq!(path.delay_ms, 2.0 / 48.0);
}

#[test]
fn convolution_bundle_contains_complete_paths_and_is_atomic() {
    let fixture = Fixture::load("filters", "computed_dolby");
    let chain = chain(&fixture);
    let run = FilterRun {
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: "",
        label: "computed Dolby",
        latency_ms: chain.latency_ms,
        rc_pieces: &[],
        stamp: "2026-10-28 00:00",
    };
    let root = TempDir::new("convolution_bundle");
    let folder = root.0.join("bundle");
    let files = export_convolution(&run, &folder).unwrap();
    let routing: Value =
        serde_json::from_slice(&fs::read(folder.join("routing.json")).unwrap()).unwrap();
    assert_eq!(routing["schema"], 1);
    assert_eq!(routing["sample_rate"], 48_000);
    assert_eq!(routing["format"], "f32");
    let length = routing["length"].as_u64().unwrap() as usize;
    let routes = routing["paths"].as_array().unwrap();
    assert_eq!(files.len(), routes.len() * 2 + 3);
    assert!(files.contains(&folder.join("secs2r_reaper.lua")));
    assert!(length.is_power_of_two() && length >= 4096);

    let plan = make_plan(&chain, None).unwrap();
    let rendered = render_paths(&plan, 1 << 20).unwrap();
    assert_eq!(length, rendered.length);
    let headroom = (headroom_db(&plan).unwrap() * 2.0).ceil() / 2.0;
    assert_eq!(routing["headroom_db"].as_f64().unwrap(), headroom);
    let attenuation = 10_f64.powf(-headroom / 20.0);
    for route in routes {
        let wav_name = route["wav"].as_str().unwrap();
        let csv_name = route["csv"].as_str().unwrap();
        let bytes = fs::read(folder.join(wav_name)).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 32);
        assert_eq!(bytes.len(), 44 + length * 4);
        let source = rendered
            .paths
            .iter()
            .find(|path| {
                path.input == route["input"].as_str().unwrap()
                    && path.output == route["output"].as_str().unwrap()
            })
            .unwrap();
        for (index, sample) in bytes[44..].as_chunks::<4>().0.iter().take(64).enumerate() {
            let actual = f32::from_le_bytes(*sample);
            let expected = (source.response[index] * attenuation) as f32;
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        let csv = fs::read_to_string(folder.join(csv_name)).unwrap();
        let rows = csv.lines().collect::<Vec<_>>();
        assert_eq!(rows[0], "frequency_hz,magnitude_db,phase_deg");
        assert_eq!(
            rows[1].split(',').next().unwrap().parse::<f64>().unwrap(),
            10.0
        );
        assert_eq!(
            rows.last()
                .unwrap()
                .split(',')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap(),
            24_000.0
        );
    }
    let guide = fs::read(folder.join(CONVOLUTION_GUIDE)).unwrap();
    assert!(guide.windows(2).any(|bytes| bytes == b"\r\n"));
    assert!(
        guide
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte != b'\n' || index > 0 && guide[index - 1] == b'\r')
    );

    let script = fs::read_to_string(folder.join("secs2r_reaper.lua")).unwrap();
    assert!(
        !script.contains('\r'),
        "script bytes do not depend on checkout line endings"
    );
    let inputs = routing["inputs"].as_array().unwrap();
    let outputs = routing["outputs"].as_array().unwrap();
    for route in routes {
        let input = inputs
            .iter()
            .position(|value| value == &route["input"])
            .unwrap()
            + 1;
        let output = outputs
            .iter()
            .position(|value| value == &route["output"])
            .unwrap()
            + 1;
        let wav = route["wav"].as_str().unwrap();
        assert_eq!(script.matches(wav).count(), 1);
        assert!(script.contains(&format!(
            "{{input = {input}, output = {output}, wav = \"{wav}\""
        )));
    }
    let repeated = root.0.join("repeated");
    let repeated_files = export_convolution(
        &FilterRun {
            stamp: "another stamp",
            ..run
        },
        &repeated,
    )
    .unwrap();
    assert_eq!(repeated_files.len(), files.len());
    for original in &files {
        assert_eq!(
            fs::read(original).unwrap(),
            fs::read(repeated.join(original.file_name().unwrap())).unwrap()
        );
    }
    let named = root.0.join("named");
    secs_filters::export_convolution_named(&run, &named, Some("거실 \"보정\"\\새 폴더\n다음"))
        .unwrap();
    let named_script = fs::read_to_string(named.join("secs2r_reaper.lua")).unwrap();
    assert!(
        named_script.starts_with("local design_name = \"거실 \\\"보정\\\"\\\\새 폴더\\010다음\"\n")
    );

    let mut invalid = chain.clone();
    invalid.sample_rate = 44_100;
    let invalid_run = FilterRun {
        chain: &invalid,
        ..run
    };
    let invalid_folder = root.0.join("invalid");
    assert!(export_convolution(&invalid_run, &invalid_folder).is_err());
    assert!(!invalid_folder.exists());
    eprintln!("convolution bundle oracle length: {length}");
}

#[derive(Default, Debug)]
struct RateDeviation {
    db: f64,
    db_hz: f64,
    db_path: String,
    degrees: f64,
    phase_hz: f64,
    phase_path: String,
    out_ratio: f64,
    out_hz: f64,
    out_path: String,
}

impl RateDeviation {
    fn observe_band(
        &mut self,
        got: secs_dsp::Complex64,
        reference: secs_dsp::Complex64,
        hz: f64,
        path: &str,
        peak: f64,
    ) {
        assert!(peak.is_finite() && peak > 0.0);
        if reference.norm() < peak * 10_f64.powf(-30.0 / 20.0) {
            let error = (got - reference).norm() / peak;
            assert!(
                error.is_finite(),
                "undefined out-of-band response: {path}, {hz} Hz"
            );
            if error > self.out_ratio {
                self.out_ratio = error;
                self.out_hz = hz;
                self.out_path = path.into();
            }
        } else {
            self.observe(got, reference, hz, path);
        }
    }

    fn observe(
        &mut self,
        got: secs_dsp::Complex64,
        reference: secs_dsp::Complex64,
        hz: f64,
        path: &str,
    ) {
        let ratio = got / reference;
        let db = (20.0 * ratio.norm().log10()).abs();
        let degrees = ratio.arg().to_degrees().abs();
        assert!(
            db.is_finite() && degrees.is_finite(),
            "undefined response: {path}, {hz} Hz"
        );
        if db > self.db {
            self.db = db;
            self.db_hz = hz;
            self.db_path = path.into();
        }
        if degrees > self.degrees {
            self.degrees = degrees;
            self.phase_hz = hz;
            self.phase_path = path.into();
        }
    }

    fn passes(&self) -> bool {
        self.db <= 0.1 && self.degrees <= 1.0 && self.out_ratio <= 10_f64.powf(-50.0 / 20.0)
    }
}

fn rate_spectrum(taps: &[f64], rate: u32) -> Vec<secs_dsp::Complex64> {
    let mut padded = taps.to_vec();
    // Two seconds, with common 0.5-Hz bins at every output rate. There is no
    // interpolation or fitted time shift. Explicit Delay ops are removed
    // from both plans before rendering, as required by ADR 0017.
    assert!(padded.len() <= 2 * rate as usize);
    padded.resize(2 * rate as usize, 0.0);
    secs_dsp::fft::rfft(&padded)
}

fn rate_path_has_short_fir(plan: &FilterPlan, input: &str, output: &str) -> bool {
    let short = |ops: &[Op]| {
        ops.iter()
            .any(|op| matches!(op, Op::Fir { taps, .. } if taps.len() < 2048))
    };
    let role_short = |items: &[(String, Vec<Op>)], role: &str| {
        items.iter().any(|(name, ops)| name == role && short(ops))
    };
    if input != "LFE"
        && input == output
        && (role_short(&plan.input_ops, input) || role_short(&plan.main_ops, input))
    {
        return true;
    }
    for (role, feed) in &plan.subs {
        if role != output || !feed.sources.iter().any(|source| source == input) {
            continue;
        }
        if input == "LFE" {
            if short(&plan.lfe_ops) || short(&feed.ops) {
                return true;
            }
        } else if let Some((_, send)) = plan.send_ops.iter().find(|(role, _)| role == input)
            && (role_short(&plan.input_ops, input) || short(send) || short(&feed.ops))
        {
            return true;
        }
    }
    plan.mimo
        .iter()
        .any(|feed| feed.input == input && feed.support == output && short(&feed.ops))
}

#[test]
fn output_rate_real_design_paths_meet_band_aware_tolerance() {
    use secs_dsp::rate::OUTPUT_RATES;
    use secs_filters::render_paths_at;
    let mut worst: [RateDeviation; 6] = std::array::from_fn(|_| RateDeviation::default());
    let mut skipped_cases = std::collections::BTreeSet::new();
    for case in [
        "rc_mixed",
        "computed_dolby",
        "mimo_iir",
        "computed_dolby_ultra",
        "computed_room_rc_mixed",
        "computed_room_rc_zero",
    ] {
        let fixture = Fixture::load("filters", case);
        let chain = chain(&fixture);
        let mimo = mimo(&fixture);
        let mut plan = make_plan(&chain, mimo.as_ref()).unwrap();
        // Compare the same chain without explicit integer Delay operations.
        // FIR phase/time reference is untouched; nothing is fitted or aligned.
        for ops in plan
            .input_ops
            .iter_mut()
            .chain(&mut plan.main_ops)
            .chain(&mut plan.send_ops)
            .map(|(_, ops)| ops)
            .chain(std::iter::once(&mut plan.lfe_ops))
            .chain(plan.subs.iter_mut().map(|(_, feed)| &mut feed.ops))
            .chain(plan.mimo.iter_mut().map(|feed| &mut feed.ops))
        {
            ops.retain(|op| !matches!(op, Op::Delay { .. }));
        }
        let baseline = render_paths(&plan, 1 << 20).unwrap();
        assert_eq!(baseline, render_paths_at(&plan, 1 << 20, 48000).unwrap());
        let references: Vec<_> = baseline
            .paths
            .iter()
            .map(|p| rate_spectrum(&p.response, 48000))
            .collect();
        for (i, rate) in OUTPUT_RATES.into_iter().enumerate() {
            let rendered = render_paths_at(&plan, 1 << 20, rate).unwrap();
            assert_eq!(rendered.rate, rate);
            assert_eq!(rendered.paths.len(), baseline.paths.len());
            let mut local = RateDeviation::default();
            let mut checked_paths = 0;
            for ((got, reference), expected) in
                rendered.paths.iter().zip(&baseline.paths).zip(&references)
            {
                assert_eq!(
                    (&got.input, &got.output),
                    (&reference.input, &reference.output)
                );
                let path = format!("{case}/{}->{}", got.input, got.output);
                if rate_path_has_short_fir(&plan, &got.input, &got.output) {
                    // Engine FIRs are at least 2048 taps. The synthetic upstream
                    // rc_mixed fixture includes a 3-tap FIR: its 62.5-us duration
                    // cannot retain the reconstruction's pre-ring and tail at
                    // another rate. Keep its crop diagnostic, but do not apply
                    // the engine-design tolerance to paths containing it.
                    assert_eq!(case, "rc_mixed", "unexpected short engine FIR on {path}");
                    skipped_cases.insert(case);
                    eprintln!(
                        "PATH skipped {path} at {rate}: synthetic FIR shorter than 2048 taps"
                    );
                    continue;
                }
                checked_paths += 1;
                let h = rate_spectrum(&got.response, rate);
                let peak = expected[40..=40000]
                    .iter()
                    .map(|h| h.norm())
                    .fold(0.0_f64, f64::max);
                for k in 40..=40000 {
                    let hz = k as f64 * 0.5;
                    local.observe_band(h[k], expected[k], hz, &path, peak);
                    worst[i].observe_band(h[k], expected[k], hz, &path, peak);
                }
            }
            // rc_mixed is wholly synthetic (a 3-tap and a 2-tap FIR), so every
            // path of it is skipped; every engine design keeps at least one.
            assert!(
                checked_paths > 0 || case == "rc_mixed",
                "no paths checked for {case} at {rate}"
            );
            eprintln!(
                "PATH {case} {rate}: {local:?}; out-of-band error relative to peak={} dB",
                20.0 * local.out_ratio.log10()
            );
        }
        // The packet explicitly calls out digital PEQ cramping. Report both
        // the inter-rate error and the unwarped analog-prototype error rather
        // than concealing either behind a looser path tolerance.
        let all_ops = plan
            .input_ops
            .iter()
            .chain(&plan.main_ops)
            .chain(&plan.send_ops)
            .flat_map(|(_, ops)| ops)
            .chain(&plan.lfe_ops)
            .chain(plan.subs.iter().flat_map(|(_, feed)| &feed.ops))
            .chain(plan.mimo.iter().flat_map(|feed| &feed.ops));
        let f: Vec<_> = (20..=20000).map(f64::from).collect();
        for op in all_ops {
            let Op::Peq {
                frequency,
                q,
                gain_db,
                ..
            } = op
            else {
                continue;
            };
            let reference =
                secs_dsp::response::peaking_response(&f, *frequency, *q, *gain_db, 48000.0);
            for rate in OUTPUT_RATES {
                if rate == 48000 {
                    continue;
                }
                let h = secs_dsp::response::peaking_response(
                    &f,
                    *frequency,
                    *q,
                    *gain_db,
                    f64::from(rate),
                );
                let mut digital = RateDeviation::default();
                let mut analog = RateDeviation::default();
                for ((&f, &got), &expected) in f.iter().zip(&h).zip(&reference) {
                    digital.observe(got, expected, f, case);
                    let a = 10_f64.powf(*gain_db / 40.0);
                    let s = secs_dsp::Complex64::new(0.0, f / frequency);
                    let prototype = (s * s + s * (a / q) + 1.0) / (s * s + s / (a * q) + 1.0);
                    analog.observe(got, prototype, f, case);
                }
                if !digital.passes() {
                    eprintln!(
                        "PEQ {case} f0={frequency} Q={q} gain={gain_db} {rate}: digital={digital:?}; analog={analog:?}"
                    );
                }
            }
        }
    }
    assert_eq!(
        skipped_cases,
        std::collections::BTreeSet::from(["rc_mixed"])
    );
    for (rate, error) in OUTPUT_RATES.into_iter().zip(&worst) {
        eprintln!(
            "PATH worst {rate}: {error:?}; out-of-band error relative to peak={} dB at {} Hz on {}",
            20.0 * error.out_ratio.log10(),
            error.out_hz,
            error.out_path
        );
    }
    assert!(
        worst.iter().all(RateDeviation::passes),
        "ADR 0017 complete-path tolerance not met"
    );
}

#[test]
fn output_rate_long_fir_conversion_timing() {
    let mut taps = vec![0.0; 16384];
    taps[8192] = 1.0;
    for (i, value) in taps.iter_mut().enumerate() {
        *value += 0.001 * (-(i as f64 - 8192.0).abs() / 300.0).exp();
    }
    let started = std::time::Instant::now();
    // No worker pool: the conversion and its FFTs run on this one thread.
    let result = secs_dsp::rate::fir_to_rate(&taps, 192000).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(result.len(), 65536);
    eprintln!("RATE TIMING 16384 -> 65536 taps at 192000 Hz, one thread, cold plans: {elapsed:?}");
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "output-rate conversion exceeds 2 seconds"
        );
    }
}

#[test]
fn output_rate_renderer_rounds_delays_and_scales_bounds() {
    let plan = FilterPlan {
        pre_ops: Vec::new(),
        inputs: vec!["L".into()],
        outputs: vec!["L".into()],
        input_ops: vec![("L".into(), vec![])],
        main_ops: vec![(
            "L".into(),
            vec![Op::Delay {
                ms: 1.0,
                fixed: false,
            }],
        )],
        send_ops: vec![],
        lfe_ops: vec![],
        subs: vec![],
        mimo: vec![],
    };
    for rate in secs_dsp::rate::OUTPUT_RATES {
        let got = secs_filters::render_paths_at(&plan, 8192, rate).unwrap();
        let delay = secs_dsp::rate::delay_samples(1.0, rate) as usize;
        assert_eq!(got.paths[0].response[delay], 1.0);
        assert!(got.paths[0].response[..delay].iter().all(|&v| v == 0.0));
        assert!(got.length <= secs_dsp::rate::taps_at(8192, rate));
        assert_eq!(got.rate, rate);
    }
    assert!(secs_filters::render_paths_at(&plan, 8192, 32000).is_err());
    assert!(secs_filters::render_paths_at(&plan, 4095, 96000).is_err());
}

#[test]
fn output_rate_rc_mixed_crop_diagnostic() {
    use secs_dsp::{Complex64 as C, fft, rate::*};
    use std::f64::consts::PI;
    let fixture = Fixture::load("filters", "rc_mixed");
    let chain = chain(&fixture);
    let taps = &chain
        .mains
        .iter()
        .find(|main| main.role == "L")
        .unwrap()
        .fir;
    let mut peak = 0;
    for i in 1..taps.len() {
        if taps[i].abs() > taps[peak].abs() {
            peak = i;
        }
    }
    let mut refined = peak as f64;
    if peak > 0 && peak + 1 < taps.len() {
        let left = taps[peak - 1].abs() / taps[peak].abs();
        let right = taps[peak + 1].abs() / taps[peak].abs();
        if left - 2.0 + right != 0.0 {
            refined += (0.5 * (left - right) / (left - 2.0 + right)).clamp(-0.5, 0.5);
        }
    }
    let energy: f64 = taps.iter().map(|v| v * v).sum();
    let first: f64 = taps[..32.min(taps.len())].iter().map(|v| v * v).sum();
    let last = (taps.len() as f64 * 0.1) as usize;
    let tail: f64 = taps[taps.len() - last..].iter().map(|v| v * v).sum();
    eprintln!(
        "RATE CROP rc_mixed/L->L: taps={}, peak index={peak}, refined={refined}, first 32 energy share={}, last 10% energy share={}",
        taps.len(),
        first / energy,
        tail / energy
    );
    for rate in OUTPUT_RATES.into_iter().filter(|&rate| rate != DESIGN_RATE) {
        let length = taps_at(taps.len(), rate);
        let nfft = (64 * length)
            .max(taps_at(1 << 17, rate))
            .next_power_of_two();
        let step = f64::from(rate) / nfft as f64;
        let end = 23000.0_f64.min(f64::from(rate) / 2.0);
        let bins = (end / step).floor() as usize + 1;
        let f: Vec<_> = (0..bins).map(|k| k as f64 * step).collect();
        // Independent direct DTFT, not the production chirp convolution.
        let mut target = fir_response(taps, DESIGN_RATE, &f);
        let magnitude = fir_response(taps, DESIGN_RATE, &[20000.0])[0].norm();
        let extension = |f: f64| C::from_polar(magnitude, -2.0 * PI * f * refined / 48000.0);
        for (k, h) in target.iter_mut().enumerate() {
            if f[k] > 20000.0 {
                let weight = 0.5 * (1.0 - (PI * (f[k] - 20000.0) / (end - 20000.0)).cos());
                *h = *h * (1.0 - weight) + extension(f[k]) * weight;
            }
        }
        target.extend((bins..=nfft / 2).map(|k| extension(k as f64 * step)));
        let full = fft::irfft(&target, nfft);
        let converted = fir_to_rate(taps, rate).unwrap();
        assert!(
            converted
                .iter()
                .zip(&full)
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
        // The second half of this very long inverse period represents t<0.
        // Split the omitted samples into that pre-ring and the causal tail.
        let mut before = full.clone();
        before[..nfft / 2].fill(0.0);
        let mut after = full;
        after[..length].fill(0.0);
        after[nfft / 2..].fill(0.0);
        let before = fft::rfft(&before);
        let after = fft::rfft(&after);
        let first = (20.0 / step).ceil() as usize;
        let last = (20000.0 / step).floor() as usize;
        let peak = target[first..=last]
            .iter()
            .map(|h| h.norm())
            .fold(0.0_f64, f64::max);
        let mut worst = first;
        for k in first..=last {
            if (before[k] + after[k]).norm() > (before[worst] + after[worst]).norm() {
                worst = k;
            }
        }
        eprintln!(
            "RATE CROP {rate}: worst complex crop error at {} Hz; pre-ring={} dB/peak, end={} dB/peak, combined={} dB/peak",
            worst as f64 * step,
            20.0 * (before[worst].norm() / peak).log10(),
            20.0 * (after[worst].norm() / peak).log10(),
            20.0 * ((before[worst] + after[worst]).norm() / peak).log10()
        );
    }
}

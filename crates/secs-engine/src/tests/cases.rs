//! The shared input loader for every engine golden family.

use crate::input::*;
use secs_oracle::{Fixture, integer, number};
use secs_types::target::TargetCurve;
use serde_json::Value;

#[allow(dead_code)]
pub(crate) struct Case {
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub tuning: Tuning,
    pub settings: Value,
    pub project: Value,
}

pub(crate) fn names() -> Vec<String> {
    Fixture::all("engine_inputs", "")
        .iter()
        .map(|f| f.id.strip_prefix("engine_inputs/").unwrap().to_owned())
        .collect()
}

pub(crate) fn pairs(v: &Value) -> impl Iterator<Item = (&str, &Value)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| (v[0].as_str().unwrap(), &v[1]))
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn optional(v: &Value) -> Option<f64> {
    (!v.is_null()).then(|| number(v))
}

pub(crate) fn manual(v: &Value) -> SubManual {
    SubManual {
        mode: match v["mode"].as_str().unwrap() {
            "fixed" => SubMode::Fixed,
            "bundle" => SubMode::Bundle,
            "auto" => SubMode::Auto,
            other => panic!("unknown mode {other}"),
        },
        bundle: v["bundle"].as_str().unwrap().into(),
        gain_db: number(&v["gain_db"]),
        delay_ms: number(&v["delay_ms"]),
        polarity: v["polarity"].as_i64().unwrap() as i8,
        allpass: v["allpass"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| (number(&b[0]), number(&b[1])))
            .collect(),
        mimo_support: v["mimo_support"].as_bool().unwrap(),
        hp_hz: number(&v["hp_hz"]),
        hp_type: BandType::from_name(v["hp_type"].as_str().unwrap()),
        lp_hz: number(&v["lp_hz"]),
        lp_type: BandType::from_name(v["lp_type"].as_str().unwrap()),
        peq: peq(&v["peq"]),
        dba_row: if v["dba_row"] == "rear" {
            DbaRow::Rear
        } else {
            DbaRow::Front
        },
    }
}
pub(crate) fn peq(v: &Value) -> Vec<Peq> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| (number(&b[0]), number(&b[1]), number(&b[2])))
        .collect()
}

pub(crate) fn load(case: &str) -> Case {
    let fx = Fixture::load("engine_inputs", case);
    let i = fx.inputs();
    let v = &i["config"];
    let config = EngineConfig {
        mains: strings(&v["mains"]),
        subs: strings(&v["subs"]),
        groups: pairs(&v["groups"])
            .map(|(g, rs)| (g.to_owned(), strings(rs)))
            .collect(),
        target: TargetCurve::new(
            v["target"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (number(&p[0]), number(&p[1])))
                .collect(),
        ),
        xo: pairs(&v["xo"])
            .map(|(g, v)| (g.to_owned(), number(v)))
            .collect(),
        main_position: integer(&v["main_position"]) as u32,
        taps: integer(&v["taps"]) as usize,
        lift_ms: number(&v["lift_ms"]),
        max_boost_db: number(&v["max_boost_db"]),
        peq_bands: (!v["peq_bands"].is_null()).then(|| integer(&v["peq_bands"]) as usize),
        max_latency_ms: optional(&v["max_latency_ms"]),
        rc_range: (number(&v["rc_range"][0]), optional(&v["rc_range"][1])),
        room_volume_m3: optional(&v["room_volume_m3"]),
        phase_frac: number(&v["phase_frac"]),
        sub_manual: pairs(&v["sub_manual"])
            .map(|(r, m)| (r.to_owned(), manual(m)))
            .collect(),
        sub_main_delay_ms: number(&v["sub_main_delay_ms"]),
        bundle_layouts: pairs(&v["bundle_layouts"])
            .map(|(b, v)| {
                (
                    b.to_owned(),
                    BundleLayout {
                        kind: if v["kind"] == "dba" {
                            LayoutKind::Dba
                        } else {
                            LayoutKind::Multi
                        },
                        rear_trim_ms: number(&v["rear_trim_ms"]),
                    },
                )
            })
            .collect(),
        infra_boost_max_db: number(&v["infra_boost_max_db"]),
        xo_peq_on: v["xo_peq_on"].as_bool().unwrap(),
        xo_peq_lo_hz: number(&v["xo_peq_lo_hz"]),
        xo_peq_hi_hz: number(&v["xo_peq_hi_hz"]),
    };
    let positions = i["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let roles = pairs(&p[1])
                .map(|(r, m)| {
                    let bytes = fx.blob(&m["samples"]);
                    let (chunks, rest) = bytes.as_chunks::<4>();
                    assert!(rest.is_empty());
                    let samples: Vec<_> = chunks.iter().map(|b| f32::from_le_bytes(*b)).collect();
                    (
                        r.to_owned(),
                        Measurement {
                            samples: Samples::F32(samples.into()),
                            sample_rate: integer(&m["sample_rate"]) as u32,
                            t0: number(&m["t0"]),
                        },
                    )
                })
                .collect();
            (integer(&p[0]) as u32, roles)
        })
        .collect();
    Case {
        measurements: MeasurementSet { positions },
        config,
        tuning: Tuning::default(),
        settings: i["settings"].clone(),
        project: i["project"].clone(),
    }
}

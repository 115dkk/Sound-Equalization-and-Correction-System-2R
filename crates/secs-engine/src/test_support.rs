//! Oracle fixture support for dependent crate parity tests.

use std::collections::BTreeMap;

use secs_compute::Context;
use secs_oracle::{Fixture, PLATFORM_RTOL, number};
use serde_json::Value;

use crate::{
    Design, DesignRequest, EngineConfig, FitRequest, FitStage, FitterFactory, Measurement,
    MeasurementSet, Samples, Tuning, design,
    fit::{RecordedFit, replay_fitter},
    input::{BundleLayout, DbaRow, LayoutKind, SubManual, SubMode},
};

/// Inputs and replayed output of one shared engine case.
#[derive(Debug)]
pub struct ReplayCase {
    pub design: Design,
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub settings: Value,
}

fn pairs(value: &Value) -> impl Iterator<Item = (&str, &Value)> {
    value
        .as_array()
        .expect("ordered pairs")
        .iter()
        .map(|v| (v[0].as_str().expect("pair key"), &v[1]))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("strings")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

fn optional(value: &Value) -> Option<f64> {
    (!value.is_null()).then(|| number(value))
}

fn peq(value: &Value) -> Vec<(f64, f64, f64)> {
    value
        .as_array()
        .expect("peq")
        .iter()
        .map(|v| (number(&v[0]), number(&v[1]), number(&v[2])))
        .collect()
}

fn manual(value: &Value) -> SubManual {
    SubManual {
        mode: match value["mode"].as_str().expect("sub mode") {
            "fixed" => SubMode::Fixed,
            "bundle" => SubMode::Bundle,
            "auto" => SubMode::Auto,
            other => panic!("unknown sub mode {other}"),
        },
        bundle: value["bundle"].as_str().expect("bundle").into(),
        gain_db: number(&value["gain_db"]),
        delay_ms: number(&value["delay_ms"]),
        polarity: value["polarity"].as_i64().expect("polarity") as i8,
        allpass: value["allpass"]
            .as_array()
            .expect("allpass")
            .iter()
            .map(|v| (number(&v[0]), number(&v[1])))
            .collect(),
        mimo_support: value["mimo_support"].as_bool().expect("mimo_support"),
        hp_hz: number(&value["hp_hz"]),
        hp_type: crate::input::BandType::from_name(value["hp_type"].as_str().expect("hp_type")),
        lp_hz: number(&value["lp_hz"]),
        lp_type: crate::input::BandType::from_name(value["lp_type"].as_str().expect("lp_type")),
        peq: peq(&value["peq"]),
        dba_row: if value["dba_row"] == "rear" {
            DbaRow::Rear
        } else {
            DbaRow::Front
        },
    }
}

fn inputs(name: &str) -> (MeasurementSet, EngineConfig, Value) {
    let fixture = Fixture::load("engine_inputs", name);
    let input = fixture.inputs();
    let value = &input["config"];
    let config = EngineConfig {
        mains: strings(&value["mains"]),
        subs: strings(&value["subs"]),
        groups: pairs(&value["groups"])
            .map(|(g, r)| (g.into(), strings(r)))
            .collect(),
        target: secs_types::target::TargetCurve::new(
            value["target"]
                .as_array()
                .expect("target")
                .iter()
                .map(|p| (number(&p[0]), number(&p[1])))
                .collect(),
        ),
        xo: pairs(&value["xo"])
            .map(|(g, v)| (g.into(), number(v)))
            .collect(),
        main_position: value["main_position"].as_u64().expect("position") as u32,
        taps: value["taps"].as_u64().expect("taps") as usize,
        lift_ms: number(&value["lift_ms"]),
        max_boost_db: number(&value["max_boost_db"]),
        peq_bands: value["peq_bands"].as_u64().map(|v| v as usize),
        max_latency_ms: optional(&value["max_latency_ms"]),
        rc_range: (
            number(&value["rc_range"][0]),
            optional(&value["rc_range"][1]),
        ),
        room_volume_m3: optional(&value["room_volume_m3"]),
        phase_frac: number(&value["phase_frac"]),
        sub_manual: pairs(&value["sub_manual"])
            .map(|(r, v)| (r.into(), manual(v)))
            .collect(),
        sub_main_delay_ms: number(&value["sub_main_delay_ms"]),
        bundle_layouts: pairs(&value["bundle_layouts"])
            .map(|(name, v)| {
                (
                    name.into(),
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
        infra_boost_max_db: number(&value["infra_boost_max_db"]),
        xo_peq_on: value["xo_peq_on"].as_bool().expect("xo_peq_on"),
        xo_peq_lo_hz: number(&value["xo_peq_lo_hz"]),
        xo_peq_hi_hz: number(&value["xo_peq_hi_hz"]),
    };
    let positions = input["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .map(|p| {
            let roles = pairs(&p[1])
                .map(|(role, m)| {
                    let bytes = fixture.blob(&m["samples"]);
                    let (chunks, rest) = bytes.as_chunks::<4>();
                    assert!(rest.is_empty());
                    let samples: Vec<_> = chunks.iter().map(|b| f32::from_le_bytes(*b)).collect();
                    (
                        role.into(),
                        Measurement {
                            samples: Samples::F32(samples.into()),
                            sample_rate: m["sample_rate"].as_u64().expect("sample rate") as u32,
                            t0: number(&m["t0"]),
                        },
                    )
                })
                .collect();
            (p[0].as_u64().expect("position") as u32, roles)
        })
        .collect();
    (
        MeasurementSet { positions },
        config,
        input["settings"].clone(),
    )
}

fn recorded(fixture: &Fixture, input: &Value, result: &Value) -> RecordedFit {
    RecordedFit {
        g: fixture.f64s(&input["g"]),
        corr_db: fixture.f64s(&input["corr_db"]),
        n: input["n"].as_u64().expect("n") as usize,
        lo: number(&input["lo"]),
        hi: number(&input["hi"]),
        boost: number(&input["boost"]),
        ring_ms: optional(&input["ring_ms"]),
        f_max: optional(&input["f_max"]),
        result: peq(result),
    }
}

/// Runs a shared oracle input with production fitters for release benchmarks.
/// Unlike replay, this supports cases whose BA path diverges from the oracle.
pub fn production_case(name: &str, threads: usize) -> ReplayCase {
    let (measurements, config, settings) = inputs(name);
    let ctx = Context::new(
        threads,
        secs_compute::MemoryBudget::new(u64::MAX),
        std::sync::Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let design = design(
        &DesignRequest {
            measurements: &measurements,
            config: &config,
            tuning: &Tuning::default(),
            subs_only: false,
            adapter_notes: &[],
        },
        &ctx,
        &*crate::production_fitters(ctx.cancel()),
    )
    .expect("benchmark design");
    ReplayCase {
        design,
        measurements,
        config,
        settings,
    }
}

/// Replays the recorded fitter decisions for a shared oracle case.
pub fn replay_case(name: &str) -> ReplayCase {
    let (measurements, config, settings) = inputs(name);
    let mut stages: BTreeMap<String, Vec<RecordedFit>> = BTreeMap::new();
    for call in Fixture::all_calls("engine_design", &format!("{name}_fits_")) {
        let fit = if let Some(reference) = call.inputs().get("reference") {
            let source = Fixture::load(
                reference["family"].as_str().expect("family"),
                reference["fixture"].as_str().expect("fixture"),
            );
            if let Some(index) = reference["call"].as_u64() {
                let source_call = &source.calls()[index as usize];
                recorded(
                    source_call,
                    source_call.inputs(),
                    &source_call.outputs()["result"],
                )
            } else {
                let fit = &source.outputs()["candidates"]
                    [reference["candidate"].as_u64().expect("candidate") as usize]["fits"]
                    [reference["fit"].as_u64().expect("fit") as usize];
                recorded(&source, &fit["inputs"], &fit["result"])
            }
        } else {
            recorded(&call, call.inputs(), &call.outputs()["result"])
        };
        stages
            .entry(call.inputs()["stage"].as_str().expect("stage").into())
            .or_default()
            .push(fit);
    }
    let factory: Box<FitterFactory<'_>> = Box::new(move |stage| {
        let key = match stage {
            FitStage::Ba => "Ba".into(),
            FitStage::Top(role) => format!("Top:{role}"),
            FitStage::Rc(role) => format!("Rc:{role}"),
        };
        let mut fitter =
            replay_fitter(stages.get(&key).cloned().unwrap_or_default(), PLATFORM_RTOL);
        Box::new(move |request: &FitRequest<'_>| fitter(request))
    });
    let design = design(
        &DesignRequest {
            measurements: &measurements,
            config: &config,
            tuning: &Tuning::default(),
            subs_only: false,
            adapter_notes: &[],
        },
        &Context::serial(),
        &*factory,
    )
    .expect("recorded design replay");
    ReplayCase {
        design,
        measurements,
        config,
        settings,
    }
}

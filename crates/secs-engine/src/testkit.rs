//! Oracle fixture support for dependent crate parity tests.

use std::collections::BTreeMap;

use secs_compute::Context;
use secs_types::target::TargetCurve;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Supplies oracle fixtures without making the published engine depend on the
/// unpublished `secs-oracle` crate.
pub trait FixtureStore {
    fn load(&self, dir: &str, name: &str) -> Value;
    fn names(&self, dir: &str, prefix: &str) -> Vec<String>;
    fn calls(&self, dir: &str, prefix: &str) -> Vec<(String, Value)>;
    fn f64s(&self, dir: &str, name: &str, value: &Value) -> Vec<f64>;
    fn blob(&self, dir: &str, name: &str, value: &Value) -> Vec<u8>;
    fn assert_digest(&self, dir: &str, name: &str, field: &str, descriptor: &Value, got: &[f64]);
    fn platform_rtol(&self) -> f64;
    fn oracle_libm(&self) -> bool;
}

/// Copied from `secs-oracle` because a published crate cannot depend on it
/// (audit #186 A9).
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("nan") => f64::NAN,
        Some("+inf" | "inf") => f64::INFINITY,
        Some("-inf") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")),
    }
}

/// Copied from `secs-oracle` because a published crate cannot depend on it
/// (audit #186 A9).
fn integer(v: &Value) -> u64 {
    v.as_u64()
        .unwrap_or_else(|| panic!("not a non-negative integer: {v}"))
}

/// Copied from `secs-oracle` because a published crate cannot depend on it
/// (audit #186 A9).
fn errors(got: &[f64], expected: &[f64]) -> (f64, f64) {
    assert_eq!(got.len(), expected.len(), "shape");
    let mut worst = (0.0_f64, 0.0_f64);
    for (&g, &e) in got.iter().zip(expected) {
        let (abs, rel) = if g.is_nan() && e.is_nan() || g == e {
            (0.0, 0.0)
        } else if !g.is_finite() || !e.is_finite() {
            (f64::INFINITY, f64::INFINITY)
        } else {
            let abs = (g - e).abs();
            (abs, abs / e.abs().max(1.0))
        };
        worst = (worst.0.max(abs), worst.1.max(rel));
    }
    worst
}

pub use crate::fit::{RecordedFit, replay_fitter};

use crate::{
    Design, DesignRequest, EngineConfig, FitStage, FitterFactory, Measurement, MeasurementSet,
    Samples, Tuning, design,
    input::{BandType, BundleLayout, DbaRow, LayoutKind, Peq, SubManual, SubMode},
};

/// Inputs and replayed output of one shared engine case.
#[derive(Debug)]
pub struct ReplayCase {
    pub design: Design,
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub settings: Value,
}

#[derive(Debug)]
pub struct Case {
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub tuning: Tuning,
    pub settings: Value,
    pub project: Value,
}

pub fn names(store: &dyn FixtureStore) -> Vec<String> {
    store.names("engine_inputs", "")
}

pub fn pairs(v: &Value) -> impl Iterator<Item = (&str, &Value)> {
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

pub fn manual(v: &Value) -> SubManual {
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
pub fn peq(v: &Value) -> Vec<Peq> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| (number(&b[0]), number(&b[1]), number(&b[2])))
        .collect()
}

pub fn load(store: &dyn FixtureStore, name: &str) -> Case {
    let large = name == "big_fir";
    let dir = if name == "big" || large {
        "perf_mimo"
    } else {
        "engine_inputs"
    };
    let fixture_name = if large { "big" } else { name };
    let fixture = store.load(dir, fixture_name);
    let mut case = load_fixture(store, dir, fixture_name, &fixture);
    if name == "big" || large {
        case.measurements = big_measurements(store, dir, fixture_name, &fixture, large);
    }
    if large {
        expand_config(&mut case.config);
    }
    case
}

pub fn load_fixture(store: &dyn FixtureStore, dir: &str, name: &str, fixture: &Value) -> Case {
    let i = &fixture["inputs"];
    let v = &i["config"];
    let config = EngineConfig {
        mains: strings(&v["mains"]),
        inverted_mains: Vec::new(),
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
        rc_bands: Vec::new(),
        room_volume_m3: optional(&v["room_volume_m3"]),
        rc_hi_basis: secs_types::calibration::RcHiBasis::Strict,
        rc_hi_transition: 1,
        low_protect: false,
        input_hp_hz: None,
        lf_align: false,
        mode_decay: false,
        hf_phase: false,
        dip_limit: false,
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
                    let bytes = store.blob(dir, name, &m["samples"]);
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

fn expand_config(config: &mut EngineConfig) {
    use secs_types::role::{ROLES, SUB_ROLES, role_group};
    config.mains = ROLES
        .iter()
        .filter(|r| !SUB_ROLES.contains(r))
        .map(|r| (*r).to_owned())
        .collect();
    config.subs = SUB_ROLES.iter().map(|r| (*r).to_owned()).collect();
    config.groups.clear();
    for &role in &ROLES {
        let group = role_group(role);
        if let Some((_, roles)) = config.groups.iter_mut().find(|(g, _)| g == group) {
            roles.push(role.into());
        } else {
            config.groups.push((group.into(), vec![role.into()]));
        }
    }
    config.xo = config
        .groups
        .iter()
        .filter(|(g, _)| g != "sub")
        .map(|(g, _)| (g.clone(), 80.0))
        .collect();
    config.sub_manual = config
        .subs
        .iter()
        .map(|role| {
            (
                role.clone(),
                SubManual {
                    mode: SubMode::Fixed,
                    bundle: "A".into(),
                    gain_db: 0.0,
                    delay_ms: 0.0,
                    polarity: 1,
                    allpass: Vec::new(),
                    mimo_support: true,
                    hp_hz: 0.0,
                    hp_type: crate::input::BandType::Lr24,
                    lp_hz: 0.0,
                    lp_type: crate::input::BandType::Lr24,
                    peq: Vec::new(),
                    dba_row: DbaRow::Front,
                },
            )
        })
        .collect();
}

pub(crate) fn read_recorded_fit(
    store: &dyn FixtureStore,
    dir: &str,
    name: &str,
    input: &Value,
    result: &Value,
) -> RecordedFit {
    RecordedFit {
        g: store.f64s(dir, name, &input["g"]),
        corr_db: store.f64s(dir, name, &input["corr_db"]),
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
pub fn production_case(store: &dyn FixtureStore, name: &str, threads: usize) -> ReplayCase {
    let Case {
        measurements,
        config,
        tuning,
        settings,
        ..
    } = load(store, name);
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
            tuning: &tuning,
            subs_only: false,
            adapter_notes: &[],
        },
        &ctx,
        &*crate::production_fitters(&ctx),
    )
    .expect("benchmark design");
    ReplayCase {
        design,
        measurements,
        config,
        settings,
    }
}

fn fixture_calls(value: &Value) -> Vec<Value> {
    let (Some(inputs), Some(outputs)) = (
        value["inputs"]["calls"].as_array(),
        value["outputs"]["calls"].as_array(),
    ) else {
        return vec![value.clone()];
    };
    assert_eq!(inputs.len(), outputs.len(), "fixture calls");
    inputs
        .iter()
        .zip(outputs)
        .map(|(input, output)| {
            serde_json::json!({"inputs": input, "outputs": output, "meta": value["meta"]})
        })
        .collect()
}

fn fixture_name(id: &str) -> &str {
    let name = id.rsplit_once('/').map_or(id, |(_, name)| name);
    name.split_once('#').map_or(name, |(name, _)| name)
}

fn verify_request_hash(
    store: &dyn FixtureStore,
    dir: &str,
    name: &str,
    fit: &RecordedFit,
    hashes: &Value,
) {
    for (key, values) in [("g", &fit.g), ("corr_db", &fit.corr_db)] {
        store.assert_digest(
            dir,
            name,
            key,
            &serde_json::json!({
                "digest": hashes[key], "length": values.len(), "first": [], "last": []
            }),
            values,
        );
    }
}

/// Bit-equal request and result. SECS 2 records the phase-independent RC
/// preparation once per phase candidate; the channel computes it once.
pub(crate) fn same_recorded_fit(a: &RecordedFit, b: &RecordedFit) -> bool {
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    bits(&a.g) == bits(&b.g)
        && bits(&a.corr_db) == bits(&b.corr_db)
        && (a.n, a.lo.to_bits(), a.hi.to_bits(), a.boost.to_bits())
            == (b.n, b.lo.to_bits(), b.hi.to_bits(), b.boost.to_bits())
        && a.ring_ms.map(f64::to_bits) == b.ring_ms.map(f64::to_bits)
        && a.f_max.map(f64::to_bits) == b.f_max.map(f64::to_bits)
        && a.result
            .iter()
            .map(|&(f, q, g)| [f.to_bits(), q.to_bits(), g.to_bits()])
            .eq(b
                .result
                .iter()
                .map(|&(f, q, g)| [f.to_bits(), q.to_bits(), g.to_bits()]))
}

/// Checked replay of one oracle design, including shared RC preparation.
#[derive(Debug)]
pub struct Replay {
    name: String,
    stages: BTreeMap<String, Vec<RecordedFit>>,
    counts: Arc<Mutex<BTreeMap<String, (usize, usize)>>>,
    platform_rtol: f64,
    oracle_libm: bool,
}

impl Replay {
    pub fn for_case(store: &dyn FixtureStore, name: &str) -> Self {
        let mut stages: BTreeMap<String, Vec<RecordedFit>> = BTreeMap::new();
        for (id, call) in store.calls("engine_design", &format!("{name}_fits_")) {
            let input = &call["inputs"];
            let output = &call["outputs"];
            let recorded = if let Some(reference) = input.get("reference") {
                let family = reference["family"].as_str().unwrap();
                let source_name = reference["fixture"].as_str().unwrap();
                let source = store.load(family, source_name);
                let fit = if let Some(index) = reference["call"].as_u64() {
                    let calls = fixture_calls(&source);
                    let call = &calls[index as usize];
                    read_recorded_fit(
                        store,
                        family,
                        source_name,
                        &call["inputs"],
                        &call["outputs"]["result"],
                    )
                } else {
                    let fit = &source["outputs"]["candidates"]
                        [reference["candidate"].as_u64().unwrap() as usize]["fits"]
                        [reference["fit"].as_u64().unwrap() as usize];
                    read_recorded_fit(store, family, source_name, &fit["inputs"], &fit["result"])
                };
                verify_request_hash(
                    store,
                    family,
                    source_name,
                    &fit,
                    &reference["request_sha256"],
                );
                fit
            } else {
                let source_name = fixture_name(&id);
                let fit = read_recorded_fit(
                    store,
                    "engine_design",
                    source_name,
                    input,
                    &output["result"],
                );
                verify_request_hash(
                    store,
                    "engine_design",
                    source_name,
                    &fit,
                    &input["request_sha256"],
                );
                fit
            };
            stages
                .entry(input["stage"].as_str().unwrap().into())
                .or_default()
                .push(recorded);
        }
        // The RC channel evaluates the phase-independent preparation once for
        // both phase candidates (ADR 0012); SECS 2 recorded it per candidate.
        for (key, calls) in &mut stages {
            if key.starts_with("Rc:")
                && let Some(k) = (1..calls.len()).find(|&k| same_recorded_fit(&calls[0], &calls[k]))
            {
                let shared = calls[k..]
                    .iter()
                    .zip(&calls[..])
                    .take_while(|(a, b)| same_recorded_fit(a, b))
                    .count();
                if calls.len() == 2 * k && shared == k {
                    calls.truncate(k);
                } else {
                    // The FIR preparation holds at most the first PEQ fit.
                    assert!(shared <= 1, "{name}/{key} shared preparation fit");
                    calls.drain(k..k + shared);
                }
            }
        }
        let counts: Arc<Mutex<BTreeMap<String, (usize, usize)>>> = Arc::new(Mutex::new(
            stages
                .iter()
                .map(|(key, calls)| (key.clone(), (0, calls.len())))
                .collect(),
        ));
        Self {
            name: name.into(),
            stages,
            counts,
            platform_rtol: store.platform_rtol(),
            oracle_libm: store.oracle_libm(),
        }
    }

    pub fn factory(&self) -> Box<FitterFactory<'_>> {
        let seen = self.counts.clone();
        let stages = &self.stages;
        let name = self.name.as_str();
        let oracle_libm = self.oracle_libm;
        let platform_rtol = self.platform_rtol;
        Box::new(move |stage| {
            let key = match stage {
                FitStage::Ba => "Ba".into(),
                FitStage::Top(r) => format!("Top:{r}"),
                FitStage::Rc(r) => format!("Rc:{r}"),
            };
            let calls = stages.get(&key).cloned().unwrap_or_default();
            let expected = calls.len();
            let report_calls = (name == "room" || !oracle_libm).then(|| calls.clone());
            let mut next = 0;
            let mut fit = replay_fitter(
                calls,
                if name == "room" || !oracle_libm {
                    f64::INFINITY
                } else {
                    platform_rtol
                },
            );
            let seen = seen.clone();
            seen.lock().unwrap().insert(key.clone(), (0, expected));
            Box::new(move |r| {
                seen.lock().unwrap().get_mut(&key).unwrap().0 += 1;
                if let Some(calls) = &report_calls {
                    let expected = &calls[next];
                    let (_, rel) = errors(r.corr_db, &expected.corr_db);
                    eprintln!(
                        "MEASURE design {name}/{key}/fit{next} corr_db max_rel={rel:e} report-only (room BA or foreign-libm thresholds)"
                    );
                }
                next += 1;
                fit(r)
            })
        })
    }

    /// Assert that every recorded stage consumed exactly its expected calls.
    pub fn finish(self) {
        for (key, (actual, expected)) in self.counts.lock().unwrap().iter() {
            assert_eq!(actual, expected, "{} {key} count", self.name);
        }
    }
}

#[cfg(test)]
pub(crate) fn replay(store: &dyn FixtureStore, name: &str) -> Design {
    replay_case(store, name).design
}

#[cfg(test)]
pub(crate) fn replay_with_context(
    store: &dyn FixtureStore,
    name: &str,
    context: &Context,
) -> Design {
    replay_case_with_context(store, name, context).design
}

/// Replays the recorded fitter decisions and checks requests and call counts.
pub fn replay_case(store: &dyn FixtureStore, name: &str) -> ReplayCase {
    replay_case_with_context(store, name, &Context::serial())
}

fn replay_case_with_context(store: &dyn FixtureStore, name: &str, context: &Context) -> ReplayCase {
    let case = load(store, name);
    let replay = Replay::for_case(store, name);
    let design = design(
        &DesignRequest {
            measurements: &case.measurements,
            config: &case.config,
            tuning: &case.tuning,
            subs_only: false,
            adapter_notes: &[],
        },
        context,
        &*replay.factory(),
    )
    .expect("recorded design replay");
    replay.finish();
    ReplayCase {
        design,
        measurements: case.measurements,
        config: case.config,
        settings: case.settings,
    }
}

// Same recorded modal-room reconstruction as the release engine_perf harness.
fn big_measurements(
    store: &dyn FixtureStore,
    dir: &str,
    name: &str,
    fixture: &Value,
    expanded: bool,
) -> MeasurementSet {
    use crate::C;
    use std::f64::consts::PI;
    let mut out = MeasurementSet {
        positions: Vec::new(),
    };
    let r = &fixture["inputs"]["recipe"];
    let xyz = |v: &serde_json::Value| [number(&v[0]), number(&v[1]), number(&v[2])];
    let dims = xyz(&r["dims"]);
    let q = number(&r["q"]);
    let scale = number(&r["scale"]);
    let excitations: Vec<_> = pairs(&r["excitations"])
        .map(|(_, v)| {
            let data = store.blob(dir, name, v);
            let (chunks, rest) = data.as_chunks::<8>();
            assert!(rest.is_empty());
            let samples: Vec<_> = chunks.iter().map(|b| f64::from_le_bytes(*b)).collect();
            secs_dsp::fft::rfft(&samples)
        })
        .collect();
    let roles: Vec<&str> = if expanded {
        secs_types::role::ROLES.to_vec()
    } else {
        r["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    };
    for (pos, receiver) in r["receivers"].as_array().unwrap().iter().enumerate() {
        let receiver = xyz(receiver);
        let mut measurements = Vec::new();
        for (i, &role) in roles.iter().enumerate() {
            let source = if expanded {
                let (x, y) = secs_types::role::role_position(role).unwrap();
                let z = match secs_types::role::role_kind(role) {
                    secs_types::role::RoleKind::Sub => 0.2,
                    secs_types::role::RoleKind::Top => dims[2] - 0.1,
                    secs_types::role::RoleKind::Height => dims[2] - 0.4,
                    secs_types::role::RoleKind::Main => 1.1,
                };
                [dims[0] * x, dims[1] * y, z]
            } else {
                xyz(&r["sources"][i])
            };
            let distance = source
                .iter()
                .zip(receiver)
                .map(|(s, r)| (s - r).powi(2))
                .sum::<f64>()
                .sqrt();
            let mut transfer = vec![C::new(0.0, 0.0); 24001];
            for mode in r["modes"].as_array().unwrap() {
                let ns = xyz(mode);
                let mut shape = 1.0;
                for k in 0..3 {
                    shape *= (ns[k] * PI * source[k] / dims[k]).cos()
                        * (ns[k] * PI * receiver[k] / dims[k]).cos();
                }
                let weight = ns
                    .iter()
                    .map(|&n| if n == 0.0 { 1.0 } else { 2.0 })
                    .product::<f64>();
                let mw = 2.0 * PI * number(&mode[3]);
                for (k, h) in transfer.iter_mut().enumerate() {
                    let w = 2.0 * PI * k as f64;
                    *h += secs_dsp::complex_div(
                        C::new(weight * shape, 0.0),
                        C::new(mw * mw - w * w, w * mw / q),
                    );
                }
            }
            for (k, h) in transfer.iter_mut().enumerate() {
                let direct = C::from_polar(1.0, -2.0 * PI * k as f64 * distance / 343.0) / distance;
                *h = excitations[i % excitations.len()][k] * (*h * scale + direct);
            }
            let samples: Vec<_> = secs_dsp::fft::irfft(&transfer, 48000)
                .into_iter()
                .map(|v| v as f32)
                .collect();

            measurements.push((
                role.to_owned(),
                Measurement {
                    samples: Samples::F32(samples.into()),
                    sample_rate: 48000,
                    t0: pos as f64 * 0.00013 + i as f64 * 0.00107 - 0.0002,
                },
            ));
        }
        out.positions.push((pos as u32 + 1, measurements));
    }
    out
}

/// Final support metrics over every measured position, using production guards.
#[derive(Debug)]
pub struct MimoAcousticMetrics {
    pub input: String,
    pub late_energy_db: f64,
    pub worst_band_db: f64,
    /// The existing 0.15 s/dB modal-tail guard, not a fitted T60 slope.
    pub modal_tail_guard: f64,
    pub paths: usize,
}

pub fn mimo_acoustic_metrics(
    _store: &dyn FixtureStore,
    case: &ReplayCase,
    run: &crate::mimo::MimoRun,
    ctx: &Context,
) -> Result<Vec<MimoAcousticMetrics>, crate::EngineError> {
    mimo_acoustic_metrics_parts(&case.design, &case.measurements, &case.config, run, ctx)
}

fn mimo_acoustic_metrics_parts(
    design: &crate::Design,
    measurements: &crate::MeasurementSet,
    config: &crate::EngineConfig,
    run: &crate::mimo::MimoRun,
    ctx: &Context,
) -> Result<Vec<MimoAcousticMetrics>, crate::EngineError> {
    use crate::mimo::{self, design as md, iir};
    use secs_types::chain::MimoFilter;
    let prepared = mimo::prepared::Prepared::new(design, measurements, config, ctx)?;
    let req = run.key.request();
    let mut metrics = Vec::new();
    for (input, ideal_supports) in &run.ideal.supports {
        let pb = iir::build_problem(
            design,
            &prepared.spectra,
            &prepared.align_delay_ms,
            &run.ideal,
            input,
            ideal_supports,
            &req,
            &Tuning::default(),
            ctx,
        )?;
        let actual_paths: Vec<_> = run
            .paths
            .paths
            .iter()
            .filter(|p| p.input == *input)
            .collect();
        let (ys, band) = if run.fir {
            let raw: Vec<Vec<Vec<f64>>> = design
                .spectra
                .positions
                .iter()
                .map(|&position| {
                    pb.roles
                        .iter()
                        .map(|role| {
                            let h = mimo::support_path(
                                &run.ideal,
                                role,
                                &prepared.spectra,
                                position,
                                &prepared.align_delay_ms,
                            )?;
                            Ok(secs_dsp::lowrate::to_low(
                                &h,
                                crate::grid::design_freqs(),
                                pb.n,
                            )?)
                        })
                        .collect::<Result<_, crate::EngineError>>()
                })
                .collect::<Result<_, _>>()?;
            let filters: Vec<_> = ideal_supports.iter().map(|(_, s)| s.clone()).collect();
            let ys = pb
                .xs
                .iter()
                .zip(&raw)
                .map(|(x, paths)| {
                    let mut add = vec![0.0; pb.n];
                    for (g, s) in paths.iter().zip(&filters) {
                        for (a, v) in add.iter_mut().zip(md::convolve(g, s)) {
                            *a += v;
                        }
                    }
                    x.iter().zip(add).map(|(x, a)| x + a).collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let band = if filters.is_empty() {
                -99.0
            } else {
                md::worst_band_db(&pb.xs, &raw, &filters, req.fsiso)
            };
            (ys, band)
        } else {
            let supports = actual_paths
                .iter()
                .filter_map(|path| {
                    if let MimoFilter::Iir {
                        gain_db,
                        delay_ms,
                        polarity,
                        peq,
                        allpass,
                    } = &path.filter
                    {
                        Some((
                            pb.roles
                                .iter()
                                .position(|r| *r == path.support)
                                .expect("actual support belongs to ideal problem"),
                            iir::IirSupport {
                                gain_db: *gain_db,
                                delay_ms: *delay_ms,
                                polarity: *polarity,
                                peq: peq.clone(),
                                allpass: allpass.clone(),
                            },
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let band = if supports.is_empty() {
                -99.0
            } else {
                iir::worst_band_db(&pb, &supports)
            };
            (iir::applied(&pb, &supports), band)
        };
        let energy = |signals: &[Vec<f64>]| {
            signals
                .iter()
                .zip(&pb.wl)
                .map(|(x, w)| {
                    crate::decay::energy(&x.iter().zip(w).map(|(x, w)| x * w).collect::<Vec<_>>())
                })
                .sum::<f64>()
        };
        let late_energy_db = 10.0 * ((energy(&ys) + 1e-30) / (energy(&pb.xs) + 1e-30)).log10();
        metrics.push(MimoAcousticMetrics {
            input: input.clone(),
            late_energy_db,
            worst_band_db: band,
            modal_tail_guard: mimo::worst_t60_change(&pb.xs, &ys, 6)?,
            paths: actual_paths.len(),
        });
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_match_the_oracle_case_set() {
        assert_eq!(
            super::names(&crate::tests::Oracle),
            [
                "dolby",
                "dolby_infra",
                "dolby_linear",
                "dolby_ultra",
                "dolby_zero",
                "room",
                "room_dba",
                "room_filters",
                "room_manual",
                "room_multi",
            ]
            .map(str::to_owned)
        );
    }
}

/// Release-only timing hooks. This module exists only with `test-support`.
pub mod perf {
    use secs_compute::{Context, MemoryBudget, NoProgress};
    use secs_dsp::Complex64 as C;
    use secs_types::{calibration::CalibrationSettings, chain::Peq};
    use std::sync::Arc;

    use crate::{
        Design, DesignRequest, EngineConfig, EngineError, FitterFactory, MeasurementSet, Tuning,
        mimo::{MimoRequest, MimoRun},
    };

    pub use crate::chain::{BaRow, term};
    pub use crate::fit::{FitRequest, fit_gd_cuts};
    pub use crate::measurement::{Prepared, prepare};
    pub use crate::setup::Setup;

    pub const N_FFT: usize = crate::grid::N_FFT;

    pub fn design_freqs() -> &'static [f64] {
        crate::grid::design_freqs()
    }

    pub fn production_fitters(ctx: &Context) -> Box<FitterFactory<'_>> {
        crate::design::production_fitters(ctx)
    }

    pub fn design_observed(
        request: &DesignRequest<'_>,
        ctx: &Context,
        fitters: &FitterFactory<'_>,
        observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    ) -> Result<Design, EngineError> {
        crate::design::design_observed(request, ctx, fitters, observed)
    }

    pub fn input_response(
        design: &Design,
        role: &str,
        spectra: &crate::measurement::Spectra,
        position: u32,
    ) -> Option<Vec<C>> {
        crate::chain::input_response(design, role, spectra, position)
    }

    pub fn main_response<'a>(design: &'a Design, role: &str) -> Option<&'a [C]> {
        design
            .mains
            .iter()
            .find(|main| main.role == role)
            .map(|main| main.response.as_ref())
    }

    pub fn main_after<'a>(design: &'a Design, role: &str) -> Option<&'a [C]> {
        design
            .mains
            .iter()
            .find(|main| main.role == role)
            .map(|main| main.after.as_ref())
    }

    pub fn first_sub_profile(design: &Design) -> Option<(&[C], &BaRow)> {
        let subs = design.subs.as_ref()?;
        let (role, row) = subs.rows.first()?;
        let response = design
            .spectra
            .used
            .get(design.spectra.main_position, role)?;
        Some((response.as_ref(), row))
    }

    pub fn score_metrics(h: &[C], f: &[f64], target: &[f64], norm: (f64, f64)) -> crate::Metrics {
        crate::scorecard::metrics(h, f, target, norm)
    }

    pub fn mimo_convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
        crate::mimo::design::convolve(a, b)
    }

    pub fn mimo_normal_equations(
        xs: &[Vec<f64>],
        paths: &[Vec<Vec<f64>>],
        taps: usize,
        lambda: f64,
        early_samples: usize,
    ) -> (Vec<f64>, Vec<f64>) {
        crate::mimo::design::normal_equations(xs, paths, taps, lambda, early_samples)
    }

    pub fn mimo_early_samples(milliseconds: f64) -> usize {
        crate::mimo::design::early_samples(milliseconds)
    }

    pub struct MimoDesignInputResult(crate::mimo::design::InputResult);

    impl MimoDesignInputResult {
        pub fn trajectory_midpoints(&self) -> Vec<f64> {
            self.0
                .trace
                .trajectory
                .iter()
                .map(|step| step.mid)
                .collect()
        }
    }

    impl std::fmt::Debug for MimoDesignInputResult {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("MimoDesignInputResult")
                .field("supports", &self.0.supports.len())
                .field("trajectory", &self.0.trace.trajectory.len())
                .finish_non_exhaustive()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mimo_design_input(
        xs: &[Vec<f64>],
        paths: &[Vec<Vec<f64>>],
        direct: &[f64],
        candidates: &[String],
        request: &MimoRequest,
        ctx: &Context,
        observed: &dyn Fn(&str, std::time::Duration),
    ) -> Result<MimoDesignInputResult, EngineError> {
        crate::mimo::design::design_input_observed(
            xs, paths, direct, candidates, request, ctx, observed,
        )
        .map(MimoDesignInputResult)
    }

    pub fn mimo_solve(
        ata: &[f64],
        atb: &[f64],
        lambda: f64,
        force_lu: bool,
    ) -> Result<Vec<f64>, EngineError> {
        crate::mimo::solve::solve(ata, atb, lambda, force_lu)
    }

    pub struct MimoRidgeSweep<'a>(crate::mimo::solve::RidgeSweep<'a>);

    impl std::fmt::Debug for MimoRidgeSweep<'_> {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("MimoRidgeSweep")
                .finish_non_exhaustive()
        }
    }

    impl<'a> MimoRidgeSweep<'a> {
        pub fn new(ata: &'a [f64], atb: &'a [f64]) -> Self {
            Self(crate::mimo::solve::RidgeSweep::new(ata, atb))
        }

        pub fn solve(&self, lambda: f64) -> Result<Vec<f64>, EngineError> {
            self.0.solve(lambda)
        }
    }

    pub struct MimoInputsResult(
        crate::mimo::MimoDesign,
        Vec<(String, crate::mimo::design::InputTrace)>,
    );

    impl std::fmt::Debug for MimoInputsResult {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("MimoInputsResult")
                .field("supports", &self.0.supports.len())
                .field("inputs", &self.1.len())
                .finish_non_exhaustive()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mimo_design_inputs_observed(
        design: &Design,
        measurements: &MeasurementSet,
        config: &EngineConfig,
        request: &MimoRequest,
        ctx: &Context,
        input: Option<&str>,
        observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    ) -> Result<MimoInputsResult, EngineError> {
        crate::mimo::design::design_mimo_inputs_observed(
            design,
            measurements,
            config,
            request,
            ctx,
            input,
            observed,
        )
        .map(|(design, inputs)| MimoInputsResult(design, inputs))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mimo_compute_observed(
        design: &Design,
        measurements: &MeasurementSet,
        config: &EngineConfig,
        settings: &CalibrationSettings,
        tuning: &Tuning,
        ctx: &Context,
        observed: &(dyn Fn(&str, std::time::Duration) + Sync),
    ) -> Result<MimoRun, EngineError> {
        crate::mimo::run::compute_observed(
            design,
            measurements,
            config,
            settings,
            tuning,
            ctx,
            observed,
        )
    }

    #[derive(Debug)]
    pub struct FitSeed {
        pub bands: Vec<Peq>,
        pub x0: Vec<f64>,
        pub lb: Vec<f64>,
        pub ub: Vec<f64>,
    }

    pub fn fit_seed(request: &FitRequest<'_>) -> FitSeed {
        let seed = crate::fit::seed(request);
        FitSeed {
            bands: seed.bands,
            x0: seed.x0,
            lb: seed.lb,
            ub: seed.ub,
        }
    }

    pub struct FitResidualCache(crate::fit::ResidualCache);

    impl std::fmt::Debug for FitResidualCache {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("FitResidualCache")
                .finish_non_exhaustive()
        }
    }

    impl FitResidualCache {
        pub fn new(grid: &[f64], bands: usize) -> Self {
            Self(crate::fit::ResidualCache::new(grid, bands))
        }

        pub fn evaluate(&mut self, request: &FitRequest<'_>, x: &[f64]) -> Vec<f64> {
            self.0.evaluate(request, x)
        }
    }

    pub fn fit_result_bands(request: &FitRequest<'_>, x: &[f64]) -> Vec<Peq> {
        crate::fit::result_bands(request, x)
    }

    pub fn unbounded_context(threads: usize) -> Context {
        Context::new(
            threads,
            MemoryBudget::new(u64::MAX),
            Arc::new(NoProgress),
            secs_compute::CancelToken::new(),
        )
    }
}

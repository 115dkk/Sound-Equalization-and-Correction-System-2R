//! Oracle fixture support for dependent crate parity tests.

use std::collections::BTreeMap;

use secs_compute::Context;
use secs_oracle::{Fixture, PLATFORM_RTOL, errors, integer, number, oracle_libm};
use secs_types::target::TargetCurve;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use crate::{
    Design, DesignRequest, EngineConfig, FitStage, FitterFactory, Measurement, MeasurementSet,
    Samples, Tuning, design,
    fit::{RecordedFit, replay_fitter},
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

pub fn names() -> Vec<String> {
    Fixture::all("engine_inputs", "")
        .iter()
        .map(|f| f.id.strip_prefix("engine_inputs/").unwrap().to_owned())
        .collect()
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

pub fn load(name: &str) -> Case {
    let large = name == "big_fir";
    let fixture = Fixture::load(
        if name == "big" || large {
            "perf_mimo"
        } else {
            "engine_inputs"
        },
        if large { "big" } else { name },
    );
    let mut case = load_fixture(&fixture);
    if name == "big" || large {
        case.measurements = big_measurements(&fixture, large);
    }
    if large {
        expand_config(&mut case.config);
    }
    case
}

pub fn load_fixture(fx: &Fixture) -> Case {
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
        rc_bands: Vec::new(),
        room_volume_m3: optional(&v["room_volume_m3"]),
        rc_hi_basis: secs_types::calibration::RcHiBasis::Strict,
        rc_hi_transition: 1,
        low_protect: false,
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

pub(crate) fn read_recorded_fit(fixture: &Fixture, input: &Value, result: &Value) -> RecordedFit {
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
    let Case {
        measurements,
        config,
        tuning,
        settings,
        ..
    } = load(name);
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

fn verify_request_hash(fixture: &Fixture, fit: &RecordedFit, hashes: &Value) {
    for (key, values) in [("g", &fit.g), ("corr_db", &fit.corr_db)] {
        fixture.assert_digest(
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
}

impl Replay {
    pub fn for_case(name: &str) -> Self {
        let mut stages: BTreeMap<String, Vec<RecordedFit>> = BTreeMap::new();
        for call in Fixture::all_calls("engine_design", &format!("{name}_fits_")) {
            let recorded = if let Some(reference) = call.inputs().get("reference") {
                let source = Fixture::load(
                    reference["family"].as_str().unwrap(),
                    reference["fixture"].as_str().unwrap(),
                );
                let fit = if let Some(index) = reference["call"].as_u64() {
                    let calls = source.calls();
                    let c = &calls[index as usize];
                    read_recorded_fit(c, c.inputs(), &c.outputs()["result"])
                } else {
                    let fit = &source.outputs()["candidates"]
                        [reference["candidate"].as_u64().unwrap() as usize]["fits"]
                        [reference["fit"].as_u64().unwrap() as usize];
                    read_recorded_fit(&source, &fit["inputs"], &fit["result"])
                };
                verify_request_hash(&source, &fit, &reference["request_sha256"]);
                fit
            } else {
                let fit = read_recorded_fit(&call, call.inputs(), &call.outputs()["result"]);
                verify_request_hash(&call, &fit, &call.inputs()["request_sha256"]);
                fit
            };
            stages
                .entry(call.inputs()["stage"].as_str().unwrap().into())
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
        }
    }

    pub fn factory(&self) -> Box<FitterFactory<'_>> {
        let seen = self.counts.clone();
        let stages = &self.stages;
        let name = self.name.as_str();
        Box::new(move |stage| {
            let key = match stage {
                FitStage::Ba => "Ba".into(),
                FitStage::Top(r) => format!("Top:{r}"),
                FitStage::Rc(r) => format!("Rc:{r}"),
            };
            let calls = stages.get(&key).cloned().unwrap_or_default();
            let expected = calls.len();
            let report_calls = (name == "room" || !oracle_libm()).then(|| calls.clone());
            let mut next = 0;
            let mut fit = replay_fitter(
                calls,
                if name == "room" || !oracle_libm() {
                    f64::INFINITY
                } else {
                    PLATFORM_RTOL
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
pub(crate) fn replay(name: &str) -> Design {
    replay_case(name).design
}

#[cfg(test)]
pub(crate) fn replay_with_context(name: &str, context: &Context) -> Design {
    replay_case_with_context(name, context).design
}

/// Replays the recorded fitter decisions and checks requests and call counts.
pub fn replay_case(name: &str) -> ReplayCase {
    replay_case_with_context(name, &Context::serial())
}

fn replay_case_with_context(name: &str, context: &Context) -> ReplayCase {
    let case = load(name);
    let replay = Replay::for_case(name);
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
fn big_measurements(fx: &Fixture, expanded: bool) -> MeasurementSet {
    use crate::C;
    use std::f64::consts::PI;
    let mut out = MeasurementSet {
        positions: Vec::new(),
    };
    let r = &fx.inputs()["recipe"];
    let xyz = |v: &serde_json::Value| [number(&v[0]), number(&v[1]), number(&v[2])];
    let dims = xyz(&r["dims"]);
    let q = number(&r["q"]);
    let scale = number(&r["scale"]);
    let excitations: Vec<_> = pairs(&r["excitations"])
        .map(|(_, v)| {
            let data = fx.blob(v);
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
    case: &ReplayCase,
    run: &crate::mimo::MimoRun,
    ctx: &Context,
) -> Result<Vec<MimoAcousticMetrics>, crate::EngineError> {
    use crate::mimo::{self, design as md, iir};
    use secs_types::chain::MimoFilter;
    let prepared =
        mimo::prepared::Prepared::new(&case.design, &case.measurements, &case.config, ctx)?;
    let req = run.key.request();
    let mut metrics = Vec::new();
    for (input, ideal_supports) in &run.ideal.supports {
        let pb = iir::build_problem(
            &case.design,
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
            let raw: Vec<Vec<Vec<f64>>> = case
                .design
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

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
    let large = name == "big_fir";
    let fixture = Fixture::load(
        if name == "big" || large {
            "perf_mimo"
        } else {
            "engine_inputs"
        },
        if large { "big" } else { name },
    );
    let input = fixture.inputs();
    let value = &input["config"];
    let mut config = EngineConfig {
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
    let mut positions = input["measurements"]
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
    if name == "big" || large {
        positions = big_measurements(&fixture, large).positions;
    }
    if large {
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
        // Off the oracle libm BA's differential evolution can diverge (macOS:
        // room_filters), so the recorded requests no longer match; inject the
        // recorded results without checking them, as the engine's own replayed
        // design test does, and let the caller report what follows BA.
        let rtol = if secs_oracle::oracle_libm() {
            PLATFORM_RTOL
        } else {
            f64::INFINITY
        };
        let mut fitter = replay_fitter(stages.get(&key).cloned().unwrap_or_default(), rtol);
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
    let prepared = mimo::prepared::Prepared::new(&case.design, &case.measurements, &case.config)?;
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

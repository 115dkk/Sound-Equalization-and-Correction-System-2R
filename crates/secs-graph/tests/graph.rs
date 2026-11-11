#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The export graph's one validation, at its interface.

use secs_graph::{ExportGraph, GraphError, LfeChoice, Op, is_negative_delay};
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};

#[test]
fn inverted_main_ends_its_output_path_with_a_fixed_invert() {
    let mut chain = chain();
    chain.mains[1].polarity = Some(-1);
    let graph = ExportGraph::build(&chain, Some(&mimo()), LfeChoice::Omit).unwrap();
    let main = graph
        .plan()
        .main_ops
        .iter()
        .find(|(role, _)| role == "R")
        .unwrap();
    assert_eq!(main.1.last(), Some(&Op::Invert { fixed: true }));
    let support = graph
        .plan()
        .mimo
        .iter()
        .find(|path| path.support == "R")
        .unwrap();
    assert_eq!(support.ops.last(), Some(&Op::Invert { fixed: true }));
}

fn chain() -> SignalChain {
    let main = |role: &str| MainChannel {
        role: role.into(),
        input_peq: vec![(70.0, 1.2, -1.5)],
        fir: vec![0.8, 0.13, -0.02],
        gain_db: -0.5,
        polarity: None,
        delay_ms: 3.0,
        xo_hz: Some(80.0),
        top_peq: vec![(130.0, 0.9, 0.7)],
        align_delay_ms: 1.0,
    };
    SignalChain {
        input_hp_hz: None,
        sample_rate: 48_000,
        mains: vec![main("L"), main("R")],
        subs: vec![SubChannel {
            role: "SW1".into(),
            gain_db: -3.0,
            delay_ms: 5.0,
            polarity: 1,
            allpass: vec![(70.0, 0.98)],
            hp_hz: 22.0,
            hp_type: BandType::Bw12,
            lp_hz: 230.0,
            lp_type: BandType::Lr24,
            peq: vec![(38.0, 1.3, -1.8)],
            mode: "auto".into(),
        }],
        sub_bus_peq: vec![(45.0, 1.3, -1.0)],
        main_extra_ms: 2.0,
        lift_ms: 0.0,
        latency_ms: 2.0,
    }
}

#[test]
fn reported_fir_files_count_each_materialized_file() {
    let graph = ExportGraph::build(&chain(), None, LfeChoice::Omit).unwrap();
    let expected = secs_graph::emit::fir_files(graph.plan(), "test_", 48_000).unwrap();
    let reports = std::cell::RefCell::new(Vec::new());

    let actual =
        secs_graph::emit::fir_files_reported(graph.plan(), "test_", 48_000, &|done, total| {
            reports.borrow_mut().push((done, total))
        })
        .unwrap();

    assert_eq!(actual, expected);
    assert_eq!(reports.into_inner(), [(0, 2), (1, 2), (2, 2)]);
}

#[test]
fn reported_fir_files_skip_progress_when_the_plan_has_no_firs() {
    let mut without_fir = chain();
    for main in &mut without_fir.mains {
        main.fir = vec![1.0];
    }
    let graph = ExportGraph::build(&without_fir, None, LfeChoice::Omit).unwrap();
    let reports = std::cell::RefCell::new(Vec::new());

    let files =
        secs_graph::emit::fir_files_reported(graph.plan(), "test_", 48_000, &|done, total| {
            reports.borrow_mut().push((done, total))
        })
        .unwrap();

    assert!(files.is_empty());
    assert!(reports.into_inner().is_empty());
}

/// Path 0 is IIR, path 1 a 48 kHz FIR, path 2 a low-rate FIR.
fn mimo() -> MimoPaths {
    MimoPaths {
        fsiso: 150.0,
        paths: vec![
            MimoPath {
                input: "L".into(),
                support: "SW1".into(),
                filter: MimoFilter::Iir {
                    gain_db: -9.0,
                    delay_ms: 0.5,
                    polarity: -1,
                    peq: vec![(111.0, 0.98, -0.45)],
                    allpass: vec![(125.0, 1.7)],
                },
                low_hz: 30.0,
                delay_ms: 5.0,
            },
            MimoPath {
                input: "R".into(),
                support: "SW1".into(),
                filter: MimoFilter::Fir {
                    taps_48k: vec![0.1, -0.02, 0.001],
                },
                low_hz: 30.0,
                delay_ms: 5.0,
            },
            MimoPath {
                input: "L".into(),
                support: "R".into(),
                filter: MimoFilter::FirLowRate {
                    taps_low: vec![0.3, -0.1, 0.02],
                    fs_low: 600.0,
                },
                low_hz: 50.0,
                delay_ms: 3.0,
            },
        ],
    }
}

fn iir(mimo: &mut MimoPaths) -> (&mut f64, &mut i8) {
    let MimoFilter::Iir {
        delay_ms, polarity, ..
    } = &mut mimo.paths[0].filter
    else {
        unreachable!()
    };
    (delay_ms, polarity)
}

fn lowrate(mimo: &mut MimoPaths) -> (&mut Vec<f64>, &mut f64) {
    let MimoFilter::FirLowRate { taps_low, fs_low } = &mut mimo.paths[2].filter else {
        unreachable!()
    };
    (taps_low, fs_low)
}

type Edit = fn(&mut SignalChain, &mut MimoPaths, &mut LfeChoice);
type Kind = fn(&GraphError) -> bool;

const NON_ASCII: Kind = |error| matches!(error, GraphError::NonAscii(_));
const NON_FINITE: Kind = |error| matches!(error, GraphError::NonFinite(_));
const INVALID: Kind = |error| matches!(error, GraphError::Invalid(_));

#[test]
fn every_rule_rejects_its_input_and_names_the_field() {
    // Negative is negative after rounding to whole 48 kHz samples: float
    // noise and exactly half a sample round to 0 and pass (TD-94).
    assert!(!is_negative_delay(-0.000_010_416_666_666_666_666));
    assert!(!is_negative_delay(-0.5 / 48.0));
    assert!(is_negative_delay(-0.51 / 48.0));
    assert!(is_negative_delay(-1.0));
    ExportGraph::build(&chain(), Some(&mimo()), LfeChoice::DOLBY).unwrap();
    let mut disabled_sub_edges = chain();
    disabled_sub_edges.subs[0].hp_hz = 0.0;
    disabled_sub_edges.subs[0].lp_hz = 0.0;
    ExportGraph::build(&disabled_sub_edges, None, LfeChoice::Omit).unwrap();
    let mut disabled_mimo_edge = mimo();
    disabled_mimo_edge.paths[0].low_hz = 0.0;
    ExportGraph::build(&chain(), Some(&disabled_mimo_edge), LfeChoice::Omit).unwrap();

    let rows: &[(&str, Edit, Kind, &str)] = &[
        // 48 kHz.
        (
            "rate",
            |c, _, _| c.sample_rate = 44_100,
            INVALID,
            "chain.sample_rate",
        ),
        // Every number finite.
        (
            "chain number",
            |c, _, _| c.main_extra_ms = f64::NAN,
            NON_FINITE,
            "chain.main_extra_ms",
        ),
        (
            "main PEQ",
            |c, _, _| c.mains[0].input_peq[0].1 = f64::NAN,
            NON_FINITE,
            "main.L.input_peq[0].q",
        ),
        (
            "main gain",
            |c, _, _| c.mains[0].gain_db = f64::NAN,
            NON_FINITE,
            "main.L.gain_db",
        ),
        (
            "main FIR",
            |c, _, _| c.mains[1].fir[1] = f64::INFINITY,
            NON_FINITE,
            "main.R.fir[1]",
        ),
        (
            "sub all-pass",
            |c, _, _| c.subs[0].allpass[0].0 = f64::NAN,
            NON_FINITE,
            "sub.SW1.allpass[0].frequency",
        ),
        (
            "bus PEQ",
            |c, _, _| c.sub_bus_peq[0].2 = f64::NAN,
            NON_FINITE,
            "sub_bus_peq[0].gain",
        ),
        (
            "Fsiso",
            |_, m, _| m.fsiso = f64::NAN,
            NON_FINITE,
            "mimo.fsiso",
        ),
        (
            "MIMO PEQ",
            |_, m, _| {
                let MimoFilter::Iir { peq, .. } = &mut m.paths[0].filter else {
                    unreachable!()
                };
                peq[0].0 = f64::NAN;
            },
            NON_FINITE,
            "mimo.paths[0].peq[0].frequency",
        ),
        (
            "MIMO FIR",
            |_, m, _| {
                m.paths[1].filter = MimoFilter::Fir {
                    taps_48k: vec![f64::NAN],
                }
            },
            NON_FINITE,
            "mimo.paths[1].taps_48k[0]",
        ),
        (
            "MIMO low-rate tap",
            |_, m, _| lowrate(m).0[1] = f64::NAN,
            NON_FINITE,
            "mimo.paths[2].taps_low[1]",
        ),
        (
            "LFE gain",
            |_, _, l| {
                *l = LfeChoice::Include {
                    gain_db: f64::NAN,
                    lp_hz: 120.0,
                }
            },
            NON_FINITE,
            "lfe.gain_db",
        ),
        (
            "LFE low-pass",
            |_, _, l| {
                *l = LfeChoice::Include {
                    gain_db: 10.0,
                    lp_hz: f64::INFINITY,
                }
            },
            NON_FINITE,
            "lfe.lp_hz",
        ),
        // Active frequencies are strictly inside the design band and Q is positive.
        (
            "PEQ at zero hertz",
            |c, _, _| c.mains[0].top_peq[0].0 = 0.0,
            INVALID,
            "main.L.top_peq[0].frequency",
        ),
        (
            "PEQ at Nyquist",
            |c, _, _| c.mains[0].top_peq[0].0 = 24_000.0,
            INVALID,
            "main.L.top_peq[0].frequency",
        ),
        (
            "PEQ with zero Q",
            |c, _, _| c.mains[0].top_peq[0].1 = 0.0,
            INVALID,
            "main.L.top_peq[0].q",
        ),
        (
            "all-pass with negative Q",
            |c, _, _| c.subs[0].allpass[0].1 = -1.0,
            INVALID,
            "sub.SW1.allpass[0].q",
        ),
        (
            "negative sub high-pass",
            |c, _, _| c.subs[0].hp_hz = -20.0,
            INVALID,
            "sub.SW1.hp_hz",
        ),
        (
            "sub low-pass above Nyquist",
            |c, _, _| c.subs[0].lp_hz = 30_000.0,
            INVALID,
            "sub.SW1.lp_hz",
        ),
        (
            "main crossover at zero hertz",
            |c, _, _| c.mains[0].xo_hz = Some(0.0),
            INVALID,
            "main.L.xo_hz",
        ),
        (
            "MIMO low edge above Nyquist",
            |_, m, _| m.paths[0].low_hz = 48_000.0,
            INVALID,
            "mimo.paths[0].low_hz",
        ),
        // Identifiers ASCII and safe.
        (
            "non-ASCII role",
            |c, _, _| c.mains[0].role = "왼쪽".into(),
            NON_ASCII,
            "main[0].role",
        ),
        (
            "non-ASCII sub mode",
            |c, _, _| c.subs[0].mode = "자동".into(),
            NON_ASCII,
            "sub.SW1.mode",
        ),
        (
            "path role",
            |c, _, _| c.mains[0].role = "../L".into(),
            INVALID,
            "main[0].role",
        ),
        (
            "punctuated role",
            |c, _, _| c.subs[0].role = "SW.1".into(),
            INVALID,
            "sub[0].role",
        ),
        (
            "empty role",
            |c, _, _| c.mains[1].role = String::new(),
            INVALID,
            "main[1].role",
        ),
        (
            "unsafe MIMO support",
            |_, m, _| m.paths[1].support = "S W".into(),
            INVALID,
            "mimo.paths[1].support",
        ),
        // No duplicate or reserved roles (TD-46).
        (
            "duplicate main",
            |c, _, _| c.mains[1].role = "L".into(),
            INVALID,
            "duplicate or reserved role \"L\" at main[1].role",
        ),
        (
            "reserved main",
            |c, _, _| c.mains[0].role = "LFE".into(),
            INVALID,
            "duplicate or reserved role \"LFE\" at main[0].role",
        ),
        (
            "sub named as a main",
            |c, _, _| c.subs[0].role = "R".into(),
            INVALID,
            "duplicate or reserved role \"R\" at sub[0].role",
        ),
        (
            "duplicate sub",
            |c, _, _| c.subs.push(c.subs[0].clone()),
            INVALID,
            "duplicate or reserved role \"SW1\" at sub[1].role",
        ),
        (
            "reserved sub",
            |c, _, _| c.subs[0].role = "LFE".into(),
            INVALID,
            "duplicate or reserved role \"LFE\" at sub[0].role",
        ),
        // A MIMO input is a main and a support an output.
        (
            "MIMO input",
            |_, m, _| m.paths[0].input = "SW1".into(),
            INVALID,
            "mimo.paths[0].input",
        ),
        (
            "MIMO support",
            |_, m, _| m.paths[1].support = "SW9".into(),
            INVALID,
            "mimo.paths[1].support",
        ),
        // Polarity 1 or -1.
        (
            "sub polarity",
            |c, _, _| c.subs[0].polarity = 0,
            INVALID,
            "sub.SW1.polarity",
        ),
        (
            "MIMO polarity",
            |_, m, _| *iir(m).1 = 2,
            INVALID,
            "mimo.paths[0].polarity",
        ),
        // Delays not negative (TD-94).
        (
            "main delay",
            |c, _, _| c.mains[0].delay_ms = -1.0,
            INVALID,
            "main.L.delay_ms",
        ),
        (
            "main delay past half a sample",
            |c, _, _| c.mains[1].delay_ms = -0.51 / 48.0,
            INVALID,
            "main.R.delay_ms",
        ),
        (
            "sub delay",
            |c, _, _| c.subs[0].delay_ms = -1.0,
            INVALID,
            "sub.SW1.delay_ms",
        ),
        (
            "MIMO delay",
            |_, m, _| m.paths[1].delay_ms = -1.0,
            INVALID,
            "mimo.paths[1].delay_ms",
        ),
        (
            "MIMO filter delay",
            |_, m, _| *iir(m).0 = -1.0,
            INVALID,
            "mimo.paths[0].filter_delay_ms",
        ),
        (
            "non-finite delay",
            |c, _, _| c.subs[0].delay_ms = f64::NAN,
            NON_FINITE,
            "sub.SW1.delay_ms",
        ),
        // The low-rate FIR rule: at most 48 kHz, and its expansion fits.
        (
            "low rate above 48 kHz",
            |_, m, _| *lowrate(m).1 = 96_000.0,
            INVALID,
            "mimo.paths[2].taps_low",
        ),
        (
            "oversized expansion",
            |_, m, _| *lowrate(m).1 = 0.001,
            INVALID,
            "mimo.paths[2].taps_low",
        ),
        (
            "no low-rate taps",
            |_, m, _| lowrate(m).0.clear(),
            INVALID,
            "mimo.paths[2].taps_low",
        ),
        (
            "non-finite low rate",
            |_, m, _| *lowrate(m).1 = f64::NAN,
            NON_FINITE,
            "mimo.paths[2].fs_low",
        ),
    ];
    for (rule, edit, kind, field) in rows {
        let (mut chain, mut mimo, mut lfe) = (chain(), mimo(), LfeChoice::DOLBY);
        edit(&mut chain, &mut mimo, &mut lfe);
        let error = ExportGraph::build(&chain, Some(&mimo), lfe).unwrap_err();
        assert!(kind(&error), "{rule}: {error:?}");
        assert!(error.to_string().contains(field), "{rule}: {error}");
    }
}

/// CamillaDSP (and upstream's writer) takes `_` and `-` in role names; the
/// text and Equalizer APO writers check their stricter letters-and-digits
/// rule themselves (secs-filters).
#[test]
fn roles_with_underscores_and_hyphens_build() {
    let mut chain = chain();
    chain.mains[0].role = "L_top".into();
    chain.subs[0].role = "SW-1".into();
    let graph = ExportGraph::build(&chain, None, LfeChoice::Omit).unwrap();
    assert!(graph.plan().outputs.iter().any(|role| role == "SW-1"));
}

#[test]
fn fir_filename_collisions_are_rejected_td266() {
    let mut collision_chain = chain();
    collision_chain.mains[0].role = "a_b".into();
    collision_chain.mains[1].role = "a".into();
    let mut c = collision_chain.subs[0].clone();
    c.role = "c".into();
    let mut b_c = c.clone();
    b_c.role = "b_c".into();
    collision_chain.subs = vec![c, b_c];
    let collision_paths = MimoPaths {
        fsiso: 150.0,
        paths: vec![
            MimoPath {
                input: "a_b".into(),
                support: "c".into(),
                filter: MimoFilter::Fir {
                    taps_48k: vec![0.25, -0.125],
                },
                low_hz: 30.0,
                delay_ms: 0.0,
            },
            MimoPath {
                input: "a".into(),
                support: "b_c".into(),
                filter: MimoFilter::Fir {
                    taps_48k: vec![0.5, -0.25],
                },
                low_hz: 30.0,
                delay_ms: 0.0,
            },
        ],
    };
    let error =
        ExportGraph::build(&collision_chain, Some(&collision_paths), LfeChoice::Omit).unwrap_err();
    assert!(matches!(
        error,
        GraphError::Invalid(message) if message.contains("mimo_a_b_c")
    ));

    ExportGraph::build(&chain(), None, LfeChoice::Omit).unwrap();
}

#[test]
fn input_hp_plan_cuts_every_input_and_mimo_once() {
    let mut chain = chain();
    let mut second = chain.subs[0].clone();
    second.role = "SW2".into();
    second.polarity = -1;
    chain.subs.push(second);
    let mimo = mimo();
    let off = ExportGraph::build(&chain, Some(&mimo), LfeChoice::DOLBY).unwrap();
    assert!(off.plan().pre_ops.is_empty());
    chain.input_hp_hz = Some(27.0);
    let on = ExportGraph::build(&chain, Some(&mimo), LfeChoice::DOLBY).unwrap();
    assert_eq!(
        on.plan().pre_ops,
        [secs_graph::Op::Hp {
            frequency: 27.0,
            kind: BandType::Bw48,
            fixed: false,
        }]
    );
    let f: Vec<_> = (0..=65536).map(|k| k as f64 * 48000.0 / 131072.0).collect();
    let hp = secs_dsp::response::band_filter(&f, 27.0, "HP", "BW48").unwrap();
    for skip_fixed in [false, true] {
        for input in &on.plan().inputs {
            let a = secs_graph::plan_responses(off.plan(), &f, input, skip_fixed).unwrap();
            let b = secs_graph::plan_responses(on.plan(), &f, input, skip_fixed).unwrap();
            assert_eq!(a.len(), b.len());
            for ((role, a), (other, b)) in a.iter().zip(&b) {
                assert_eq!(role, other);
                let (mut db, mut deg) = (0.0_f64, 0.0_f64);
                for (i, &f) in f
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| (10.0..=20000.0).contains(*f))
                {
                    let ratio = b[i] / (a[i] * hp[i]);
                    assert!(
                        ratio.re.is_finite() && ratio.im.is_finite(),
                        "{input}/{role}/{f}"
                    );
                    db = db.max((20.0 * ratio.norm().log10()).abs());
                    deg = deg.max(ratio.arg().to_degrees().abs());
                }
                eprintln!(
                    "input_hp plan {input}->{role} skip_fixed={skip_fixed}: max_db={db:.12e} max_deg={deg:.12e}"
                );
                assert!(db < 0.01 && deg < 0.1);
            }
        }
    }
    assert!(
        secs_graph::headroom_db(on.plan()).unwrap() <= secs_graph::headroom_db(off.plan()).unwrap()
    );
    chain.input_hp_hz = Some(f64::NAN);
    assert!(
        ExportGraph::build(&chain, None, LfeChoice::Omit)
            .unwrap_err()
            .to_string()
            .contains("input_hp_hz")
    );
}

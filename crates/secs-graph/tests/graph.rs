#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The export graph's one validation, at its interface.

use secs_graph::{ExportGraph, GraphError, LfeChoice, is_negative_delay};
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};

fn chain() -> SignalChain {
    let main = |role: &str| MainChannel {
        role: role.into(),
        input_peq: vec![(70.0, 1.2, -1.5)],
        fir: vec![0.8, 0.13, -0.02],
        gain_db: -0.5,
        delay_ms: 3.0,
        xo_hz: Some(80.0),
        top_peq: vec![(130.0, 0.9, 0.7)],
        align_delay_ms: 1.0,
    };
    SignalChain {
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

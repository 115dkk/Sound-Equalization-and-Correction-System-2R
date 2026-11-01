//! The upstream `make_plan` topology and its as-written transfer functions.
//!
//! Upstream source: `export_filters.py` (`make_plan`, `plan_responses`,
//! `headroom_db`), moved here from `secs-filters` so every writer reads one
//! plan (#92 C11).

use crate::{GraphError, LfeChoice, SAMPLE_RATE};
use secs_dsp::Complex64;
use secs_dsp::fft;
use secs_dsp::response::{allpass2, band_filter, peaking_response};
use secs_types::chain::{BandType, MimoFilter, MimoPaths, SignalChain};
use std::borrow::Cow;
use std::f64::consts::PI;

/// One filter-plan operation. `fixed` marks a fixed subwoofer filter.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// RBJ peaking EQ.
    Peq {
        /// Center frequency in hertz.
        frequency: f64,
        /// Quality factor.
        q: f64,
        /// Gain in dB.
        gain_db: f64,
        /// Whether the band is a fixed subwoofer filter.
        fixed: bool,
    },
    /// 48 kHz FIR and its plan-local stem.
    Fir {
        /// FIR stem before the writer-specific prefix.
        name: String,
        /// IEEE-float samples before WAV serialization.
        taps: Vec<f64>,
        /// Fixed-filter marker (normally false for FIRs).
        fixed: bool,
    },
    /// Compact design-rate FIR, expanded only for a consumer that needs 48 kHz taps.
    FirLowRate {
        /// FIR stem before the writer-specific prefix.
        name: String,
        /// Design-rate coefficients including the isolation low-pass.
        taps_low: Vec<f64>,
        /// Design sample rate in hertz.
        fs_low: f64,
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Gain in dB and its fixed marker.
    Gain {
        /// Gain in dB.
        db: f64,
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Polarity inversion and its fixed marker.
    Invert {
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Analog-prototype high-pass.
    Hp {
        /// Cutoff frequency in hertz.
        frequency: f64,
        /// Filter family and order.
        kind: BandType,
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Analog-prototype low-pass.
    Lp {
        /// Cutoff frequency in hertz.
        frequency: f64,
        /// Filter family and order.
        kind: BandType,
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Second-order analog all-pass.
    Ap {
        /// Center frequency in hertz.
        frequency: f64,
        /// Quality factor.
        q: f64,
        /// Fixed-filter marker.
        fixed: bool,
    },
    /// Delay in milliseconds.
    Delay {
        /// Delay in milliseconds.
        ms: f64,
        /// Fixed-filter marker.
        fixed: bool,
    },
}

impl Op {
    /// Whether the operation is a fixed subwoofer filter.
    pub fn is_fixed(&self) -> bool {
        match self {
            Self::Peq { fixed, .. }
            | Self::Fir { fixed, .. }
            | Self::FirLowRate { fixed, .. }
            | Self::Gain { fixed, .. }
            | Self::Invert { fixed }
            | Self::Hp { fixed, .. }
            | Self::Lp { fixed, .. }
            | Self::Ap { fixed, .. }
            | Self::Delay { fixed, .. } => *fixed,
        }
    }
}

/// One subwoofer bus output, preserving upstream insertion order.
#[derive(Debug, Clone, PartialEq)]
pub struct SubFeed {
    /// Main sends and optional LFE input summed into this bus.
    pub sources: Vec<String>,
    /// Per-sub bus gain.
    pub gain_db: f64,
    /// `1` or `-1`.
    pub polarity: i8,
    /// Bus, fixed, all-pass, and delay operations.
    pub ops: Vec<Op>,
}

/// One MIMO support feed.
#[derive(Debug, Clone, PartialEq)]
pub struct MimoFeed {
    /// Raw input role.
    pub input: String,
    /// Support output role.
    pub support: String,
    /// Path operations.
    pub ops: Vec<Op>,
}

/// The complete export plan, with every map represented as an ordered vector.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterPlan {
    /// Main roles followed by optional LFE.
    pub inputs: Vec<String>,
    /// Main roles followed by sub roles.
    pub outputs: Vec<String>,
    /// Common operations before input correction and the MIMO fork, including LFE.
    pub pre_ops: Vec<Op>,
    /// Input-correction operations by role.
    pub input_ops: Vec<(String, Vec<Op>)>,
    /// Main-path operations by role.
    pub main_ops: Vec<(String, Vec<Op>)>,
    /// Crossover sends by role.
    pub send_ops: Vec<(String, Vec<Op>)>,
    /// LFE input operations.
    pub lfe_ops: Vec<Op>,
    /// Sub feeds by role.
    pub subs: Vec<(String, SubFeed)>,
    /// MIMO paths.
    pub mimo: Vec<MimoFeed>,
}

/// The low-rate FIR rule, written once: a design rate of at most 48 kHz
/// whose 48 kHz expansion has a length `secs_dsp` accepts.
pub(crate) fn lowrate_fits(taps: usize, fs_low: f64) -> bool {
    fs_low <= f64::from(SAMPLE_RATE) && secs_dsp::fir::lowrate_len_48k(taps, fs_low).is_some()
}

/// One FIR of a plan, borrowed: collecting export files never clones stored
/// FIR arrays.
#[derive(Debug, Clone, Copy)]
pub enum FirView<'a> {
    /// 48 kHz taps.
    Full(&'a [f64]),
    /// Design-rate taps and their rate in hertz.
    LowRate(&'a [f64], f64),
}

impl<'a> FirView<'a> {
    /// Check a low-rate recipe against the low-rate FIR rule and its taps for
    /// finite values. A plan from [`ExportGraph`](crate::ExportGraph) always
    /// passes; a hand-built or revived plan may not.
    pub fn validate(self) -> Result<(), GraphError> {
        if let Self::LowRate(taps, fs_low) = self
            && (!lowrate_fits(taps.len(), fs_low) || taps.iter().any(|tap| !tap.is_finite()))
        {
            return Err(GraphError::Invalid(
                "invalid or oversized low-rate FIR recipe".into(),
            ));
        }
        Ok(())
    }

    /// Number of taps at 48 kHz.
    pub fn len_48k(self) -> usize {
        match self {
            Self::Full(taps) => taps.len(),
            Self::LowRate(taps, fs_low) => {
                (taps.len() as f64 * (f64::from(SAMPLE_RATE) / fs_low)).ceil() as usize
            }
        }
    }

    /// The 48 kHz taps, expanding a low-rate recipe.
    pub fn materialize(self) -> Cow<'a, [f64]> {
        match self {
            Self::Full(taps) => Cow::Borrowed(taps),
            Self::LowRate(taps, fs_low) => Cow::Owned(secs_dsp::fir::lowrate_to_48k(taps, fs_low)),
        }
    }
}

impl FilterPlan {
    /// The plan's FIRs by stem, input corrections first, then MIMO paths; a
    /// repeated stem keeps its first position and its last taps.
    pub fn firs(&self) -> Vec<(&str, FirView<'_>)> {
        let mut out: Vec<(&str, FirView<'_>)> = Vec::new();
        for ops in self
            .input_ops
            .iter()
            .map(|(_, ops)| ops)
            .chain(self.mimo.iter().map(|m| &m.ops))
        {
            for op in ops {
                let (name, view) = match op {
                    Op::Fir { name, taps, .. } => (name.as_str(), FirView::Full(taps)),
                    Op::FirLowRate {
                        name,
                        taps_low,
                        fs_low,
                        ..
                    } => (name.as_str(), FirView::LowRate(taps_low, *fs_low)),
                    _ => continue,
                };
                if let Some(found) = out.iter_mut().find(|(key, _)| *key == name) {
                    found.1 = view;
                } else {
                    out.push((name, view));
                }
            }
        }
        out
    }
}

fn find_ops<'a>(items: &'a [(String, Vec<Op>)], role: &str) -> Option<&'a [Op]> {
    items
        .iter()
        .find(|(key, _)| key == role)
        .map(|(_, value)| value.as_slice())
}

fn fixed_ops(chain: &SignalChain, role: &str) -> Vec<Op> {
    let Some(sub) = chain.subs.iter().find(|sub| sub.role == role) else {
        return Vec::new();
    };
    let mut ops = Vec::new();
    if sub.hp_hz != 0.0 {
        ops.push(Op::Hp {
            frequency: sub.hp_hz,
            kind: sub.hp_type,
            fixed: true,
        });
    }
    if sub.lp_hz != 0.0 {
        ops.push(Op::Lp {
            frequency: sub.lp_hz,
            kind: sub.lp_type,
            fixed: true,
        });
    }
    ops.extend(sub.peq.iter().map(|&(frequency, q, gain_db)| Op::Peq {
        frequency,
        q,
        gain_db,
        fixed: true,
    }));
    ops
}

/// Convert a validated chain into the upstream filter plan.
pub(crate) fn make(
    chain: &SignalChain,
    mimo: Option<&MimoPaths>,
    lfe_choice: LfeChoice,
) -> FilterPlan {
    let lfe_gain_db = match lfe_choice {
        LfeChoice::Omit => None,
        LfeChoice::Include { gain_db, lp_hz } => Some((gain_db, lp_hz)),
    };
    let lfe = !chain.subs.is_empty() && lfe_gain_db.is_some();
    let mut plan = FilterPlan {
        inputs: chain.mains.iter().map(|main| main.role.clone()).collect(),
        outputs: chain
            .mains
            .iter()
            .map(|main| main.role.clone())
            .chain(chain.subs.iter().map(|sub| sub.role.clone()))
            .collect(),
        pre_ops: chain
            .input_hp_hz
            .into_iter()
            .map(|frequency| Op::Hp {
                frequency,
                kind: BandType::Bw48,
                fixed: false,
            })
            .collect(),
        input_ops: Vec::new(),
        main_ops: Vec::new(),
        send_ops: Vec::new(),
        lfe_ops: Vec::new(),
        subs: Vec::new(),
        mimo: Vec::new(),
    };
    if lfe {
        plan.inputs.push("LFE".into());
    }
    for main in &chain.mains {
        let mut input = main
            .input_peq
            .iter()
            .map(|&(frequency, q, gain_db)| Op::Peq {
                frequency,
                q,
                gain_db,
                fixed: false,
            })
            .collect::<Vec<_>>();
        if main.fir.len() > 1 {
            input.push(Op::Fir {
                name: format!("{}_fir", main.role),
                taps: main.fir.clone(),
                fixed: false,
            });
        }
        plan.input_ops.push((main.role.clone(), input));
        let mut path = vec![Op::Gain {
            db: main.gain_db,
            fixed: false,
        }];
        path.extend(main.top_peq.iter().map(|&(frequency, q, gain_db)| Op::Peq {
            frequency,
            q,
            gain_db,
            fixed: false,
        }));
        if let Some(frequency) = main.xo_hz.filter(|value| *value != 0.0) {
            path.push(Op::Hp {
                frequency,
                kind: BandType::Lr24,
                fixed: false,
            });
            plan.send_ops.push((
                main.role.clone(),
                vec![Op::Lp {
                    frequency,
                    kind: BandType::Lr24,
                    fixed: false,
                }],
            ));
        }
        path.push(Op::Delay {
            ms: main.delay_ms,
            fixed: false,
        });
        plan.main_ops.push((main.role.clone(), path));
    }
    if lfe {
        let (gain_db, lp_hz) = lfe_gain_db.expect("LFE is enabled");
        plan.lfe_ops = vec![
            Op::Gain {
                db: gain_db,
                fixed: false,
            },
            Op::Lp {
                frequency: lp_hz,
                kind: BandType::Lr24,
                fixed: false,
            },
        ];
    }
    let mut sources = plan
        .send_ops
        .iter()
        .map(|(role, _)| role.clone())
        .collect::<Vec<_>>();
    if lfe {
        sources.push("LFE".into());
    }
    for sub in &chain.subs {
        let mut ops = chain
            .sub_bus_peq
            .iter()
            .map(|&(frequency, q, gain_db)| Op::Peq {
                frequency,
                q,
                gain_db,
                fixed: false,
            })
            .collect::<Vec<_>>();
        ops.extend(fixed_ops(chain, &sub.role));
        ops.extend(sub.allpass.iter().map(|&(frequency, q)| Op::Ap {
            frequency,
            q,
            fixed: false,
        }));
        ops.push(Op::Delay {
            ms: sub.delay_ms,
            fixed: false,
        });
        plan.subs.push((
            sub.role.clone(),
            SubFeed {
                sources: sources.clone(),
                gain_db: sub.gain_db,
                polarity: if sub.polarity > 0 { 1 } else { -1 },
                ops,
            },
        ));
    }
    if let Some(paths) = mimo {
        for path in &paths.paths {
            let mut ops = match &path.filter {
                MimoFilter::Fir { taps_48k } => vec![
                    Op::Delay {
                        ms: path.delay_ms,
                        fixed: false,
                    },
                    Op::Hp {
                        frequency: path.low_hz,
                        kind: BandType::Lr24,
                        fixed: false,
                    },
                    Op::Fir {
                        name: format!("mimo_{}_{}", path.input, path.support),
                        taps: taps_48k.clone(),
                        fixed: false,
                    },
                ],
                MimoFilter::FirLowRate { taps_low, fs_low } => vec![
                    Op::Delay {
                        ms: path.delay_ms,
                        fixed: false,
                    },
                    Op::Hp {
                        frequency: path.low_hz,
                        kind: BandType::Lr24,
                        fixed: false,
                    },
                    Op::FirLowRate {
                        name: format!("mimo_{}_{}", path.input, path.support),
                        taps_low: taps_low.clone(),
                        fs_low: *fs_low,
                        fixed: false,
                    },
                ],
                MimoFilter::Iir {
                    gain_db,
                    delay_ms,
                    polarity,
                    peq,
                    allpass,
                } => {
                    let mut ops = vec![Op::Gain {
                        db: *gain_db,
                        fixed: false,
                    }];
                    if *polarity < 0 {
                        ops.push(Op::Invert { fixed: false });
                    }
                    ops.push(Op::Delay {
                        ms: path.delay_ms + delay_ms,
                        fixed: false,
                    });
                    ops.extend(peq.iter().map(|&(frequency, q, gain_db)| Op::Peq {
                        frequency,
                        q,
                        gain_db,
                        fixed: false,
                    }));
                    ops.extend(allpass.iter().map(|&(frequency, q)| Op::Ap {
                        frequency,
                        q,
                        fixed: false,
                    }));
                    ops.push(Op::Hp {
                        frequency: path.low_hz,
                        kind: BandType::Lr24,
                        fixed: false,
                    });
                    ops.push(Op::Lp {
                        frequency: paths.fsiso,
                        kind: BandType::Lr24,
                        fixed: false,
                    });
                    ops
                }
            };
            ops.extend(fixed_ops(chain, &path.support));
            plan.mimo.push(MimoFeed {
                input: path.input.clone(),
                support: path.support.clone(),
                ops,
            });
        }
    }
    plan
}

fn multiply(out: &mut [Complex64], values: impl IntoIterator<Item = Complex64>) {
    for (left, right) in out.iter_mut().zip(values) {
        *left *= right;
    }
}

fn op_response(op: &Op, f: &[f64], skip_fixed: bool) -> Result<Vec<Complex64>, GraphError> {
    if skip_fixed && op.is_fixed() {
        return Ok(vec![Complex64::new(1.0, 0.0); f.len()]);
    }
    let result = match op {
        Op::Peq {
            frequency,
            q,
            gain_db,
            ..
        } => peaking_response(f, *frequency, *q, *gain_db, f64::from(SAMPLE_RATE)),
        Op::Fir { taps, .. } => {
            let n = 2 * f.len().saturating_sub(1);
            if n == 0 {
                return Ok(Vec::new());
            }
            let mut padded = vec![0.0; n];
            let count = n.min(taps.len());
            padded[..count].copy_from_slice(&taps[..count]);
            fft::rfft(&padded)
        }
        Op::FirLowRate {
            taps_low, fs_low, ..
        } => {
            FirView::LowRate(taps_low, *fs_low).validate()?;
            let taps = secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low);
            let n = 2 * f.len().saturating_sub(1);
            if n == 0 {
                return Ok(Vec::new());
            }
            let mut padded = vec![0.0; n];
            let count = n.min(taps.len());
            padded[..count].copy_from_slice(&taps[..count]);
            fft::rfft(&padded)
        }
        Op::Gain { db, .. } => vec![Complex64::new(10_f64.powf(db / 20.0), 0.0); f.len()],
        Op::Invert { .. } => vec![Complex64::new(-1.0, 0.0); f.len()],
        Op::Hp {
            frequency, kind, ..
        } => band_filter(f, *frequency, "HP", kind.name())
            .map_err(|error| GraphError::Invalid(error.to_string()))?,
        Op::Lp {
            frequency, kind, ..
        } => band_filter(f, *frequency, "LP", kind.name())
            .map_err(|error| GraphError::Invalid(error.to_string()))?,
        Op::Ap { frequency, q, .. } => allpass2(f, *frequency, *q),
        Op::Delay { ms, .. } => f
            .iter()
            .map(|frequency| Complex64::from_polar(1.0, -2.0 * PI * frequency * ms / 1000.0))
            .collect(),
    };
    Ok(result)
}

fn response_chain(ops: &[Op], f: &[f64], skip_fixed: bool) -> Result<Vec<Complex64>, GraphError> {
    let mut out = vec![Complex64::new(1.0, 0.0); f.len()];
    for op in ops {
        multiply(&mut out, op_response(op, f, skip_fixed)?);
    }
    Ok(out)
}

fn add_response(out: &mut Vec<(String, Vec<Complex64>)>, role: &str, value: Vec<Complex64>) {
    if let Some((_, current)) = out.iter_mut().find(|(name, _)| name == role) {
        for (left, right) in current.iter_mut().zip(value) {
            *left += right;
        }
    } else {
        out.push((role.to_owned(), value));
    }
}

/// Compute electrical responses from one input to every reachable output.
///
/// A FIR's response is one real FFT of its taps, which lands on that FFT's
/// grid, not on `f`. So when the plan holds a FIR, `f` must be the 48 kHz
/// real-FFT grid `f[k] = k * 48000 / (2 * (f.len() - 1))` with at least two
/// points, and any other `f` is [`GraphError::Invalid`]. Without a FIR,
/// every response is evaluated on `f` itself.
pub fn plan_responses(
    plan: &FilterPlan,
    f: &[f64],
    source: &str,
    skip_fixed: bool,
) -> Result<Vec<(String, Vec<Complex64>)>, GraphError> {
    if !plan.firs().is_empty() {
        check_rfft_grid(f)?;
    }
    plan_responses_prepared(plan, f, source, skip_fixed, None)
}

fn check_rfft_grid(f: &[f64]) -> Result<(), GraphError> {
    let n = 2 * f.len().saturating_sub(1);
    let step = f64::from(SAMPLE_RATE) / n as f64;
    // A millionth of a bin absorbs how a caller rounds `k * step`.
    let on_grid = n > 0
        && f.iter()
            .enumerate()
            .all(|(k, &value)| (value - k as f64 * step).abs() <= step * 1e-6);
    if on_grid {
        Ok(())
    } else {
        Err(GraphError::Invalid(format!(
            "FIR responses need the 48 kHz real-FFT grid of {} points",
            f.len()
        )))
    }
}

fn plan_responses_prepared(
    plan: &FilterPlan,
    f: &[f64],
    source: &str,
    skip_fixed: bool,
    sub_responses: Option<&[Vec<Complex64>]>,
) -> Result<Vec<(String, Vec<Complex64>)>, GraphError> {
    let mut out = Vec::new();
    let (pre, send) = if source == "LFE" {
        if !plan.inputs.iter().any(|role| role == "LFE") {
            return Err(GraphError::Invalid("unknown input LFE".into()));
        }
        (
            response_chain(&plan.lfe_ops, f, false)?,
            Some(vec![Complex64::new(1.0, 0.0); f.len()]),
        )
    } else {
        let input = find_ops(&plan.input_ops, source)
            .ok_or_else(|| GraphError::Invalid(format!("unknown input {source}")))?;
        let pre = response_chain(input, f, false)?;
        let mut main = pre.clone();
        multiply(
            &mut main,
            response_chain(
                find_ops(&plan.main_ops, source).expect("plan main"),
                f,
                false,
            )?,
        );
        add_response(&mut out, source, main);
        let send = find_ops(&plan.send_ops, source)
            .map(|ops| response_chain(ops, f, false))
            .transpose()?;
        (pre, send)
    };
    for (sub_index, (role, feed)) in plan.subs.iter().enumerate() {
        if feed.sources.iter().any(|item| item == source)
            && let Some(send) = &send
        {
            let factor = f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0);
            let mut value = pre.clone();
            multiply(&mut value, send.iter().copied());
            for item in &mut value {
                *item *= factor;
            }
            if let Some(responses) = sub_responses {
                multiply(&mut value, responses[sub_index].iter().copied());
            } else {
                multiply(&mut value, response_chain(&feed.ops, f, skip_fixed)?);
            }
            add_response(&mut out, role, value);
        }
    }
    for path in &plan.mimo {
        if path.input == source {
            add_response(
                &mut out,
                &path.support,
                response_chain(&path.ops, f, skip_fixed)?,
            );
        }
    }
    // Every route starts at the same input. Factor its common response out
    // of the sum so base and MIMO contributions receive it exactly once.
    if !plan.pre_ops.is_empty() {
        let common = response_chain(&plan.pre_ops, f, false)?;
        for (_, response) in &mut out {
            multiply(response, common.iter().copied());
        }
    }
    Ok(out)
}

/// Largest non-LFE output boost from 20 Hz through 20 kHz.
pub fn headroom_db(plan: &FilterPlan) -> Result<f64, GraphError> {
    let firs = plan.firs();
    for (_, taps) in &firs {
        taps.validate()?;
    }
    let taps = firs
        .iter()
        .map(|(_, taps)| taps.len_48k())
        .max()
        .unwrap_or(1);
    let n = 65_536_usize.max(taps.max(2).next_power_of_two());
    let f = (0..=n / 2)
        .map(|index| index as f64 * f64::from(SAMPLE_RATE) / n as f64)
        .collect::<Vec<_>>();
    // The same sub output chain is reached from every input. Compute it once;
    // keep multiplication and summation in the original per-input order.
    let sub_responses = plan
        .subs
        .iter()
        .map(|(_, feed)| response_chain(&feed.ops, &f, false))
        .collect::<Result<Vec<_>, _>>()?;
    let mut peak = 0.0_f64;
    for source in plan.inputs.iter().filter(|source| source.as_str() != "LFE") {
        for (_, response) in plan_responses_prepared(plan, &f, source, false, Some(&sub_responses))?
        {
            for (&frequency, value) in f.iter().zip(response) {
                if (20.0..=20_000.0).contains(&frequency) {
                    peak = peak.max(value.norm());
                }
            }
        }
    }
    Ok(if peak > 0.0 {
        (20.0 * peak.log10()).max(0.0)
    } else {
        0.0
    })
}

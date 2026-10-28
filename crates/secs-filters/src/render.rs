//! Time-domain rendering of complete export-plan input-to-output paths.

use crate::{FiltersError, SECTION_Q};
use secs_graph::{FilterPlan, FirView, Op, SAMPLE_RATE, round_ties_even_i64};
use std::f64::consts::PI;

const FLOOR_RATIO: f64 = 1e-6;
const MIN_LENGTH: usize = 4096;
const TAIL_GUARD: usize = 4096;

/// One complete, reachable input-to-output impulse response.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPath {
    /// Input role.
    pub input: String,
    /// Output role.
    pub output: String,
    /// Complete f64 impulse response at 48 kHz.
    pub response: Vec<f64>,
    /// Sum of the plan's delay operations along the route that arrives first
    /// (a pair fed by several routes, such as a bass-managed sub that is also
    /// a MIMO support, reports its earliest one).
    pub delay_ms: f64,
}

/// Every non-zero path of a filter plan, in input order and then output order.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPaths {
    /// Plan input roles in their original order.
    pub inputs: Vec<String>,
    /// Plan output roles in their original order.
    pub outputs: Vec<String>,
    /// Reachable, non-zero paths.
    pub paths: Vec<RenderedPath>,
    /// Shared response length.
    pub length: usize,
}

#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn normalized(b: [f64; 3], a: [f64; 3]) -> Result<Self, FiltersError> {
        let values = [b[0], b[1], b[2], a[0], a[1], a[2]];
        if a[0] == 0.0 || values.iter().any(|value| !value.is_finite()) {
            return Err(FiltersError::Invalid(
                "invalid RBJ biquad coefficients".into(),
            ));
        }
        Ok(Self {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
        })
    }

    fn peaking(frequency: f64, q: f64, gain_db: f64) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(SAMPLE_RATE);
        let alpha = w.sin() / (2.0 * q);
        let gain = 10_f64.powf(gain_db / 40.0);
        Self::normalized(
            [1.0 + alpha * gain, -2.0 * w.cos(), 1.0 - alpha * gain],
            [1.0 + alpha / gain, -2.0 * w.cos(), 1.0 - alpha / gain],
        )
    }

    fn band(frequency: f64, q: f64, high_pass: bool) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(SAMPLE_RATE);
        let alpha = w.sin() / (2.0 * q);
        let cosine = w.cos();
        let edge = if high_pass {
            (1.0 + cosine) / 2.0
        } else {
            (1.0 - cosine) / 2.0
        };
        let middle = if high_pass { -2.0 * edge } else { 2.0 * edge };
        Self::normalized(
            [edge, middle, edge],
            [1.0 + alpha, -2.0 * cosine, 1.0 - alpha],
        )
    }

    fn all_pass(frequency: f64, q: f64) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(SAMPLE_RATE);
        let alpha = w.sin() / (2.0 * q);
        let middle = -2.0 * w.cos();
        Self::normalized(
            [1.0 - alpha, middle, 1.0 + alpha],
            [1.0 + alpha, middle, 1.0 - alpha],
        )
    }

    fn apply(self, samples: &mut [f64]) {
        // Transposed direct form II, in sample order.
        let (mut z1, mut z2) = (0.0, 0.0);
        for sample in samples {
            let input = *sample;
            let output = self.b0 * input + z1;
            z1 = self.b1 * input - self.a1 * output + z2;
            z2 = self.b2 * input - self.a2 * output;
            *sample = output;
        }
    }
}

#[derive(Debug)]
struct Route {
    fir: Vec<f64>,
    gain: f64,
    delay_samples: usize,
    delay_ms: f64,
    biquads: Vec<Biquad>,
}

impl Route {
    fn from_parts(
        parts: &[&[Op]],
        route_gain: f64,
        input: &str,
        output: &str,
        max_len: usize,
    ) -> Result<Self, FiltersError> {
        let mut route = Self {
            fir: vec![1.0],
            gain: route_gain,
            delay_samples: 0,
            delay_ms: 0.0,
            biquads: Vec::new(),
        };
        for op in parts.iter().flat_map(|ops| ops.iter()) {
            match op {
                Op::Peq {
                    frequency,
                    q,
                    gain_db,
                    ..
                } => route
                    .biquads
                    .push(Biquad::peaking(*frequency, *q, *gain_db)?),
                Op::Fir { taps, .. } => {
                    route.fir = convolve(&route.fir, taps, max_len, input, output)?;
                }
                Op::FirLowRate {
                    taps_low, fs_low, ..
                } => {
                    let view = FirView::LowRate(taps_low, *fs_low);
                    view.validate()?;
                    route.fir = convolve(
                        &route.fir,
                        view.materialize().as_ref(),
                        max_len,
                        input,
                        output,
                    )?;
                }
                Op::Gain { db, .. } => route.gain *= 10_f64.powf(db / 20.0),
                Op::Invert { .. } => route.gain = -route.gain,
                Op::Hp {
                    frequency, kind, ..
                }
                | Op::Lp {
                    frequency, kind, ..
                } => {
                    let high_pass = matches!(op, Op::Hp { .. });
                    let values = SECTION_Q
                        .iter()
                        .find(|(name, _)| *name == kind.name())
                        .expect("known band type")
                        .1;
                    for &q in values {
                        route.biquads.push(Biquad::band(*frequency, q, high_pass)?);
                    }
                }
                Op::Ap { frequency, q, .. } => {
                    route.biquads.push(Biquad::all_pass(*frequency, *q)?)
                }
                Op::Delay { ms, .. } => {
                    let samples = round_ties_even_i64(*ms * f64::from(SAMPLE_RATE) / 1000.0);
                    if samples < 0 {
                        return Err(FiltersError::Invalid(format!(
                            "negative delay on {input} -> {output}"
                        )));
                    }
                    route.delay_samples = route
                        .delay_samples
                        .checked_add(samples as usize)
                        .ok_or_else(|| {
                            FiltersError::Invalid(format!(
                                "delay on {input} -> {output} exceeds max_len {max_len}"
                            ))
                        })?;
                    route.delay_ms += *ms;
                }
            }
        }
        if !route.gain.is_finite()
            || !route.delay_ms.is_finite()
            || route.fir.iter().any(|sample| !sample.is_finite())
        {
            return Err(FiltersError::Invalid(format!(
                "non-finite rendered path {input} -> {output}"
            )));
        }
        let end = route
            .delay_samples
            .checked_add(route.fir.len())
            .ok_or_else(|| {
                FiltersError::Invalid(format!(
                    "path {input} -> {output} exceeds max_len {max_len}"
                ))
            })?;
        if end > max_len {
            return Err(FiltersError::Invalid(format!(
                "path {input} -> {output} exceeds max_len {max_len}"
            )));
        }
        Ok(route)
    }

    fn render(&self, length: usize) -> Result<Vec<f64>, FiltersError> {
        let mut response = vec![0.0; length];
        let available = length
            .saturating_sub(self.delay_samples)
            .min(self.fir.len());
        for (output, input) in response[self.delay_samples..self.delay_samples + available]
            .iter_mut()
            .zip(&self.fir[..available])
        {
            *output = input * self.gain;
        }
        for biquad in &self.biquads {
            biquad.apply(&mut response);
        }
        if response.iter().any(|sample| !sample.is_finite()) {
            return Err(FiltersError::Invalid(
                "non-finite sample while rendering an RBJ cascade".into(),
            ));
        }
        Ok(response)
    }
}

fn convolve(
    left: &[f64],
    right: &[f64],
    max_len: usize,
    input: &str,
    output: &str,
) -> Result<Vec<f64>, FiltersError> {
    if left.is_empty() || right.is_empty() {
        return Ok(Vec::new());
    }
    let length = left
        .len()
        .checked_add(right.len())
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| {
            FiltersError::Invalid(format!(
                "path {input} -> {output} exceeds max_len {max_len}"
            ))
        })?;
    if length > max_len {
        return Err(FiltersError::Invalid(format!(
            "path {input} -> {output} exceeds max_len {max_len}"
        )));
    }
    let mut result = vec![0.0; length];
    for (left_index, &left_value) in left.iter().enumerate() {
        for (right_index, &right_value) in right.iter().enumerate() {
            result[left_index + right_index] += left_value * right_value;
        }
    }
    Ok(result)
}

#[derive(Debug)]
struct PairRoutes {
    input: String,
    output: String,
    routes: Vec<Route>,
}

fn find_ops<'a>(items: &'a [(String, Vec<Op>)], role: &str) -> Option<&'a [Op]> {
    items
        .iter()
        .find(|(name, _)| name == role)
        .map(|(_, ops)| ops.as_slice())
}

fn collect_routes(plan: &FilterPlan, max_len: usize) -> Result<Vec<PairRoutes>, FiltersError> {
    let mut pairs = plan
        .inputs
        .iter()
        .flat_map(|input| {
            plan.outputs.iter().map(move |output| PairRoutes {
                input: input.clone(),
                output: output.clone(),
                routes: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    let output_count = plan.outputs.len();
    let mut add = |input_index: usize,
                   output: &str,
                   parts: &[&[Op]],
                   gain: f64|
     -> Result<(), FiltersError> {
        let output_index = plan
            .outputs
            .iter()
            .position(|candidate| candidate == output)
            .expect("plan output");
        let pair = &mut pairs[input_index * output_count + output_index];
        pair.routes.push(Route::from_parts(
            parts,
            gain,
            &pair.input,
            &pair.output,
            max_len,
        )?);
        Ok(())
    };

    for (input_index, input) in plan.inputs.iter().enumerate() {
        if input == "LFE" {
            for (output, feed) in &plan.subs {
                if feed.sources.iter().any(|source| source == input) {
                    add(
                        input_index,
                        output,
                        &[&plan.lfe_ops, &feed.ops],
                        f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0),
                    )?;
                }
            }
        } else {
            let input_ops = find_ops(&plan.input_ops, input)
                .ok_or_else(|| FiltersError::Invalid(format!("unknown input {input}")))?;
            let main_ops = find_ops(&plan.main_ops, input)
                .ok_or_else(|| FiltersError::Invalid(format!("missing main path {input}")))?;
            add(input_index, input, &[input_ops, main_ops], 1.0)?;
            if let Some(send_ops) = find_ops(&plan.send_ops, input) {
                for (output, feed) in &plan.subs {
                    if feed.sources.iter().any(|source| source == input) {
                        add(
                            input_index,
                            output,
                            &[input_ops, send_ops, &feed.ops],
                            f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0),
                        )?;
                    }
                }
            }
        }
        for feed in plan.mimo.iter().filter(|feed| feed.input == *input) {
            add(input_index, &feed.support, &[&feed.ops], 1.0)?;
        }
    }
    Ok(pairs)
}

/// Render every reachable input-to-output path at 48 kHz.
///
/// Operations use the same digital RBJ sections and 48 kHz FIR materialization
/// as the configuration writers. Responses are trimmed together at -120 dB of
/// the largest sample, then rounded up to a power of two with a 4096-sample
/// minimum. `max_len` is a hard bound on both finite operations and IIR tails.
pub fn render_paths(plan: &FilterPlan, max_len: usize) -> Result<RenderedPaths, FiltersError> {
    if max_len < MIN_LENGTH {
        return Err(FiltersError::Invalid(format!(
            "max_len {max_len} is below the minimum {MIN_LENGTH}"
        )));
    }
    let pairs = collect_routes(plan, max_len)?;
    let finite_end = pairs
        .iter()
        .flat_map(|pair| &pair.routes)
        .map(|route| route.delay_samples + route.fir.len())
        .max()
        .unwrap_or(1);
    let mut candidate = MIN_LENGTH.max(finite_end.next_power_of_two()).min(max_len);

    loop {
        let mut rendered = Vec::with_capacity(pairs.len());
        let mut peak = 0.0_f64;
        for pair in &pairs {
            let mut response = vec![0.0; candidate];
            for route in &pair.routes {
                for (sum, value) in response.iter_mut().zip(route.render(candidate)?) {
                    *sum += value;
                }
            }
            peak = response
                .iter()
                .fold(peak, |largest, sample| largest.max(sample.abs()));
            rendered.push(response);
        }
        if peak == 0.0 {
            return Ok(RenderedPaths {
                inputs: plan.inputs.clone(),
                outputs: plan.outputs.clone(),
                paths: Vec::new(),
                length: MIN_LENGTH,
            });
        }
        let threshold = peak * FLOOR_RATIO;
        let last_by_pair = rendered
            .iter()
            .map(|response| response.iter().rposition(|sample| sample.abs() > threshold))
            .collect::<Vec<_>>();
        let (pair_index, last) = last_by_pair
            .iter()
            .enumerate()
            .filter_map(|(pair_index, last)| last.map(|last| (pair_index, last)))
            .max_by_key(|(_, last)| *last)
            .expect("non-zero peak has a path");
        let unguarded = pairs
            .iter()
            .zip(&last_by_pair)
            .enumerate()
            .find(|(_, (pair, last))| {
                last.is_some()
                    && pair.routes.iter().any(|route| !route.biquads.is_empty())
                    && candidate.saturating_sub(last.unwrap() + 1) < TAIL_GUARD
            })
            .map(|(index, _)| index);
        if let Some(unguarded) = unguarded {
            if candidate == max_len {
                let pair = &pairs[unguarded];
                return Err(FiltersError::Invalid(format!(
                    "path {} -> {} exceeds max_len {max_len}",
                    pair.input, pair.output
                )));
            }
            candidate = candidate.saturating_mul(2).min(max_len);
            continue;
        }
        let length = MIN_LENGTH.max((last + 1).next_power_of_two());
        if length > max_len {
            let pair = &pairs[pair_index];
            return Err(FiltersError::Invalid(format!(
                "path {} -> {} exceeds max_len {max_len}",
                pair.input, pair.output
            )));
        }
        let mut paths = Vec::new();
        for (pair, mut response) in pairs.iter().zip(rendered) {
            response.truncate(length);
            if response.iter().any(|sample| *sample != 0.0) {
                paths.push(RenderedPath {
                    input: pair.input.clone(),
                    output: pair.output.clone(),
                    response,
                    delay_ms: pair
                        .routes
                        .iter()
                        .map(|route| route.delay_ms)
                        .fold(f64::INFINITY, f64::min),
                });
            }
        }
        return Ok(RenderedPaths {
            inputs: plan.inputs.clone(),
            outputs: plan.outputs.clone(),
            paths,
            length,
        });
    }
}

//! Time-domain rendering of complete export-plan input-to-output paths.

use crate::{FiltersError, SECTION_Q};
use secs_dsp::rate::{
    DESIGN_RATE, delay_samples, fir_to_rate, is_output_rate, lowrate_to_rate, taps_at,
};
use secs_graph::{FilterPlan, FirView, Op};
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
    /// Complete f64 impulse response at `RenderedPaths::rate`.
    pub response: Vec<f64>,
    /// Sum of the plan's delay operations along the route that arrives first
    /// (a pair fed by several routes, such as a bass-managed sub that is also
    /// a MIMO support, reports its earliest one).
    pub delay_ms: f64,
}

/// Every non-zero path of a filter plan, in input order and then output order.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPaths {
    /// Sample rate of all responses, in hertz.
    pub rate: u32,
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

    fn peaking(frequency: f64, q: f64, gain_db: f64, rate: u32) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(rate);
        let alpha = w.sin() / (2.0 * q);
        let gain = 10_f64.powf(gain_db / 40.0);
        Self::normalized(
            [1.0 + alpha * gain, -2.0 * w.cos(), 1.0 - alpha * gain],
            [1.0 + alpha / gain, -2.0 * w.cos(), 1.0 - alpha / gain],
        )
    }

    fn band(frequency: f64, q: f64, high_pass: bool, rate: u32) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(rate);
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

    fn all_pass(frequency: f64, q: f64, rate: u32) -> Result<Self, FiltersError> {
        let w = 2.0 * PI * frequency / f64::from(rate);
        let alpha = w.sin() / (2.0 * q);
        let middle = -2.0 * w.cos();
        Self::normalized(
            [1.0 - alpha, middle, 1.0 + alpha],
            [1.0 + alpha, middle, 1.0 - alpha],
        )
    }

    #[cfg(test)]
    fn apply(self, samples: &mut [f64]) {
        self.apply_cancellable(samples, &|| false)
            .expect("a non-cancelling callback cannot fail");
    }

    fn apply_cancellable(
        self,
        samples: &mut [f64],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), FiltersError> {
        // Transposed direct form II, in sample order.
        let (mut z1, mut z2) = (0.0, 0.0);
        for (index, sample) in samples.iter_mut().enumerate() {
            if index.is_multiple_of(4096) && cancelled() {
                return Err(FiltersError::Cancelled);
            }
            let input = *sample;
            let output = self.b0 * input + z1;
            z1 = self.b1 * input - self.a1 * output + z2;
            z2 = self.b2 * input - self.a2 * output;
            *sample = output;
        }
        Ok(())
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
    #[cfg(test)]
    fn from_parts(
        parts: &[&[Op]],
        route_gain: f64,
        input: &str,
        output: &str,
        max_len: usize,
        rate: u32,
    ) -> Result<Self, FiltersError> {
        Self::from_parts_cancellable(parts, route_gain, input, output, max_len, rate, &|| false)
    }

    fn from_parts_cancellable(
        parts: &[&[Op]],
        route_gain: f64,
        input: &str,
        output: &str,
        max_len: usize,
        rate: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, FiltersError> {
        let mut route = Self {
            fir: vec![1.0],
            gain: route_gain,
            delay_samples: 0,
            delay_ms: 0.0,
            biquads: Vec::new(),
        };
        for op in parts.iter().flat_map(|ops| ops.iter()) {
            if cancelled() {
                return Err(FiltersError::Cancelled);
            }
            match op {
                Op::Peq {
                    frequency,
                    q,
                    gain_db,
                    ..
                } => route
                    .biquads
                    .push(Biquad::peaking(*frequency, *q, *gain_db, rate)?),
                Op::Fir { taps, .. } => {
                    let taps = fir_to_rate(taps, rate)
                        .map_err(|error| FiltersError::Invalid(error.to_string()))?;
                    route.fir = convolve(&route.fir, &taps, max_len, input, output, cancelled)?;
                }
                Op::FirLowRate {
                    taps_low, fs_low, ..
                } => {
                    let view = FirView::LowRate(taps_low, *fs_low);
                    view.validate()?;
                    let taps = lowrate_to_rate(taps_low, *fs_low, rate)
                        .map_err(|error| FiltersError::Invalid(error.to_string()))?;
                    route.fir = convolve(&route.fir, &taps, max_len, input, output, cancelled)?;
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
                        route
                            .biquads
                            .push(Biquad::band(*frequency, q, high_pass, rate)?);
                    }
                }
                Op::Ap { frequency, q, .. } => {
                    route.biquads.push(Biquad::all_pass(*frequency, *q, rate)?)
                }
                Op::Delay { ms, .. } => {
                    let samples = delay_samples(*ms, rate);
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

    fn render_cancellable(
        &self,
        length: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f64>, FiltersError> {
        if cancelled() {
            return Err(FiltersError::Cancelled);
        }
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
            biquad.apply_cancellable(&mut response, cancelled)?;
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
    cancelled: &dyn Fn() -> bool,
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
        if cancelled() {
            return Err(FiltersError::Cancelled);
        }
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

fn collect_routes(
    plan: &FilterPlan,
    max_len: usize,
    rate: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<PairRoutes>, FiltersError> {
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
        pair.routes.push(Route::from_parts_cancellable(
            parts,
            gain,
            &pair.input,
            &pair.output,
            max_len,
            rate,
            cancelled,
        )?);
        Ok(())
    };

    for (input_index, input) in plan.inputs.iter().enumerate() {
        if cancelled() {
            return Err(FiltersError::Cancelled);
        }
        if input == "LFE" {
            for (output, feed) in &plan.subs {
                if feed.sources.iter().any(|source| source == input) {
                    add(
                        input_index,
                        output,
                        &[&plan.pre_ops, &plan.lfe_ops, &feed.ops],
                        f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0),
                    )?;
                }
            }
        } else {
            let input_ops = find_ops(&plan.input_ops, input)
                .ok_or_else(|| FiltersError::Invalid(format!("unknown input {input}")))?;
            let main_ops = find_ops(&plan.main_ops, input)
                .ok_or_else(|| FiltersError::Invalid(format!("missing main path {input}")))?;
            add(
                input_index,
                input,
                &[&plan.pre_ops, input_ops, main_ops],
                1.0,
            )?;
            if let Some(send_ops) = find_ops(&plan.send_ops, input) {
                for (output, feed) in &plan.subs {
                    if feed.sources.iter().any(|source| source == input) {
                        add(
                            input_index,
                            output,
                            &[&plan.pre_ops, input_ops, send_ops, &feed.ops],
                            f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0),
                        )?;
                    }
                }
            }
        }
        for feed in plan.mimo.iter().filter(|feed| feed.input == *input) {
            add(input_index, &feed.support, &[&plan.pre_ops, &feed.ops], 1.0)?;
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
    render_paths_at(plan, max_len, DESIGN_RATE)
}

/// Render at an output rate, rebuilding FIRs and RBJ sections and rounding
/// delays at that rate. The length bound is specified in 48 kHz samples.
/// Minimum length and tail guard retain their duration; the final response
/// length is rounded up to a power of two, within the scaled bound.
pub fn render_paths_at(
    plan: &FilterPlan,
    max_len_48k: usize,
    rate: u32,
) -> Result<RenderedPaths, FiltersError> {
    render_paths_at_cancellable(plan, max_len_48k, rate, &|| false)
}

/// Render at an output rate while checking cooperative cancellation during
/// route construction, convolution, and sample rendering.
pub fn render_paths_at_cancellable(
    plan: &FilterPlan,
    max_len_48k: usize,
    rate: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<RenderedPaths, FiltersError> {
    if !is_output_rate(rate) {
        return Err(FiltersError::Invalid(
            "unsupported output sample rate".into(),
        ));
    }
    let max_len = taps_at(max_len_48k, rate);
    let min_length = taps_at(MIN_LENGTH, rate);
    let tail_guard = taps_at(TAIL_GUARD, rate);
    if max_len_48k < MIN_LENGTH {
        return Err(FiltersError::Invalid(format!(
            "max_len {max_len} is below the minimum {MIN_LENGTH}"
        )));
    }
    let pairs = collect_routes(plan, max_len, rate, cancelled)?;
    let finite_end = pairs
        .iter()
        .flat_map(|pair| &pair.routes)
        .map(|route| route.delay_samples + route.fir.len())
        .max()
        .unwrap_or(1);
    let mut candidate = min_length.max(finite_end.next_power_of_two()).min(max_len);

    loop {
        let mut rendered = Vec::with_capacity(pairs.len());
        let mut peak = 0.0_f64;
        for pair in &pairs {
            if cancelled() {
                return Err(FiltersError::Cancelled);
            }
            let mut response = vec![0.0; candidate];
            for route in &pair.routes {
                for (sum, value) in response
                    .iter_mut()
                    .zip(route.render_cancellable(candidate, cancelled)?)
                {
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
                rate,
                inputs: plan.inputs.clone(),
                outputs: plan.outputs.clone(),
                paths: Vec::new(),
                length: min_length,
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
                    && candidate.saturating_sub(last.unwrap() + 1) < tail_guard
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
        let length = min_length.max((last + 1).next_power_of_two());
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
            rate,
            inputs: plan.inputs.clone(),
            outputs: plan.outputs.clone(),
            paths,
            length,
        });
    }
}

#[cfg(test)]
mod input_hp_tests {
    use super::*;
    use secs_dsp::Complex64 as C;

    fn response(route: &Route, f: f64) -> C {
        let z = C::from_polar(1.0, -2.0 * PI * f / 48000.0);
        let fir: C = route
            .fir
            .iter()
            .enumerate()
            .map(|(i, h)| h * z.powu(i as u32))
            .sum();
        route.biquads.iter().fold(
            fir * route.gain * z.powu(route.delay_samples as u32),
            |h, b| h * (b.b0 + b.b1 * z + b.b2 * z * z) / (1.0 + b.a1 * z + b.a2 * z * z),
        )
    }

    #[test]
    fn input_hp_render_collector_prefixes_every_route() {
        let (mut chain, mimo) = crate::optimization_tests::sample();
        let off = crate::make_plan(&chain, Some(&mimo)).unwrap();
        chain.input_hp_hz = Some(27.0);
        let on = crate::make_plan(&chain, Some(&mimo)).unwrap();
        let before = collect_routes(&off, 1 << 18, 48000, &|| false).unwrap();
        let after = collect_routes(&on, 1 << 18, 48000, &|| false).unwrap();
        let hp = Route::from_parts(&[&on.pre_ops], 1.0, "L", "L", 1 << 18, 48000).unwrap();
        let f: Vec<_> = (0..=400)
            .map(|i| 10.0 * 2000.0_f64.powf(i as f64 / 400.0))
            .collect();
        let analog = secs_dsp::response::band_filter(&f, 27.0, "HP", "BW48").unwrap();
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.routes.len(), b.routes.len());
            for (route_index, (a_route, b_route)) in a.routes.iter().zip(&b.routes).enumerate() {
                assert_eq!(b_route.biquads.len(), a_route.biquads.len() + 4);
                for (got, expected) in b_route.biquads.iter().zip(&hp.biquads) {
                    assert_eq!(
                        [got.b0, got.b1, got.b2, got.a1, got.a2],
                        [
                            expected.b0,
                            expected.b1,
                            expected.b2,
                            expected.a1,
                            expected.a2
                        ]
                    );
                }
                let (mut db, mut deg, mut analog_db, mut analog_deg) =
                    (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
                for (&f, analog) in f.iter().zip(&analog) {
                    let before = response(a_route, f);
                    let after = response(b_route, f);
                    let ratio = after / (before * response(&hp, f));
                    let against_analog = after / (before * analog);
                    assert!(ratio.re.is_finite() && ratio.im.is_finite());
                    db = db.max((20.0 * ratio.norm().log10()).abs());
                    deg = deg.max(ratio.arg().to_degrees().abs());
                    analog_db = analog_db.max((20.0 * against_analog.norm().log10()).abs());
                    analog_deg = analog_deg.max(against_analog.arg().to_degrees().abs());
                }
                eprintln!(
                    "input_hp render {}->{} route={route_index}: digital max_db={db:.12e} max_deg={deg:.12e}; analog max_db={analog_db:.12e} max_deg={analog_deg:.12e}",
                    a.input, a.output
                );
                assert!(db < 0.01 && deg < 0.1);
                assert!(analog_db < 0.01);
                // Analog simulation versus bilinear-transform playback: 0.26° at
                // 20 kHz, within ADR 0031's 0.5°.
                assert!(analog_deg < 0.5);
            }
        }
        // Actual convolution IRs (routing.json and REAPER share this renderer).
        let before = render_paths(&off, 1 << 18).unwrap();
        let after = render_paths(&on, 1 << 18).unwrap();
        assert_eq!(before.paths.len(), after.paths.len());
        let mut max_abs = 0.0_f64;
        for (a, b) in before.paths.iter().zip(&after.paths) {
            assert_eq!((&a.input, &a.output), (&b.input, &b.output));
            let mut expected = a.response.clone();
            expected.resize(before.length.max(after.length), 0.0);
            for section in &hp.biquads {
                section.apply(&mut expected);
            }
            for (i, expected) in expected.iter().enumerate() {
                let got = b.response.get(i).copied().unwrap_or(0.0);
                max_abs = max_abs.max((got - expected).abs());
            }
        }
        eprintln!("input_hp convolution IR max_abs={max_abs:.12e}");
        assert!(max_abs < 1e-5);
    }
}

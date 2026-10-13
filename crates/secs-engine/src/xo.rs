//! Crossover recommendation from immutable designs and measured responses.
//!
//! Upstream: `calibration.xo_groups`, `xo_scores`, `pick_xo`, `recommend_xo`.

use std::sync::Arc;

use secs_compute::{Context, MapError, MemoryBudget, NoProgress};
use secs_dsp::pairwise_sum;
use secs_types::calibration::CalibrationSettings;

use crate::{
    Design, DesignRequest, EngineConfig, EngineError, MeasurementSet, ProjectView, SubFilter,
    Tuning,
    calibration::engine_inputs,
    chain::stage_responses,
    decay::late_db_20_150,
    design::{design, production_fitters},
    grid::design_freqs,
    levels, measurement, scorecard,
    setup::Setup,
};

/// Default crossover candidates in Hz, in upstream order.
pub const XO_CANDIDATES: [f64; 4] = [60.0, 80.0, 100.0, 120.0];

/// Group-averaged crossover quality before the candidate-relative penalty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XoScores {
    /// Position-weighted percentage within ±1 dB, 20–200 Hz (higher is better).
    pub flat: f64,
    /// Reference-position group-delay span in ms, 20–100 Hz (lower is better).
    pub gd: f64,
    /// Late/early energy ratio in dB, 20–150 Hz (lower is better).
    pub late: f64,
}

/// One candidate's metrics and combined score.
#[derive(Debug, Clone, PartialEq)]
pub struct XoRow {
    /// Crossover frequency in Hz.
    pub fc: f64,
    /// Metrics averaged over the group's measured main channels.
    pub scores: XoScores,
    /// Flatness minus GD and relative late-energy penalties (higher is better).
    pub score: f64,
}

/// Ordered crossover comparison and the best candidate for each group.
#[derive(Debug, Clone, PartialEq)]
pub struct XoRecommendation {
    /// Requested candidates in input order.
    pub candidates: Vec<f64>,
    /// Groups in configuration order, rows in candidate order.
    pub table: Vec<(String, Vec<XoRow>)>,
    /// Selected frequency per group; equal scores prefer the lower frequency.
    pub best: Vec<(String, f64)>,
}

/// Main groups in configuration order, or no groups when there are no subs.
pub fn xo_groups(config: &EngineConfig) -> Vec<String> {
    config
        .groups
        .iter()
        .filter(|(group, _)| !config.subs.is_empty() && group != "sub")
        .map(|(group, _)| group.clone())
        .collect()
}

/// Replays the final chain through 500 ms and 2 s measurement windows.
///
/// `measurements` and `config` must be the valid inputs used for `design`.
/// Panics if they are invalid or a scored main's required sub measurement is missing,
/// as upstream does; recommendation propagates input errors before reaching here.
pub fn xo_scores(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
) -> Vec<(String, XoScores)> {
    let prepared = measurement::prepare(measurements, config, &Setup::new(config))
        .expect("measurements belonging to the design");
    let filters: Vec<_> = design
        .chain
        .subs
        .iter()
        .map(|s| {
            (
                s.role.clone(),
                SubFilter {
                    hp_hz: s.hp_hz,
                    hp_type: s.hp_type,
                    lp_hz: s.lp_hz,
                    lp_type: s.lp_type,
                    peq: s.peq.clone(),
                },
            )
        })
        .collect();
    let short = measurement::long_spectra(measurements, &prepared, &filters, 500.0)
        .expect("valid design measurements");
    let long = measurement::long_spectra(measurements, &prepared, &filters, 2000.0)
        .expect("valid design measurements");
    let f = design_freqs();
    let target = config
        .target
        .sample(&f.iter().map(|v| v.max(1.0)).collect::<Vec<_>>());
    let weights = levels::weights(short.positions(), config.main_position);
    let mean = |v: &[f64]| pairwise_sum(v) / v.len() as f64;
    config
        .groups
        .iter()
        .filter(|(g, _)| g != "sub")
        .filter_map(|(group, roles)| {
            let (mut flat, mut gd, mut late) = (Vec::new(), Vec::new(), Vec::new());
            for role in roles
                .iter()
                .filter(|r| design.mains.iter().any(|m| &m.role == *r))
            {
                let (mut total, mut weight) = (0.0, 0.0);
                for (&position, &w) in short.positions().iter().zip(&weights) {
                    if short.get(position, role).is_none() {
                        continue;
                    }
                    let h = stage_responses(design, role, &short, position)
                        .expect("complete measurement position");
                    total += w * scorecard::metrics(&h.output, f, &target, (100.0, 400.0)).in1_20;
                    weight += w;
                }
                flat.push(total / weight);
                let h = stage_responses(design, role, &short, config.main_position)
                    .expect("reference main");
                gd.push(scorecard::metrics(&h.output, f, &target, (100.0, 400.0)).gd_20);
                let h = stage_responses(design, role, &long, config.main_position)
                    .expect("reference main");
                late.push(late_db_20_150(&h.output));
            }
            (!flat.is_empty()).then(|| {
                (
                    group.clone(),
                    XoScores {
                        flat: mean(&flat),
                        gd: mean(&gd),
                        late: mean(&late),
                    },
                )
            })
        })
        .collect()
}

/// Writes candidate-relative scores, selecting the lower Hz on exact ties.
/// Empty groups are omitted from the returned winners.
pub fn pick_xo(table: &mut [(String, Vec<XoRow>)]) -> Vec<(String, f64)> {
    table
        .iter_mut()
        .filter_map(|(group, rows)| {
            let low = rows.iter().map(|r| r.scores.late).reduce(f64::min)?;
            for r in rows.iter_mut() {
                r.score = r.scores.flat - 0.5 * r.scores.gd - (r.scores.late - low);
            }
            let best = rows.iter().reduce(|a, b| {
                if b.score > a.score || (b.score == a.score && b.fc < a.fc) {
                    b
                } else {
                    a
                }
            })?;
            Some((group.clone(), best.fc))
        })
        .collect()
}

/// Designs independent XO candidates on the context's ordered map, then scores them.
/// Progress counts finished candidates, not their internal design stages. Cancellation
/// is shared with every production fitter; no process pool is created.
pub fn recommend_xo(
    view: &ProjectView<'_>,
    irs: &MeasurementSet,
    settings: &CalibrationSettings,
    tuning: &Tuning,
    candidates: &[f64],
    ctx: &Context,
) -> Result<XoRecommendation, EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
    let inputs = engine_inputs(view, irs, settings)?;
    let groups = xo_groups(&inputs.config);
    if groups.is_empty() {
        return Err(EngineError::Input(
            "no subs for crossover recommendation".into(),
        ));
    }
    let progress = ctx.stage("engine.xo_recommendation", candidates.len() as u64);
    let trials = ctx
        .try_map(candidates.to_vec(), |fc| {
            let mut trial = settings.clone();
            trial.xo = groups.iter().map(|g| (g.clone(), fc)).collect();
            let inputs = engine_inputs(view, irs, &trial)?;
            let inner = Context::new(
                1,
                MemoryBudget::new(ctx.budget().total()),
                Arc::new(NoProgress),
                ctx.cancel().clone(),
            );
            let result = design(
                &DesignRequest {
                    measurements: &inputs.measurements,
                    config: &inputs.config,
                    tuning,
                    subs_only: false,
                    adapter_notes: &inputs.notes,
                },
                &inner,
                &*production_fitters(inner.cancel()),
            )?;
            let scores = xo_scores(&result, &inputs.measurements, &inputs.config);
            ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
            progress.advance(1);
            Ok(scores)
        })
        .map_err(|e| match e {
            MapError::Cancelled => EngineError::Cancelled,
            MapError::Item { error, .. } => error,
        })?;
    let mut table: Vec<_> = groups
        .into_iter()
        .filter_map(|g| {
            let mut rows = Vec::new();
            for &fc in candidates {
                // Upstream's frequency-keyed dictionaries collapse duplicates,
                // retaining first insertion order and the last trial's value.
                if rows.iter().any(|r: &XoRow| r.fc == fc) {
                    continue;
                }
                let index = candidates
                    .iter()
                    .rposition(|c| *c == fc)
                    .expect("candidate key");
                if let Some((_, scores)) = trials[index].iter().find(|(group, _)| group == &g) {
                    rows.push(XoRow {
                        fc,
                        scores: *scores,
                        score: 0.0,
                    });
                }
            }
            (!rows.is_empty()).then_some((g, rows))
        })
        .collect();
    let best = pick_xo(&mut table);
    Ok(XoRecommendation {
        candidates: candidates.to_vec(),
        table,
        best,
    })
}

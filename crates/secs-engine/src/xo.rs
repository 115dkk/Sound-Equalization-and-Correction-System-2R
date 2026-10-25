//! Crossover recommendation from immutable designs and measured responses.
//!
//! Upstream: `calibration.xo_groups`, `xo_scores`, `pick_xo`, `recommend_xo`.

use secs_compute::Context;
use secs_dsp::pairwise_sum;
use secs_types::calibration::CalibrationSettings;

use crate::{
    Design, DesignRequest, EngineConfig, EngineError, MeasurementSet, ProjectView, SubFilter,
    Tuning,
    calibration::{engine_inputs, resolve_xo},
    chain::stage_responses,
    decay::late_db_20_150,
    design::{DesignBytes, design_prepared_for_xo, production_fitters},
    grid::design_freqs,
    levels, measurement, scorecard,
    setup::Setup,
};

/// Default crossover candidates in Hz, in upstream order.
pub const XO_CANDIDATES: [f64; 4] = [60.0, 80.0, 100.0, 120.0];

/// Arrays one XO score evaluation keeps alive, in spectra: `stage_responses`'
/// delay, `top`, the sub `sum` and one sub term, the `lr4` HP and LP,
/// `combined` and `output`.
const SCORE_SPECTRA: u64 = 8;

/// What `recommend_xo` reserves (TD-24), as `(shared, candidate)`.
///
/// `shared` is held from before `measurement::prepare` to the end: its `h`
/// and the unfiltered `ScoreWindows` `short` and `long` (three spectra per IR
/// cell) plus the build scratch. Each candidate reserves `candidate` before
/// it starts: its design on a serial child context (`DesignBytes::retained`
/// and one RC channel at a time) and its scoring (`filtered_spectra` copies of
/// every sub cell in both windows, their filter responses, `SCORE_SPECTRA`).
pub(crate) fn xo_bytes(set: &MeasurementSet, config: &EngineConfig) -> (u64, u64) {
    let setup = Setup::new(config);
    let shared = measurement::spectra_bytes(set)
        .saturating_mul(3)
        .saturating_add(measurement::build_scratch_bytes(setup.sub_filters.len()));
    let design = DesignBytes::new(set, config, &setup, false);
    let scoring = (2 * measurement::cells(set, Some(&config.subs))
        + config.subs.len() as u64
        + SCORE_SPECTRA)
        .saturating_mul(measurement::SPECTRUM_BYTES);
    (
        shared,
        design
            .retained
            .saturating_add(design.channel)
            .saturating_add(scoring),
    )
}

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
/// as upstream does; recommendation checks both (`check_scored_positions`) and
/// returns input errors before reaching here.
pub fn xo_scores(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
) -> Vec<(String, XoScores)> {
    let prepared = measurement::prepare(measurements, config, &Setup::new(config))
        .expect("measurements belonging to the design");
    xo_scores_prepared(design, measurements, config, &prepared, None)
}

/// What `xo_scores` replays (TD-48): every main it scores (a group member
/// measured at the reference) at every position where that main was measured,
/// through `stage_responses`, which needs every sub there whenever the main has
/// a crossover. BA and RC only use complete positions, so a design succeeds
/// without them. Recommendation gives every group a nonzero crossover, so each
/// such position must hold every sub; the first one that does not is named.
fn check_scored_positions(set: &MeasurementSet, config: &EngineConfig) -> Result<(), EngineError> {
    let scored = config
        .groups
        .iter()
        .filter(|(group, _)| group != "sub")
        .flat_map(|(_, roles)| roles)
        .filter(|role| {
            config.mains.contains(role) && set.get(config.main_position, role).is_some()
        });
    for role in scored {
        for (position, measured) in &set.positions {
            let has = |r: &str| measured.iter().any(|(m, _)| m == r);
            if !has(role) {
                continue;
            }
            if let Some(sub) = config.subs.iter().find(|sub| !has(sub)) {
                return Err(EngineError::Input(format!(
                    "position {position} {role}: missing {sub} for crossover scoring"
                )));
            }
        }
    }
    Ok(())
}

struct ScoreWindows {
    short: measurement::Spectra,
    long: measurement::Spectra,
}

impl ScoreWindows {
    fn new(
        measurements: &MeasurementSet,
        prepared: &measurement::Prepared,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            short: measurement::long_spectra_until(measurements, prepared, &[], 500.0, cancelled)?,
            long: measurement::long_spectra_until(measurements, prepared, &[], 2000.0, cancelled)?,
        })
    }
}

fn xo_scores_prepared(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    prepared: &measurement::Prepared,
    windows: Option<&ScoreWindows>,
) -> Vec<(String, XoScores)> {
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
    let (short, long) = if let Some(windows) = windows {
        (
            measurement::filtered_spectra(&windows.short, &filters),
            measurement::filtered_spectra(&windows.long, &filters),
        )
    } else {
        (
            measurement::long_spectra(measurements, prepared, &filters, 500.0)
                .expect("valid design measurements"),
            measurement::long_spectra(measurements, prepared, &filters, 2000.0)
                .expect("valid design measurements"),
        )
    };
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
                let mut reference_gd = None;
                for (&position, &w) in short.positions().iter().zip(&weights) {
                    if short.get(position, role).is_none() {
                        continue;
                    }
                    let h = stage_responses(design, role, &short, position)
                        .expect("complete measurement position");
                    total += w * scorecard::flatness_20(&h.output, f, &target, (100.0, 400.0));
                    if position == config.main_position {
                        reference_gd = Some(scorecard::gd_span_20(&h.output, f));
                    }
                    weight += w;
                }
                flat.push(total / weight);
                gd.push(reference_gd.unwrap_or_else(|| {
                    let h = stage_responses(design, role, &short, config.main_position)
                        .expect("reference main");
                    scorecard::gd_span_20(&h.output, f)
                }));
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
    let mut inputs = engine_inputs(view, irs, settings)?;
    let groups = xo_groups(&inputs.config);
    if groups.is_empty() {
        return Err(EngineError::Input(
            "no subs for crossover recommendation".into(),
        ));
    }
    if !candidates.is_empty() {
        inputs.measurements =
            crate::normalize_measurements(&inputs.measurements, settings.sample_rate_policy, ctx)?;
    }
    let progress = ctx.stage("engine.xo_recommendation", candidates.len() as u64);
    // Reserve the shared preparation and score windows before allocating them
    // (TD-24); each batch of candidates reserves its designs out of the rest.
    let (shared, candidate) = if candidates.is_empty() {
        (0, 0)
    } else {
        xo_bytes(&inputs.measurements, &inputs.config)
    };
    let _shared = crate::memory::reserve(ctx, shared, candidate)?;
    // Empty candidate lists retain their old no-preparation behaviour.
    let prepared = if candidates.is_empty() {
        None
    } else {
        Some(measurement::prepare_until(
            &inputs.measurements,
            &inputs.config,
            &Setup::new(&inputs.config),
            &|| ctx.cancel().is_cancelled(),
        )?)
    };
    if prepared.is_some() {
        check_scored_positions(&inputs.measurements, &inputs.config)?;
    }
    // Window lengths, taper, onset and time origin are all XO-independent.
    // Applying final sub filters here would not be: BA can change them.
    let windows = prepared
        .as_ref()
        .map(|prepared| {
            ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
            ScoreWindows::new(&inputs.measurements, prepared, &|| {
                ctx.cancel().is_cancelled()
            })
        })
        .transpose()?;
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
    // A candidate's permit is taken on this thread for its whole batch, never
    // inside a pool task (TD-23); its design runs on a serial child of this
    // job, so it reserves nothing more and waits on no pool work.
    let trials = crate::memory::try_map(
        ctx,
        candidates.to_vec(),
        shared,
        |_| candidate,
        |fc| {
            // engine_inputs depends on the trial only through settings.xo.
            // Keep its zero-value fallback and ordering in the same helper.
            let mut config = inputs.config.clone();
            let selected: Vec<_> = groups.iter().map(|g| (g.clone(), fc)).collect();
            config.xo = resolve_xo(&config.groups, &selected, view.project_xo);
            let prepared = prepared.as_ref().expect("nonempty candidates");
            let inner = ctx.child_serial();
            let result = design_prepared_for_xo(
                &DesignRequest {
                    measurements: &inputs.measurements,
                    config: &config,
                    tuning,
                    subs_only: false,
                    adapter_notes: &inputs.notes,
                },
                &inner,
                &*production_fitters(inner.cancel()),
                prepared,
            )?;
            // Only the unfiltered windows are shared. The final multiplication
            // by BA-derived sub filters still happens per candidate, in order.
            let scores = xo_scores_prepared(
                &result,
                &inputs.measurements,
                &config,
                prepared,
                windows.as_ref(),
            );
            ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
            progress.advance(1);
            Ok(scores)
        },
    )?;
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

#[cfg(test)]
mod perf_tests {
    use super::*;
    use crate::tests::cases;
    use std::sync::Arc;

    #[test]
    fn xo_preparation_is_candidate_independent() {
        for name in ["dolby", "room", "room_dba", "room_filters"] {
            let case = cases::load(name);
            let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
            let view = ProjectView {
                groups: &case.config.groups,
                project_xo: &case.config.xo,
                target: &case.config.target,
            };
            let base = engine_inputs(&view, &case.measurements, &settings).unwrap();
            let prepared =
                measurement::prepare(&base.measurements, &base.config, &Setup::new(&base.config))
                    .unwrap();
            for fc in [0.0, 60.0, 80.0, 100.0, 120.0] {
                let mut trial = settings.clone();
                trial.xo = xo_groups(&base.config)
                    .into_iter()
                    .map(|g| (g, fc))
                    .collect();
                let inputs = engine_inputs(&view, &case.measurements, &trial).unwrap();
                assert_eq!(
                    inputs.config.xo,
                    resolve_xo(&base.config.groups, &trial.xo, view.project_xo)
                );
                assert_eq!(inputs.notes, base.notes);
                let other = measurement::prepare(
                    &inputs.measurements,
                    &inputs.config,
                    &Setup::new(&inputs.config),
                )
                .unwrap();
                assert_eq!(prepared.base_s.to_bits(), other.base_s.to_bits());
                assert_eq!(prepared.arrival_s, other.arrival_s);
                assert_eq!(prepared.align_delay_ms, other.align_delay_ms);
                for &p in prepared.h.positions() {
                    for r in prepared.h.roles_at(p) {
                        let bits = |h: &Arc<[crate::C]>| {
                            h.iter()
                                .map(|z| (z.re.to_bits(), z.im.to_bits()))
                                .collect::<Vec<_>>()
                        };
                        assert_eq!(
                            bits(prepared.h.get(p, r).unwrap()),
                            bits(other.h.get(p, r).unwrap())
                        );
                        assert_eq!(
                            bits(prepared.h0.get(p, r).unwrap()),
                            bits(other.h0.get(p, r).unwrap())
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn xo_shared_windows_preserve_filtered_bits() {
        for name in ["dolby", "room_dba", "room_filters"] {
            let case = cases::load(name);
            let setup = Setup::new(&case.config);
            let prepared = measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
            let mut filters = setup.sub_filters.clone();
            filters.retain(|(r, _)| r != &case.config.subs[0]);
            filters.push((
                case.config.subs[0].clone(),
                SubFilter {
                    hp_hz: 23.0,
                    hp_type: secs_types::chain::BandType::Lr24,
                    lp_hz: 157.0,
                    lp_type: secs_types::chain::BandType::Bw12,
                    peq: vec![(71.0, 2.5, -3.0)],
                },
            ));
            for window in [500.0, 2000.0] {
                let raw =
                    measurement::long_spectra(&case.measurements, &prepared, &[], window).unwrap();
                let shared = measurement::filtered_spectra(&raw, &filters);
                let own =
                    measurement::long_spectra(&case.measurements, &prepared, &filters, window)
                        .unwrap();
                for &p in own.positions() {
                    for role in own.roles_at(p) {
                        let a = own.get(p, role).unwrap();
                        let b = shared.get(p, role).unwrap();
                        for (a, b) in a.iter().zip(b.iter()) {
                            assert_eq!(
                                (a.re.to_bits(), a.im.to_bits()),
                                (b.re.to_bits(), b.im.to_bits())
                            );
                        }
                        if !filters.iter().any(|(r, _)| r == role) {
                            assert!(Arc::ptr_eq(raw.get(p, role).unwrap(), b));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn xo_shared_preparation_preserves_candidate_bits() {
        let case = cases::load("room_dba");
        let prepared =
            measurement::prepare(&case.measurements, &case.config, &Setup::new(&case.config))
                .unwrap();
        let ctx = Context::serial();
        for fc in [60.0, 120.0] {
            let mut config = case.config.clone();
            for (_, xo) in &mut config.xo {
                *xo = fc;
            }
            let req = DesignRequest {
                measurements: &case.measurements,
                config: &config,
                tuning: &case.tuning,
                subs_only: false,
                adapter_notes: &[],
            };
            let fitters = production_fitters(ctx.cancel());
            let own = crate::design::design(&req, &ctx, &*fitters).unwrap();
            let shared =
                crate::design::design_prepared(&req, &ctx, &*fitters, &prepared, &|_, _| {})
                    .unwrap();
            assert_eq!(
                serde_json::to_vec(&own.chain).unwrap(),
                serde_json::to_vec(&shared.chain).unwrap()
            );
            for (a, b) in own.mains.iter().zip(&shared.mains) {
                for (x, y) in a.after.iter().zip(b.after.iter()) {
                    assert_eq!(
                        (x.re.to_bits(), x.im.to_bits()),
                        (y.re.to_bits(), y.im.to_bits())
                    );
                }
                assert!(Arc::ptr_eq(
                    shared
                        .spectra
                        .raw
                        .get(config.main_position, &b.role)
                        .unwrap(),
                    prepared.h0.get(config.main_position, &b.role).unwrap()
                ));
            }
            for (a, b) in own.scores.iter().zip(&shared.scores) {
                for (x, y) in [
                    (&a.spl_before, &b.spl_before),
                    (&a.spl_after, &b.spl_after),
                    (&a.gd_before, &b.gd_before),
                    (&a.gd_after, &b.gd_after),
                ] {
                    assert_eq!(
                        x.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        y.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                    );
                }
            }
            let own_scores = xo_scores(&own, &case.measurements, &config);
            let windows = ScoreWindows::new(&case.measurements, &prepared, &|| false).unwrap();
            let shared_scores = xo_scores_prepared(
                &shared,
                &case.measurements,
                &config,
                &prepared,
                Some(&windows),
            );
            for ((ga, a), (gb, b)) in own_scores.iter().zip(shared_scores) {
                assert_eq!(ga, &gb);
                assert_eq!(
                    [a.flat, a.gd, a.late].map(f64::to_bits),
                    [b.flat, b.gd, b.late].map(f64::to_bits)
                );
            }
        }
    }
}

use std::sync::{Arc, Mutex};

use secs_compute::{Context, MemoryBudget};
use secs_engine::{
    Design, DesignRequest, EngineConfig, Measurement, MeasurementSet, Metrics, ProjectView,
    Samples, Tuning, design, engine_inputs, production_fitters,
};
use secs_project::{
    Project, calibration_from_project, measurement_signature, store_calibration, target_key,
};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrateParams, CalibrationSummary, ChannelScore, ErrorCode, IpcError, JobKind, JobResult,
    JobStarted, MetricsSummary, MimoParams, MimoSummary, Notice, ResultsSummary, VerdictSummary,
    XoRecommendation,
};

use crate::jobs::JobContext;
use crate::{EventSink, Service, lock};

#[derive(Debug)]
pub(crate) struct CalibrationRun {
    pub summary: CalibrationSummary,
    pub settings: CalibrationSettings,
    pub signature: String,
    pub target_key: String,
    pub design: Arc<Design>,
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
}

#[derive(Debug)]
pub(crate) struct MimoResult {
    pub summary: MimoSummary,
    pub settings_key: String,
    pub calibration_key: String,
    pub run: secs_engine::MimoRun,
}

#[derive(Debug, Default)]
pub(crate) struct Results {
    pub calibration: Option<CalibrationRun>,
    pub mimo: Option<MimoResult>,
    /// The last subs-only run; the BA tab shows it, a full run clears it
    /// (`step_calibration.py:1868-1876`).
    pub subs: Option<CalibrationRun>,
}

#[derive(Debug)]
pub(crate) struct Prepared {
    pub project: Project,
    pub settings: CalibrationSettings,
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub notes: Vec<String>,
    pub signature: String,
    pub target_key: String,
}

impl Service {
    pub(crate) fn start_calibration(
        &self,
        params: CalibrateParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let prepared = self.prepare(params.settings)?;
        let subs_only = params.subs_only;
        let results = self.results_state.clone();
        let session = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        self.jobs.start(JobKind::Calibrate, events, move |job| {
            let ctx = compute_context(&job, memory);
            let request = DesignRequest {
                measurements: &prepared.measurements,
                config: &prepared.config,
                tuning: &Tuning::default(),
                subs_only,
                adapter_notes: &prepared.notes,
            };
            let fitters = production_fitters(job.cancel());
            let design = design(&request, &ctx, fitters.as_ref()).map_err(engine_error)?;
            job.begin_commit()?;
            let summary = summarize(&design, &prepared.settings);
            let run = CalibrationRun {
                summary: summary.clone(),
                settings: prepared.settings.clone(),
                signature: prepared.signature,
                target_key: prepared.target_key,
                design: Arc::new(design),
                measurements: prepared.measurements,
                config: prepared.config,
            };
            {
                let mut state = lock(&results);
                if subs_only {
                    state.subs = Some(run);
                } else {
                    state.calibration = Some(run);
                    state.subs = None;
                }
            }
            {
                let mut session = lock(&session);
                let open = session.require_mut().map_err(IpcError::from)?;
                store_calibration(open.store.project_mut(), &prepared.settings);
                open.dirty = true;
            }
            persist_cache(&results, &session)?;
            let current = summaries(&results, &session)?;
            Ok(JobResult::Calibration(Box::new(current)))
        })
    }

    pub(crate) fn start_xo(&self, events: Arc<dyn EventSink>) -> Result<JobStarted, IpcError> {
        let prepared = self.prepare(None)?;
        let memory = self.deps.memory.available_bytes();
        self.jobs.start(JobKind::RecommendXo, events, move |job| {
            let groups = project_groups(&prepared.project);
            let view = ProjectView {
                groups: &groups,
                project_xo: &prepared.project.xo,
                target: &prepared.project.target,
            };
            let ctx = compute_context(&job, memory);
            let result = secs_engine::xo::recommend_xo(
                &view,
                &prepared.measurements,
                &prepared.settings,
                &Tuning::default(),
                &secs_engine::xo::XO_CANDIDATES,
                &ctx,
            )
            .map_err(engine_error)?;
            Ok(JobResult::Xo(XoRecommendation {
                best: result.best,
                candidates: result.candidates,
            }))
        })
    }

    pub(crate) fn start_mimo(
        &self,
        params: MimoParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let settings = {
            let session = lock(&self.session);
            let open = session.require().map_err(IpcError::from)?;
            params
                .settings
                .unwrap_or_else(|| calibration_from_project(open.store.project()))
        };
        let state = lock(&self.results_state);
        let calibration = state.calibration.as_ref().ok_or_else(|| {
            IpcError::new(ErrorCode::EngineInput, "calibration must run before MIMO")
        })?;
        if calibration_stale(
            calibration,
            lock(&self.session)
                .require()
                .map_err(IpcError::from)?
                .store
                .project(),
        ) {
            return Err(IpcError::new(
                ErrorCode::StaleInputs,
                "calibration result is stale",
            ));
        }
        if settings.engine_key() != calibration.settings.engine_key() {
            return Err(IpcError::new(
                ErrorCode::StaleInputs,
                "MIMO settings do not match the calibration result",
            ));
        }
        let calibration_key = calibration.settings.engine_key();
        let design = calibration.design.clone();
        let measurements = calibration.measurements.clone();
        let config = calibration.config.clone();
        drop(state);
        let memory = self.deps.memory.available_bytes();
        let results = self.results_state.clone();
        let session = self.session.clone();
        self.jobs.start(JobKind::Mimo, events, move |job| {
            let ctx = compute_context(&job, memory);
            let run = secs_engine::mimo::compute(
                &design,
                &measurements,
                &config,
                &settings,
                &Tuning::default(),
                &ctx,
            )
            .map_err(engine_error)?;
            job.begin_commit()?;
            let summary = MimoSummary {
                stale: false,
                kind: if run.fir { "fir" } else { "iir" }.into(),
                fsiso: run.paths.fsiso,
                support_count: run.paths.paths.len(),
                fir_taps_total: run
                    .paths
                    .paths
                    .iter()
                    .map(|path| match &path.filter {
                        secs_types::chain::MimoFilter::Fir { taps_48k } => taps_48k.len(),
                        secs_types::chain::MimoFilter::Iir { .. } => 0,
                    })
                    .sum(),
                notes: run.notes.iter().flat_map(notices).collect(),
                paths: run.paths.clone(),
            };
            lock(&results).mimo = Some(MimoResult {
                summary: summary.clone(),
                settings_key: settings.mimo_key(),
                calibration_key,
                run,
            });
            persist_cache(&results, &session)?;
            Ok(JobResult::Mimo(summary))
        })
    }

    pub(crate) fn results(&self) -> Result<ResultsSummary, IpcError> {
        summaries(&self.results_state, &self.session)
    }

    pub(crate) fn prepare(
        &self,
        override_settings: Option<CalibrationSettings>,
    ) -> Result<Prepared, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut().map_err(IpcError::from)?;
        let project = open.store.project().clone();
        let settings = override_settings.unwrap_or_else(|| calibration_from_project(&project));
        let measurements = load_measurements(&mut open.store)?;
        let groups = project_groups(&project);
        let view = ProjectView {
            groups: &groups,
            project_xo: &project.xo,
            target: &project.target,
        };
        let inputs = engine_inputs(&view, &measurements, &settings).map_err(engine_error)?;
        Ok(Prepared {
            signature: signature_key(&project),
            target_key: target_key_string(&project),
            project,
            settings,
            measurements: inputs.measurements,
            config: inputs.config,
            notes: inputs.notes,
        })
    }
}

pub(crate) fn compute_context(job: &JobContext, available: u64) -> Context {
    let threads = std::thread::available_parallelism().map_or(1, |value| value.get());
    Context::new(
        threads,
        MemoryBudget::new((available / 2).max(64 * 1024 * 1024)),
        job.progress_sink(),
        job.cancel().clone(),
    )
}

pub(crate) fn project_groups(project: &Project) -> Vec<(String, Vec<String>)> {
    project
        .config
        .groups()
        .into_iter()
        .map(|(name, speakers)| {
            (
                name.to_owned(),
                speakers
                    .into_iter()
                    .map(|speaker| speaker.role.clone())
                    .collect(),
            )
        })
        .collect()
}

pub(crate) fn load_measurements(
    store: &mut secs_project::MeasurementStore,
) -> Result<MeasurementSet, IpcError> {
    let mut positions = Vec::new();
    for record in store.records() {
        if record.pos < 0 {
            continue;
        }
        let mut roles = Vec::new();
        for role in record.ir.keys() {
            let (samples, sample_rate) = store
                .ir_of(role, record.pos)
                .map_err(crate::ServiceError::from)?;
            roles.push((
                role.clone(),
                Measurement {
                    samples: Samples::F32(samples.into()),
                    sample_rate,
                    t0: record.t0.get(role).copied().unwrap_or(0.0),
                },
            ));
        }
        if !roles.is_empty() {
            positions.push((record.pos as u32, roles));
        }
    }
    positions.sort_by_key(|(position, _)| *position);
    Ok(MeasurementSet { positions })
}

fn summarize(design: &Design, settings: &CalibrationSettings) -> CalibrationSummary {
    let scorecard: Vec<_> = design
        .scores
        .iter()
        .map(|scores| ChannelScore {
            role: scores.role.clone(),
            before: metrics(scores.before),
            after: metrics(scores.after),
        })
        .collect();
    let count = scorecard.len().max(1) as f64;
    let average = |pick: fn(&MetricsSummary) -> f64, after: bool| {
        scorecard
            .iter()
            .map(|score| pick(if after { &score.after } else { &score.before }))
            .sum::<f64>()
            / count
    };
    CalibrationSummary {
        stale: false,
        stale_reason: None,
        mode: settings.mode,
        ultra_ms: settings.ultra_ms,
        verdict: VerdictSummary {
            within_1db_before_pct: average(|value| value.in1_20, false),
            within_1db_after_pct: average(|value| value.in1_20, true),
            low_band_time_spread_before_ms: average(|value| value.gd_20, false),
            low_band_time_spread_after_ms: average(|value| value.gd_20, true),
            added_delay_ms: design.chain.latency_ms,
            position_spread_db: secs_engine::preview::sub_combine(design)
                .and_then(|view| view.seat.map(|(_, after)| after))
                .unwrap_or(0.0),
        },
        scorecard,
        latency_ms: design.chain.latency_ms,
        warnings: Vec::new(),
        notes: design.notes.iter().flat_map(notices).collect(),
        chain: design.chain.clone(),
    }
}

// Collection notes expand in producer order: one record per role/filter/mode.
// Every parameter is a scalar string; no JSON or translated prose crosses IPC.
fn notices(note: &secs_engine::Note) -> Vec<Notice> {
    use secs_engine::{MimoNote, Note};
    let record = |code: &str, values: Vec<(&str, String)>| Notice {
        code: code.into(),
        params: values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    };
    match note {
        Note::SubFilters(filters) => {
            filters
                .iter()
                .flat_map(|(role, filter)| {
                    let mut records = vec![record(
                        "sub_filters",
                        vec![
                            ("role", role.clone()),
                            ("hp_hz", filter.hp_hz.to_string()),
                            ("hp_type", filter.hp_type.name().into()),
                            ("lp_hz", filter.lp_hz.to_string()),
                            ("lp_type", filter.lp_type.name().into()),
                            ("peq_count", filter.peq.len().to_string()),
                        ],
                    )];
                    records.extend(filter.peq.iter().enumerate().map(
                        |(index, (hz, q, gain_db))| {
                            record(
                                "sub_filter_peq",
                                vec![
                                    ("role", role.clone()),
                                    ("index", index.to_string()),
                                    ("hz", hz.to_string()),
                                    ("q", q.to_string()),
                                    ("gain_db", gain_db.to_string()),
                                ],
                            )
                        },
                    ));
                    records
                })
                .collect()
        }
        Note::PhaseStrength(values) => values
            .iter()
            .map(|(role, strength)| {
                record(
                    "phase_strength",
                    vec![("role", role.clone()), ("strength", strength.to_string())],
                )
            })
            .collect(),
        Note::PhaseBand(values) => values
            .iter()
            .map(|(role, hz)| {
                let mut params = vec![
                    ("role", role.clone()),
                    ("enabled", hz.is_some().to_string()),
                ];
                if let Some(hz) = hz {
                    params.push(("hz", hz.to_string()));
                }
                record("phase_band", params)
            })
            .collect(),
        Note::LongModeBoost { cap_db, modes } => modes
            .iter()
            .map(|(hz, t60_s)| {
                record(
                    "long_mode_boost",
                    vec![
                        ("cap_db", cap_db.to_string()),
                        ("hz", hz.to_string()),
                        ("t60_s", t60_s.to_string()),
                    ],
                )
            })
            .collect(),
        Note::GdCuts { role, cuts } => cuts
            .iter()
            .enumerate()
            .map(|(index, (hz, q, gain_db))| {
                record(
                    "gd_cuts",
                    vec![
                        ("role", role.clone()),
                        ("index", index.to_string()),
                        ("hz", hz.to_string()),
                        ("q", q.to_string()),
                        ("gain_db", gain_db.to_string()),
                    ],
                )
            })
            .collect(),
        Note::TwoWay {
            roles,
            gain_db,
            xo_hz,
        } => roles
            .iter()
            .map(|role| {
                record(
                    "two_way",
                    vec![
                        ("role", role.clone()),
                        ("gain_db", gain_db.to_string()),
                        ("xo_hz", xo_hz.to_string()),
                    ],
                )
            })
            .collect(),
        Note::Mimo(note) => match note {
            MimoNote::InsufficientPositions { measured } => vec![record(
                "mimo_insufficient_positions",
                vec![("measured", measured.to_string()), ("required", "3".into())],
            )],
            MimoNote::ExcludedSupports(roles) => roles
                .iter()
                .map(|role| record("mimo_support_excluded", vec![("role", role.clone())]))
                .collect(),
            MimoNote::SupportRanges(ranges) => ranges
                .iter()
                .map(|(role, range)| {
                    let mut params = vec![
                        ("role", role.clone()),
                        ("enabled", range.is_some().to_string()),
                    ];
                    if let Some((lo, hi)) = range {
                        params.extend([("lo_hz", lo.to_string()), ("hi_hz", hi.to_string())]);
                    }
                    record("mimo_support_range", params)
                })
                .collect(),
            MimoNote::GuardReduced { role, gain_db } => vec![record(
                "mimo_guard_reduced",
                vec![("role", role.clone()), ("gain_db", gain_db.to_string())],
            )],
            MimoNote::GuardDisabled { role } => {
                vec![record("mimo_guard_disabled", vec![("role", role.clone())])]
            }
            MimoNote::SupportLimit { max_supports } => vec![record(
                "mimo_support_limit",
                vec![("max_supports", max_supports.to_string())],
            )],
            MimoNote::FirBudget {
                support_count,
                taps_total,
            } => vec![record(
                "mimo_fir_budget",
                vec![
                    ("support_count", support_count.to_string()),
                    ("taps_total", taps_total.to_string()),
                    ("sample_rate_hz", "48000".into()),
                ],
            )],
        },
        _ => vec![scalar_notice(note)],
    }
}

fn scalar_notice(note: &secs_engine::Note) -> Notice {
    use secs_engine::Note;

    let mut params = std::collections::BTreeMap::new();
    let code = match note {
        Note::LiftCapped {
            from_ms,
            taps,
            lift_ms,
        } => {
            params.insert("from_ms".into(), from_ms.to_string());
            params.insert("taps".into(), taps.to_string());
            params.insert("lift_ms".into(), lift_ms.to_string());
            "lift_capped"
        }
        Note::RcLimit {
            hz,
            auto,
            schroeder_hz,
        } => {
            params.insert("hz".into(), hz.to_string());
            params.insert("auto".into(), auto.to_string());
            if let Some(value) = schroeder_hz {
                params.insert("schroeder_hz".into(), value.to_string());
            }
            "rc_limit"
        }
        Note::Latency {
            latency_ms,
            lift_ms,
            main_extra_ms,
        } => {
            params.insert("latency_ms".into(), latency_ms.to_string());
            params.insert("lift_ms".into(), lift_ms.to_string());
            params.insert("main_extra_ms".into(), main_extra_ms.to_string());
            "latency"
        }
        Note::BaCost(value) => {
            params.insert("value".into(), value.to_string());
            "ba_cost"
        }
        Note::BaBusPass {
            first,
            second,
            kept_first,
        } => {
            params.insert("first".into(), first.to_string());
            params.insert("second".into(), second.to_string());
            params.insert("kept_first".into(), kept_first.to_string());
            "ba_bus_pass"
        }
        Note::BaManual => "ba_manual",
        Note::LayoutFallback(value) => {
            params.insert("value".into(), value.clone());
            "layout_fallback"
        }
        Note::Layout(value) => {
            params.insert("value".into(), value.clone());
            "layout"
        }
        Note::BusGainAnchored(value) => {
            params.insert("value".into(), value.to_string());
            "bus_gain_anchored"
        }
        Note::SubFilters(_)
        | Note::PhaseStrength(_)
        | Note::PhaseBand(_)
        | Note::LongModeBoost { .. }
        | Note::GdCuts { .. }
        | Note::TwoWay { .. }
        | Note::Mimo(_) => unreachable!("collection notes expand in notices"),
    };
    Notice {
        code: code.into(),
        params,
    }
}

fn metrics(value: Metrics) -> MetricsSummary {
    MetricsSummary {
        level: value.level,
        in1_20: value.in1_20,
        in2_20: value.in2_20,
        in1_200: value.in1_200,
        in2_200: value.in2_200,
        in1_2000: value.in1_2000,
        in2_2000: value.in2_2000,
        turns: value.turns,
        gd_20: value.gd_20,
        gd_100: value.gd_100,
        gd_500: value.gd_500,
        gd24_abs: value.gd24_abs,
        gd24_abs_hz: value.gd24_abs_hz,
        gd24_max: value.gd24_max,
        gd24_span: value.gd24_span,
        gd_30: value.gd_30,
        pre_db: value.pre_db,
    }
}

fn summaries(
    results: &Arc<Mutex<Results>>,
    session: &Arc<Mutex<crate::session::Session>>,
) -> Result<ResultsSummary, IpcError> {
    let session = lock(session);
    let project = session.require().map_err(IpcError::from)?.store.project();
    let state = lock(results);
    let live_calibration = state.calibration.as_ref().map(|run| {
        let mut summary = run.summary.clone();
        summary.stale_reason = calibration_stale_reason(run, project).map(str::to_owned);
        summary.stale = summary.stale_reason.is_some();
        summary
    });
    let cached = session.require().map_err(IpcError::from)?.cached.as_ref();
    let calibration = live_calibration.or_else(|| {
        cached.map(|cached| {
            let mut summary = cached.calibration.clone();
            summary.stale_reason = cached_stale_reason(cached, project).map(str::to_owned);
            summary.stale = summary.stale_reason.is_some();
            summary
        })
    });
    let live_mimo = state.mimo.as_ref().map(|result| {
        let mut summary = result.summary.clone();
        summary.stale = calibration.as_ref().is_none_or(|value| value.stale)
            || state
                .calibration
                .as_ref()
                .is_none_or(|run| run.settings.engine_key() != result.calibration_key)
            || calibration_from_project(project).mimo_key() != result.settings_key;
        summary
    });
    let mimo = live_mimo.or_else(|| {
        cached.and_then(|cache| {
            cache.mimo.clone().map(|mut summary| {
                let current_mimo_key = calibration_from_project(project).mimo_key();
                summary.stale = calibration.as_ref().is_none_or(|value| value.stale)
                    || cache.mimo_key.as_deref() != Some(current_mimo_key.as_str())
                    || cache.mimo_calibration_key.as_deref() != Some(cache.engine_key.as_str())
                    || cache.engine_key != calibration_from_project(project).engine_key();
                summary
            })
        })
    });
    Ok(ResultsSummary { calibration, mimo })
}

fn persist_cache(
    results: &Arc<Mutex<Results>>,
    session: &Arc<Mutex<crate::session::Session>>,
) -> Result<(), IpcError> {
    let state = lock(results);
    let Some(calibration) = state.calibration.as_ref() else {
        return Ok(());
    };
    let cached = crate::cache::CachedResults {
        calibration: calibration.summary.clone(),
        mimo: state.mimo.as_ref().map(|result| result.summary.clone()),
        measurement_signature: calibration.signature.clone(),
        target_key: calibration.target_key.clone(),
        engine_key: calibration.settings.engine_key(),
        mimo_key: state
            .mimo
            .as_ref()
            .map(|result| result.settings_key.clone()),
        mimo_calibration_key: state
            .mimo
            .as_ref()
            .map(|result| result.calibration_key.clone()),
        labels: crate::cache::CachedRunLabels {
            mode: match calibration.settings.mode {
                secs_types::calibration::Mode::Zero => "zero",
                secs_types::calibration::Mode::Mixed => "mixed",
                secs_types::calibration::Mode::Linear => "linear",
                secs_types::calibration::Mode::Ultra => "ultra",
            }
            .into(),
            ultra_ms: calibration.settings.ultra_ms,
            latency_ms: calibration.design.chain.latency_ms,
            rc_hi_hz: calibration.design.rc_hi,
            mimo_max_supports: calibration.settings.mimo_max_supports.max(0) as usize,
        },
        mimo_paths: state.mimo.as_ref().map(|result| result.run.paths.clone()),
    };
    drop(state);
    let mut session = lock(session);
    let open = session.require_mut().map_err(IpcError::from)?;
    crate::cache::save(&open.path, &cached).map_err(crate::ServiceError::from)?;
    open.cached = Some(cached);
    Ok(())
}

pub(crate) fn calibration_stale(run: &CalibrationRun, project: &Project) -> bool {
    calibration_stale_reason(run, project).is_some()
}

fn calibration_stale_reason(run: &CalibrationRun, project: &Project) -> Option<&'static str> {
    stale_reason(
        &run.signature,
        &run.settings.engine_key(),
        &run.target_key,
        project,
    )
}

pub(crate) fn cached_stale_reason(
    cached: &crate::cache::CachedResults,
    project: &Project,
) -> Option<&'static str> {
    stale_reason(
        &cached.measurement_signature,
        &cached.engine_key,
        &cached.target_key,
        project,
    )
}

fn stale_reason(
    signature: &str,
    engine_key: &str,
    target_key: &str,
    project: &Project,
) -> Option<&'static str> {
    if signature != signature_key(project) {
        Some("measurements")
    } else if engine_key != calibration_from_project(project).engine_key() {
        Some("settings")
    } else if target_key != target_key_string(project) {
        Some("target")
    } else {
        None
    }
}

pub(crate) fn signature_key(project: &Project) -> String {
    // The upstream signature omits UUIDs, so replacing a waveform at the same
    // role, position and t0 otherwise leaves a completed design looking current.
    let identities = match &project.measurements {
        secs_project::PyJson::Array(entries) => entries
            .iter()
            .map(|entry| match entry {
                secs_project::PyJson::Object(fields) => fields
                    .iter()
                    .find(|(key, _)| key == "uuid")
                    .map(|(_, value)| value),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    format!("{:?}:{identities:?}", measurement_signature(project))
}
pub(crate) fn target_key_string(project: &Project) -> String {
    format!("{:?}", target_key(project))
}

pub(crate) fn engine_error(error: secs_engine::EngineError) -> IpcError {
    match error {
        secs_engine::EngineError::Cancelled => IpcError::new(ErrorCode::Cancelled, "cancelled"),
        other => IpcError::new(ErrorCode::EngineInput, other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_engine::Note;
    use secs_engine::SubFilter;
    use secs_types::chain::BandType;

    #[test]
    fn stale_reasons_prioritize_measurements_then_settings_then_target() {
        let project = Project::default();
        let signature = signature_key(&project);
        let engine = calibration_from_project(&project).engine_key();
        let target = target_key_string(&project);
        assert_eq!(
            stale_reason("changed", "changed", "changed", &project),
            Some("measurements")
        );
        assert_eq!(
            stale_reason(&signature, "changed", "changed", &project),
            Some("settings")
        );
        assert_eq!(
            stale_reason(&signature, &engine, "changed", &project),
            Some("target")
        );
        assert_eq!(stale_reason(&signature, &engine, &target, &project), None);
    }

    #[test]
    fn flat_notice_schemas_preserve_scalar_values_and_order() {
        use secs_engine::MimoNote;
        use serde_json::json;
        let input = [
            Note::PhaseStrength(vec![("L".into(), 0.5), ("R".into(), 1.0)]),
            Note::PhaseBand(vec![("L".into(), Some(100.0)), ("R".into(), None)]),
            Note::SubFilters(vec![(
                "SW1".into(),
                SubFilter {
                    hp_hz: 10.0,
                    hp_type: BandType::Lr12,
                    lp_hz: 80.0,
                    lp_type: BandType::Lr24,
                    peq: vec![(40.0, 1.0, -2.0)],
                },
            )]),
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: vec![(40.0, 2.0)],
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: vec![(50.0, 2.0, -3.0)],
            },
            Note::TwoWay {
                roles: vec!["SW1".into(), "SW2".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            Note::Mimo(MimoNote::InsufficientPositions { measured: 2 }),
            Note::Mimo(MimoNote::ExcludedSupports(vec!["SW2".into()])),
            Note::Mimo(MimoNote::SupportRanges(vec![
                ("L".into(), Some((50.25, 150.0))),
                ("R".into(), None),
            ])),
            Note::Mimo(MimoNote::GuardReduced {
                role: "L".into(),
                gain_db: -6.25,
            }),
            Note::Mimo(MimoNote::GuardDisabled { role: "R".into() }),
            Note::Mimo(MimoNote::SupportLimit { max_supports: 2 }),
            Note::Mimo(MimoNote::FirBudget {
                support_count: 2,
                taps_total: 16000,
            }),
        ];
        assert_eq!(
            serde_json::to_value(input.iter().flat_map(notices).collect::<Vec<_>>()).unwrap(),
            json!([
                {"code":"phase_strength","params":{"role":"L","strength":"0.5"}},
                {"code":"phase_strength","params":{"role":"R","strength":"1"}},
                {"code":"phase_band","params":{"role":"L","enabled":"true","hz":"100"}},
                {"code":"phase_band","params":{"role":"R","enabled":"false"}},
                {"code":"sub_filters","params":{"role":"SW1","hp_hz":"10","hp_type":"LR12","lp_hz":"80","lp_type":"LR24","peq_count":"1"}},
                {"code":"sub_filter_peq","params":{"role":"SW1","index":"0","hz":"40","q":"1","gain_db":"-2"}},
                {"code":"long_mode_boost","params":{"cap_db":"3","hz":"40","t60_s":"2"}},
                {"code":"gd_cuts","params":{"role":"L","index":"0","hz":"50","q":"2","gain_db":"-3"}},
                {"code":"two_way","params":{"role":"SW1","gain_db":"-1","xo_hz":"50"}},
                {"code":"two_way","params":{"role":"SW2","gain_db":"-1","xo_hz":"50"}},
                {"code":"mimo_insufficient_positions","params":{"measured":"2","required":"3"}},
                {"code":"mimo_support_excluded","params":{"role":"SW2"}},
                {"code":"mimo_support_range","params":{"role":"L","enabled":"true","lo_hz":"50.25","hi_hz":"150"}},
                {"code":"mimo_support_range","params":{"role":"R","enabled":"false"}},
                {"code":"mimo_guard_reduced","params":{"role":"L","gain_db":"-6.25"}},
                {"code":"mimo_guard_disabled","params":{"role":"R"}},
                {"code":"mimo_support_limit","params":{"max_supports":"2"}},
                {"code":"mimo_fir_budget","params":{"support_count":"2","taps_total":"16000","sample_rate_hz":"48000"}}
            ])
        );
    }

    #[test]
    fn every_engine_note_maps_to_a_notice_code_and_params() {
        let notes = vec![
            Note::LiftCapped {
                from_ms: 2.0,
                taps: 3,
                lift_ms: 1.0,
            },
            Note::SubFilters(vec![(
                "SW1".into(),
                SubFilter {
                    hp_hz: 10.0,
                    hp_type: BandType::Lr12,
                    lp_hz: 80.0,
                    lp_type: BandType::Lr24,
                    peq: vec![(40.0, 1.0, -2.0)],
                },
            )]),
            Note::RcLimit {
                hz: 300.0,
                auto: true,
                schroeder_hz: Some(250.0),
            },
            Note::Latency {
                latency_ms: 10.0,
                lift_ms: 2.0,
                main_extra_ms: 1.0,
            },
            Note::PhaseStrength(vec![("L".into(), 0.5)]),
            Note::PhaseBand(vec![("L".into(), Some(100.0))]),
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: vec![(40.0, 2.0)],
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: vec![(50.0, 2.0, -3.0)],
            },
            Note::BaCost(1.5),
            Note::BaBusPass {
                first: 1.0,
                second: 2.0,
                kept_first: true,
            },
            Note::BaManual,
            Note::LayoutFallback("fallback".into()),
            Note::Layout("dba".into()),
            Note::TwoWay {
                roles: vec!["SW1".into(), "SW2".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            Note::BusGainAnchored(-2.0),
            Note::Mimo(secs_engine::MimoNote::SupportLimit { max_supports: 2 }),
        ];
        let codes: Vec<_> = notes
            .iter()
            .flat_map(notices)
            .map(|value| value.code)
            .collect();
        assert_eq!(
            codes,
            [
                "lift_capped",
                "sub_filters",
                "sub_filter_peq",
                "rc_limit",
                "latency",
                "phase_strength",
                "phase_band",
                "long_mode_boost",
                "gd_cuts",
                "ba_cost",
                "ba_bus_pass",
                "ba_manual",
                "layout_fallback",
                "layout",
                "two_way",
                "two_way",
                "bus_gain_anchored",
                "mimo_support_limit",
            ]
        );
        assert_eq!(notices(&notes[0])[0].params["from_ms"], "2");
        assert_eq!(notices(&notes[13])[0].params["role"], "SW1");
    }
}

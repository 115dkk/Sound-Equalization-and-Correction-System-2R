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
        let results = self.results_state.clone();
        let session = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        self.jobs.start(JobKind::Calibrate, events, move |job| {
            let ctx = compute_context(&job, memory);
            let request = DesignRequest {
                measurements: &prepared.measurements,
                config: &prepared.config,
                tuning: &Tuning::default(),
                subs_only: false,
                adapter_notes: &prepared.notes,
            };
            let fitters = production_fitters(job.cancel());
            let design = design(&request, &ctx, fitters.as_ref()).map_err(engine_error)?;
            job.begin_commit()?;
            let summary = summarize(&design);
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
                state.calibration = Some(run);
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
                notes: run.notes.iter().map(notice).collect(),
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

fn summarize(design: &Design) -> CalibrationSummary {
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
        notes: design.notes.iter().map(notice).collect(),
        chain: design.chain.clone(),
    }
}

fn notice(note: &secs_engine::Note) -> Notice {
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
        Note::SubFilters(filters) => {
            let values: Vec<_> = filters
                .iter()
                .map(|(role, filter)| {
                    serde_json::json!({
                        "role": role,
                        "hp_hz": filter.hp_hz,
                        "hp_type": filter.hp_type.name(),
                        "lp_hz": filter.lp_hz,
                        "lp_type": filter.lp_type.name(),
                        "peq": filter.peq,
                    })
                })
                .collect();
            params.insert(
                "filters".into(),
                serde_json::to_string(&values).unwrap_or_default(),
            );
            "sub_filters"
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
        Note::PhaseStrength(values) => {
            params.insert(
                "values".into(),
                serde_json::to_string(values).unwrap_or_default(),
            );
            "phase_strength"
        }
        Note::PhaseBand(values) => {
            params.insert(
                "values".into(),
                serde_json::to_string(values).unwrap_or_default(),
            );
            "phase_band"
        }
        Note::LongModeBoost { cap_db, modes } => {
            params.insert("cap_db".into(), cap_db.to_string());
            params.insert(
                "modes".into(),
                serde_json::to_string(modes).unwrap_or_default(),
            );
            "long_mode_boost"
        }
        Note::GdCuts { role, cuts } => {
            params.insert("role".into(), role.clone());
            params.insert(
                "cuts".into(),
                serde_json::to_string(cuts).unwrap_or_default(),
            );
            "gd_cuts"
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
        Note::TwoWay {
            roles,
            gain_db,
            xo_hz,
        } => {
            params.insert(
                "roles".into(),
                serde_json::to_string(roles).unwrap_or_default(),
            );
            params.insert("gain_db".into(), gain_db.to_string());
            params.insert("xo_hz".into(), xo_hz.to_string());
            "two_way"
        }
        Note::BusGainAnchored(value) => {
            params.insert("value".into(), value.to_string());
            "bus_gain_anchored"
        }
        Note::Mimo(value) => {
            params.insert("value".into(), value.clone());
            "mimo"
        }
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
        summary.stale = calibration_stale(run, project);
        summary
    });
    let cached = session.require().map_err(IpcError::from)?.cached.as_ref();
    let calibration = live_calibration.or_else(|| {
        cached.map(|cached| {
            let mut summary = cached.calibration.clone();
            summary.stale = cached.measurement_signature != signature_key(project)
                || cached.target_key != target_key_string(project)
                || cached.engine_key != calibration_from_project(project).engine_key();
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
    run.signature != signature_key(project)
        || run.target_key != target_key_string(project)
        || run.settings.engine_key() != calibration_from_project(project).engine_key()
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
            Note::Mimo("range".into()),
        ];
        let codes: Vec<_> = notes.iter().map(notice).map(|value| value.code).collect();
        assert_eq!(
            codes,
            [
                "lift_capped",
                "sub_filters",
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
                "bus_gain_anchored",
                "mimo",
            ]
        );
        assert_eq!(notice(&notes[0]).params["from_ms"], "2");
        assert_eq!(notice(&notes[13]).params["roles"], r#"["SW1","SW2"]"#);
    }
}

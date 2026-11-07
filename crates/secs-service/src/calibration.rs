use std::sync::Arc;

use secs_compute::{Context, FixedMemory, MemoryBudget};
use secs_engine::{
    CalibrationRequest, Design, Measurement, MeasurementSet, Metrics, ProjectView, Samples, Tuning,
    calibrate,
};
use secs_project::{
    Project, calibration_from_project, measurement_signature, store_calibration, target_key,
};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrateParams, CalibrationSummary, ChannelScore, ErrorCode, IpcError, JobKind, JobResult,
    JobStarted, LateEnergy, MetricsSummary, MimoChannelScore, MimoLateEnergy, MimoParams,
    MimoSummary, Notice, ResultsSummary, VerdictSummary, XoRecommendation,
};

use crate::jobs::JobContext;
use crate::measure::json::JsonExt;
use crate::results::{CacheOwner, CalibrationRun, MimoResult, calibration_stale};
use crate::session::{self, ProjectTicket};
use crate::{EventSink, Service, lock};

/// What loading the measurements needs, taken under the session lock
/// without reading any WAV (TD-37).
#[derive(Debug)]
pub(crate) struct Snapshot {
    ticket: ProjectTicket,
    path: std::path::PathBuf,
    project: Project,
    project_settings: CalibrationSettings,
    settings: CalibrationSettings,
}

#[derive(Debug)]
struct LoadedMeasurements {
    measurements: MeasurementSet,
    notices: Vec<Notice>,
}

pub(crate) struct Prepared {
    pub ticket: ProjectTicket,
    pub project: Project,
    pub project_settings: CalibrationSettings,
    pub settings: CalibrationSettings,
    pub measurements: MeasurementSet,
    pub measurement_notices: Vec<Notice>,
    pub signature: String,
    pub target_key: String,
    pub config_key: String,
}

impl Service {
    pub(crate) fn start_calibration(
        &self,
        params: CalibrateParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let run_mimo = params.mimo && !params.subs_only;
        let background = params.background;
        let selection = run_mimo.then(|| self.require_acceleration()).transpose()?;
        let snapshot = self.snapshot(params.settings)?;
        if run_mimo {
            check_mimo_preconditions(&snapshot.project, &snapshot.settings)?;
        }
        let subs_only = params.subs_only;
        let session = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        self.jobs.start(JobKind::Calibrate, events, move |job| {
            job.log(settings_notice(&snapshot.settings, background));
            let prepared = snapshot.load(&job)?;
            let ctx = compute_context(&job, memory, background);
            let groups = project_groups(&prepared.project);
            let inverted = prepared.project.config.inverted_mains();
            let view = ProjectView {
                groups: &groups,
                project_xo: &prepared.project.xo,
                target: &prepared.project.target,
                inverted: &inverted,
            };
            let calibration = calibrate(
                &CalibrationRequest {
                    view: &view,
                    irs: &prepared.measurements,
                    settings: &prepared.settings,
                    subs_only,
                },
                &ctx,
            )
            .map_err(engine_error)?;
            let design = Arc::new(calibration.design);
            // The 2 s export view is built outside the session lock.
            let late_energy = secs_engine::late_energy(&design).map_err(engine_error)?;
            let ticket = prepared.ticket;
            let calibration_key = run_key(
                &prepared.signature,
                &prepared.target_key,
                &prepared.settings.engine_key(),
                &prepared.config_key,
            );
            let commit_calibration = |open: &mut session::OpenProject| {
                let mut summary = summarize(&design, &prepared.settings, &late_energy);
                summary.notes.extend(prepared.measurement_notices.clone());
                if calibration.resampled > 0 {
                    summary.notes.push(Notice {
                        code: "measurement_resampled".into(),
                        params: [
                            ("count".into(), calibration.resampled.to_string()),
                            ("target_hz".into(), "48000".into()),
                        ]
                        .into(),
                    });
                }
                let run = CalibrationRun::new(
                    summary,
                    prepared.settings.clone(),
                    prepared.signature,
                    prepared.target_key,
                    prepared.config_key,
                    calibration_key.clone(),
                    design.clone(),
                );
                open.results.commit_calibration(run, subs_only);
                let stores_settings =
                    calibration_from_project(open.project()) == prepared.project_settings;
                if stores_settings {
                    open.edit(|project| store_calibration(project, &prepared.settings));
                }
                Ok((
                    (open.path.clone(), open.results.cache_snapshot()),
                    stores_settings,
                ))
            };
            let (path, cached) = if run_mimo {
                session::commit_phase(&session, ticket, &job, commit_calibration)?
            } else {
                session::commit(&session, ticket, &job, commit_calibration)?
            };
            let owner = if subs_only {
                CacheOwner::Subs
            } else {
                CacheOwner::Calibration
            };
            persist_cache(cached, &path, ticket, &session, owner);
            if let Some(selection) = selection {
                compute_and_commit_mimo(
                    &session,
                    ticket,
                    &job,
                    memory,
                    design,
                    prepared.settings,
                    selection,
                    calibration_key,
                    background,
                )?;
            }
            Ok(JobResult::Calibration)
        })
    }

    pub(crate) fn start_xo(
        &self,
        background: bool,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let snapshot = self.snapshot(None)?;
        let recommendation_key = xo_rec_key(&snapshot.project, &snapshot.settings);
        let memory = self.deps.memory.available_bytes();
        let session = self.session.clone();
        self.jobs.start(JobKind::RecommendXo, events, move |job| {
            job.log(settings_notice(&snapshot.settings, background));
            let prepared = snapshot.load(&job)?;
            let groups = project_groups(&prepared.project);
            let inverted = prepared.project.config.inverted_mains();
            let view = ProjectView {
                groups: &groups,
                project_xo: &prepared.project.xo,
                target: &prepared.project.target,
                inverted: &inverted,
            };
            let ctx = compute_context(&job, memory, background);
            let result = secs_engine::xo::recommend_xo(
                &view,
                &prepared.measurements,
                &prepared.settings,
                &Tuning::default(),
                &secs_engine::xo::XO_CANDIDATES,
                &ctx,
            )
            .map_err(engine_error)?;
            let recommendation = XoRecommendation {
                best: result.best,
                candidates: result.candidates,
            };
            session::commit(&session, prepared.ticket, &job, |open| {
                let mut settings = calibration_from_project(open.project());
                let xo_unchanged = settings.xo == prepared.settings.xo
                    && settings.xo_rec_include == prepared.settings.xo_rec_include;
                if xo_unchanged {
                    for (group, hz) in &recommendation.best {
                        match settings.xo.iter_mut().find(|(name, _)| name == group) {
                            Some((_, current)) => *current = *hz,
                            None => settings.xo.push((group.clone(), *hz)),
                        }
                    }
                }
                settings.xo_rec = recommendation.best.clone();
                settings.xo_rec_key.clone_from(&recommendation_key);
                open.edit(|project| store_calibration(project, &settings));
                Ok(((), true))
            })?;
            Ok(JobResult::Xo(recommendation))
        })
    }

    pub(crate) fn start_mimo(
        &self,
        params: MimoParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let selection = self.require_acceleration()?;
        let session_guard = lock(&self.session);
        let open = session_guard.require().map_err(IpcError::from)?;
        let ticket = session_guard.ticket()?;
        let settings = params
            .settings
            .unwrap_or_else(|| calibration_from_project(open.project()));
        check_mimo_preconditions(open.project(), &settings)?;
        let calibration = open.results.live_calibration().ok_or_else(|| {
            IpcError::new(ErrorCode::EngineInput, "calibration must run before MIMO")
        })?;
        if calibration_stale(calibration, open.project()) {
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
        let calibration_key = calibration.run_key.clone();
        let design = calibration.shared_design();
        drop(session_guard);
        let memory = self.deps.memory.available_bytes();
        let session = self.session.clone();
        self.jobs.start(JobKind::Mimo, events, move |job| {
            job.log(settings_notice(&settings, false));
            compute_and_commit_mimo(
                &session,
                ticket,
                &job,
                memory,
                design,
                settings,
                selection,
                calibration_key,
                false,
            )?;
            Ok(JobResult::Mimo)
        })
    }

    pub(crate) fn results(&self) -> Result<ResultsSummary, IpcError> {
        self.read_open(|open, compute| open.results.summary(open.project(), compute))
    }

    /// Holds the session lock only to copy the project; the WAVs are read by
    /// [`Snapshot::load`] inside the job, so a busy group answers `JobBusy`
    /// before any file is read and other requests do not wait (TD-37).
    pub(crate) fn snapshot(
        &self,
        override_settings: Option<CalibrationSettings>,
    ) -> Result<Snapshot, IpcError> {
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        let project = open.project().clone();
        let project_settings = calibration_from_project(&project);
        let mut settings = override_settings.unwrap_or_else(|| project_settings.clone());
        // A request may carry settings the edit checks never saw; the run, its
        // notes and its export use the bands the engine uses (ADR 0024).
        settings.rc_bands = secs_types::calibration::valid_rc_bands(&settings.rc_bands);
        Ok(Snapshot {
            ticket: session.ticket()?,
            path: open.path.clone(),
            project,
            project_settings,
            settings,
        })
    }
}

fn check_mimo_preconditions(
    project: &Project,
    settings: &CalibrationSettings,
) -> Result<(), IpcError> {
    if !settings.mimo_on {
        return Err(engine_error(secs_engine::EngineError::Input(
            "MIMO is disabled".into(),
        )));
    }
    let positions = secs_project::measured_positions(project).len();
    let (available, reason) = secs_engine::mimo::availability(positions);
    if !available {
        return Err(engine_error(secs_engine::EngineError::Input(reason)));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compute_and_commit_mimo(
    session: &Arc<std::sync::Mutex<session::Session>>,
    ticket: ProjectTicket,
    job: &JobContext,
    memory: u64,
    design: Arc<Design>,
    settings: CalibrationSettings,
    selection: secs_engine::mimo::AccelerationSelection,
    calibration_key: String,
    background: bool,
) -> Result<(), IpcError> {
    let compute_key = crate::compute::ComputeIdentity::from(&selection.status);
    let ctx = compute_context(job, memory, background);
    let run = secs_engine::mimo::compute_with_acceleration(
        &design,
        design.measurements(),
        design.config(),
        &settings,
        &Tuning::default(),
        &ctx,
        &selection,
    )
    .map_err(|error| engine_error(error).with_param("stage", "mimo"))?;
    let (path, cached) = session::commit(session, ticket, job, |open| {
        let summary = summarize_mimo(&run);
        open.results.commit_mimo(MimoResult {
            summary,
            settings_key: settings.mimo_key(),
            compute_key,
            calibration_key,
            run,
        });
        Ok(((open.path.clone(), open.results.cache_snapshot()), false))
    })?;
    persist_cache(cached, &path, ticket, session, CacheOwner::Mimo);
    Ok(())
}

fn summarize_mimo(run: &secs_engine::mimo::MimoRun) -> MimoSummary {
    MimoSummary {
        stale: false,
        kind: if run.fir { "fir" } else { "iir" }.into(),
        fsiso: run.paths.fsiso,
        support_count: run.paths.paths.len(),
        fir_taps_total: run
            .paths
            .paths
            .iter()
            .map(|path| path.filter.fir_len_48k())
            .sum(),
        notes: run
            .notes
            .iter()
            .flat_map(|note| {
                secs_engine::NoteRecords::record(note)
                    .into_iter()
                    .map(notice_from_record)
            })
            .chain((run.compute.fallbacks > 0).then(|| Notice {
                code: "compute_fallback".into(),
                params: [("count".into(), run.compute.fallbacks.to_string())].into(),
            }))
            .collect(),
        input_count: run
            .paths
            .paths
            .iter()
            .map(|path| &path.input)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        scorecard: run
            .scorecard
            .iter()
            .map(|score| MimoChannelScore {
                role: score.role.clone(),
                after_mimo: metrics(score.after_mimo),
            })
            .collect(),
        late_energy: run
            .late_energy
            .iter()
            .map(|row| MimoLateEnergy {
                role: row.role.clone(),
                position: row.position,
                after_mimo_db: row.after_mimo_db,
            })
            .collect(),
        compute: run.compute.clone(),
    }
}

impl Snapshot {
    /// Reads every IR of the snapshot without the session lock, checking for
    /// cancellation between files. The commit checks the ticket.
    pub(crate) fn load(self, job: &JobContext) -> Result<Prepared, IpcError> {
        let mut store = secs_project::MeasurementStore::new(&self.path, self.project.clone());
        let loaded = load_measurements_with_notices(&mut store, job.cancel())?;
        check_rc_agreement(&self.project, &self.settings, &loaded.measurements)?;
        let project = self.project;
        Ok(Prepared {
            ticket: self.ticket,
            signature: signature_key(&project),
            target_key: target_key_string(&project),
            config_key: config_key(&project),
            project,
            project_settings: self.project_settings,
            settings: self.settings,
            measurements: loaded.measurements,
            measurement_notices: loaded.notices,
        })
    }
}

fn check_rc_agreement(
    project: &Project,
    settings: &CalibrationSettings,
    measurements: &MeasurementSet,
) -> Result<(), IpcError> {
    if settings.rc_hi.is_some()
        || settings.rc_hi_basis != secs_types::calibration::RcHiBasis::Agreement
    {
        return Ok(());
    }
    let mains: Vec<_> = project_groups(project)
        .into_iter()
        .filter(|(group, _)| group != "sub")
        .flat_map(|(_, roles)| roles)
        .collect();
    let measured = measurements
        .positions
        .iter()
        .filter(|(_, roles)| {
            roles
                .iter()
                .any(|(role, m)| mains.contains(role) && !m.samples.is_empty())
        })
        .count();
    if measured < 3 {
        return Err(IpcError::new(
            ErrorCode::EngineInput,
            "RC agreement requires at least 3 measured positions",
        )
        .with_param("reason", "rc_agreement_positions")
        .with_param("measured", measured.to_string())
        .with_param("required", "3"));
    }
    Ok(())
}

fn settings_notice(settings: &CalibrationSettings, background: bool) -> Notice {
    Notice {
        code: "calibration_settings".into(),
        params: [
            ("engine_key".into(), settings.engine_key()),
            ("background".into(), background.to_string()),
        ]
        .into(),
    }
}

pub(crate) fn xo_rec_key(project: &Project, settings: &CalibrationSettings) -> String {
    let mut settings = settings.clone();
    settings.xo.clear();
    [
        signature_key(project),
        target_key_string(project),
        config_key(project),
        settings.engine_key(),
    ]
    .join("|")
}

fn background_threads(available: usize, background: bool) -> (usize, bool) {
    let available = available.max(1);
    if !background {
        (available, false)
    } else if available == 1 {
        (1, true)
    } else {
        ((available / 2).max(1), false)
    }
}

pub(crate) fn compute_context(job: &JobContext, available: u64, background: bool) -> Context {
    let parallelism = std::thread::available_parallelism().map_or(1, |value| value.get());
    let (threads, paced) = background_threads(parallelism, background);
    let cancel = if paced {
        job.cancel().paced(std::time::Duration::from_millis(50))
    } else {
        job.cancel().clone()
    };
    Context::new(
        threads,
        MemoryBudget::from_probe(&FixedMemory(available), 0.5, 64 * 1024 * 1024),
        job.progress_sink(),
        cancel,
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

fn mic_curve(cal: &secs_mdat::CalInfo) -> Option<secs_audio::MicCalibration> {
    let frequencies = cal.freq.as_ref()?;
    let gains = cal.gain.as_ref()?;
    if frequencies.len() != gains.len() {
        return None;
    }
    let phase = cal
        .phase
        .as_ref()
        .filter(|phase| phase.len() == frequencies.len());
    let points = frequencies
        .iter()
        .zip(gains)
        .enumerate()
        .map(
            |(index, (&frequency, &gain_db))| secs_audio::CalibrationPoint {
                frequency: f64::from(frequency),
                gain_db: f64::from(gain_db),
                phase_degrees: phase.map(|values| f64::from(values[index])),
            },
        )
        .collect();
    secs_audio::MicCalibration::from_points(points).ok()
}

pub(crate) fn has_valid_meter_calibration(info: &secs_mdat::SourceInfo) -> bool {
    secs_project::attached_calibration(info)
        .meter
        .and_then(mic_curve)
        .is_some()
}

fn curve_arrays_present(cal: &secs_mdat::CalInfo) -> bool {
    matches!(
        (&cal.freq, &cal.gain),
        (Some(frequencies), Some(gains))
            if frequencies.len() >= 2 && frequencies.len() == gains.len()
    )
}

fn response_correction(info: &secs_mdat::SourceInfo) -> (secs_audio::ResponseCorrection, bool) {
    let attached = secs_project::attached_calibration(info);
    let meter = attached.meter.and_then(mic_curve);
    let invalid_meter = info
        .cal
        .iter()
        .find(|(name, cal)| name == "meterCal" && curve_arrays_present(cal))
        .is_some_and(|(_, cal)| mic_curve(cal).is_none());
    let mut invalid_other = info
        .cal
        .iter()
        .filter(|(name, cal)| name == "scCal" && curve_arrays_present(cal))
        .any(|(_, cal)| mic_curve(cal).is_none());
    let others = attached
        .others
        .into_iter()
        .filter_map(|cal| {
            let curve = mic_curve(cal);
            invalid_other |= curve.is_none();
            curve
        })
        .collect();
    (
        secs_audio::ResponseCorrection::new(meter, others, attached.c_weighted && !invalid_meter),
        invalid_meter || invalid_other,
    )
}

fn measured_ir_with_curve_status(
    store: &mut secs_project::MeasurementStore,
    role: &str,
    pos: i64,
) -> Result<((Vec<f32>, u32), bool), crate::ServiceError> {
    let (samples, sample_rate) = store.ir_of(role, pos)?;
    let Some(info) = store.source_of(role, pos) else {
        return Ok(((samples, sample_rate), false));
    };
    let (correction, invalid_curve) = response_correction(&info);
    if correction.is_empty() {
        return Ok(((samples, sample_rate), invalid_curve));
    }
    let corrected = correction.apply(&samples, sample_rate).map_err(|error| {
        crate::ServiceError::ProjectMessage(format!("measurement response correction: {error}"))
    })?;
    Ok(((corrected, sample_rate), invalid_curve))
}

/// Loads a stored IR and removes the response curves attached to its source
/// sidecar without changing the stored waveform (ADR 0015).
pub(crate) fn measured_ir(
    store: &mut secs_project::MeasurementStore,
    role: &str,
    pos: i64,
) -> Result<(Vec<f32>, u32), crate::ServiceError> {
    measured_ir_with_curve_status(store, role, pos).map(|(measurement, _)| measurement)
}

pub(crate) fn load_measurements_cancellable(
    store: &mut secs_project::MeasurementStore,
    cancel: &secs_compute::CancelToken,
) -> Result<MeasurementSet, IpcError> {
    load_measurements_with_notices(store, cancel).map(|loaded| loaded.measurements)
}

fn load_measurements_with_notices(
    store: &mut secs_project::MeasurementStore,
    cancel: &secs_compute::CancelToken,
) -> Result<LoadedMeasurements, IpcError> {
    let mut positions = Vec::new();
    let mut notices = Vec::new();
    for record in store.records() {
        if record.pos < 0 {
            continue;
        }
        let mut roles = Vec::new();
        for role in record.ir.keys() {
            // The project reader bounds each WAV to MAX_IR_BYTES but reads it in
            // one call, so cancellation is checked between files.
            if cancel.is_cancelled() {
                return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
            }
            let ((samples, sample_rate), invalid_curve) =
                measured_ir_with_curve_status(store, role, record.pos)?;
            if invalid_curve {
                notices.push(Notice {
                    code: "calibration_curve_invalid".into(),
                    params: [
                        ("role".into(), role.clone()),
                        ("position".into(), record.pos.to_string()),
                    ]
                    .into(),
                });
            }
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
    if cancel.is_cancelled() {
        return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
    }
    positions.sort_by_key(|(position, _)| *position);
    Ok(LoadedMeasurements {
        measurements: MeasurementSet { positions },
        notices,
    })
}

fn summarize(
    design: &Design,
    settings: &CalibrationSettings,
    late_energy: &[secs_engine::RoleLateEnergy],
) -> CalibrationSummary {
    let scorecard: Vec<_> = design
        .scores
        .iter()
        .map(|scores| ChannelScore {
            role: scores.role.clone(),
            before: metrics(scores.before),
            after: metrics(scores.after),
        })
        .collect();
    let late_energy = late_energy
        .iter()
        .map(|row| LateEnergy {
            role: row.role.clone(),
            position: row.position,
            before_db: row.before_db,
            after_db: row.after_db,
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
        cached: false,
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
        late_energy,
        latency_ms: design.chain.latency_ms,
        notes: design
            .notes
            .iter()
            .flat_map(|note| {
                secs_engine::NoteRecords::record(note)
                    .into_iter()
                    .map(notice_from_record)
            })
            .collect(),
        main_roles: design
            .chain
            .mains
            .iter()
            .map(|main| main.role.clone())
            .collect(),
        sub_roles: design
            .chain
            .subs
            .iter()
            .map(|sub| sub.role.clone())
            .collect(),
    }
}

fn notice_from_record(record: secs_engine::NoteRecord) -> Notice {
    Notice {
        code: record.code.into(),
        params: record
            .params
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    }
}

// `curves.rs` is outside P188's allowed files and still calls this adapter.
pub(crate) fn scalar_notice(note: &secs_engine::Note) -> Notice {
    notice_from_record(
        secs_engine::NoteRecords::record(note)
            .into_iter()
            .next()
            .expect("scalar note records one notice"),
    )
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

/// The notice a calculation carries when its results stay in memory but
/// could not be written to the project folder (TD-38).
const CACHE_SAVE_FAILED: &str = "cache_save_failed";

/// Saves the cache after the results were committed in memory. A failure is
/// not a failed calculation: the owner's summary carries a
/// `cache_save_failed` notice until a later save succeeds (TD-38).
fn persist_cache(
    cached: Option<crate::cache::CachedResults>,
    path: &std::path::Path,
    ticket: ProjectTicket,
    session: &Arc<std::sync::Mutex<crate::session::Session>>,
    owner: CacheOwner,
) {
    let Some(cached) = cached else {
        return;
    };
    match crate::cache::save(path, &cached) {
        Ok(()) => {
            // Another project opened since the commit, or this one saved
            // under another path, keeps its own cache: only the in-memory
            // copy is skipped.
            let _ = session::apply(session, ticket, |open| {
                if open.path == path {
                    open.results.replace_cache(cached);
                    for notes in open.results.all_notes_mut() {
                        notes.retain(|notice| notice.code != CACHE_SAVE_FAILED);
                    }
                }
                Ok(((), false))
            });
        }
        Err(error) => {
            let _ = session::apply(session, ticket, |open| {
                if open.path == path
                    && let Some(notes) = open.results.owner_notes_mut(owner)
                {
                    notes.retain(|notice| notice.code != CACHE_SAVE_FAILED);
                    notes.push(Notice {
                        code: CACHE_SAVE_FAILED.into(),
                        params: [("error".into(), error.to_string())].into(),
                    });
                }
                Ok(((), false))
            });
        }
    }
}

pub(crate) fn config_key(project: &Project) -> String {
    format!("{:?}:{:?}", project.config, project.xo)
}

pub(crate) fn run_key(
    signature: &str,
    target_key: &str,
    engine_key: &str,
    config_key: &str,
) -> String {
    format!("{signature}\u{1f}{target_key}\u{1f}{engine_key}\u{1f}{config_key}")
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
    let base = format!("{:?}:{identities:?}", measurement_signature(project));
    let has_ir = |position: i64, role: &str| {
        match &project.measurements {
            secs_project::PyJson::Array(entries) => entries.as_slice(),
            _ => &[],
        }
        .iter()
        .any(|entry| {
            let matches_position = match entry.get("pos") {
                Some(secs_project::PyJson::Int(value) | secs_project::PyJson::Str(value)) => {
                    value.parse() == Ok(position)
                }
                Some(secs_project::PyJson::Float(value)) => {
                    value.is_finite() && value.trunc() == position as f64
                }
                Some(secs_project::PyJson::Bool(value)) => i64::from(*value) == position,
                _ => false,
            };
            matches_position
                    && entry
                        .get("ir")
                        .and_then(secs_project::PyJson::object)
                        .and_then(|values| values.iter().find(|(name, _)| name == role))
                        .is_some_and(|(_, value)| {
                            matches!(value, secs_project::PyJson::Str(path) if !path.is_empty())
                        })
        })
    };
    let calibration = project
        .measurement_calibration
        .iter()
        .filter_map(|((position, role), fingerprint)| {
            has_ir(*position, role).then_some((*position, role.as_str(), *fingerprint))
        })
        .collect::<Vec<_>>();
    if calibration.is_empty() {
        base
    } else {
        format!("{base}:cal{calibration:?}")
    }
}
pub(crate) fn target_key_string(project: &Project) -> String {
    format!("{:?}", target_key(project))
}

pub(crate) fn engine_error(error: secs_engine::EngineError) -> IpcError {
    use secs_engine::EngineError;
    let failure = IpcError::new(ErrorCode::EngineInput, error.to_string());
    match error {
        EngineError::Cancelled => IpcError::new(ErrorCode::Cancelled, "cancelled"),
        EngineError::MeasurementSampleRate {
            position,
            role,
            actual_hz,
            expected_hz,
        } => failure
            .with_param("reason", "measurement_sample_rate")
            .with_param("position", position.to_string())
            .with_param("role", role)
            .with_param("actual_hz", actual_hz.to_string())
            .with_param("expected_hz", expected_hz.to_string()),
        EngineError::UnsupportedResamplingRate {
            position,
            role,
            actual_hz,
        } => failure
            .with_param("reason", "unsupported_resampling_rate")
            .with_param("position", position.to_string())
            .with_param("role", role)
            .with_param("actual_hz", actual_hz.to_string())
            .with_param("min_hz", secs_engine::MIN_RESAMPLE_RATE.to_string())
            .with_param("max_hz", secs_engine::MAX_RESAMPLE_RATE.to_string()),
        EngineError::InvalidMeasurement { position, role } => failure
            .with_param("reason", "invalid_measurement")
            .with_param("position", position.to_string())
            .with_param("role", role),
        EngineError::DuplicateMeasurement { position, role } => failure
            .with_param("reason", "duplicate_measurement")
            .with_param("position", position.to_string())
            .with_param("role", role),
        EngineError::MissingReferencePosition { position } => failure
            .with_param("reason", "missing_reference_position")
            .with_param("position", position.to_string()),
        EngineError::MissingMainMeasurement { position } => failure
            .with_param("reason", "missing_main_measurement")
            .with_param("position", position.to_string()),
        EngineError::Dsp(detail) => {
            IpcError::new(ErrorCode::Internal, detail.to_string()).with_param("reason", "dsp")
        }
        EngineError::Memory { requested, total } => failure
            .with_param("reason", "memory_budget")
            .with_param("requested", requested.to_string())
            .with_param("total", total.to_string()),
        EngineError::Solver(detail) => {
            IpcError::new(ErrorCode::Internal, detail).with_param("reason", "solver")
        }
        EngineError::MissingSubMeasurement { position, role } => failure
            .with_param("reason", "missing_sub_measurement")
            .with_param("position", position.to_string())
            .with_param("role", role),
        EngineError::Input(_) => failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_mdat::{CalInfo, SourceInfo};
    use secs_project::{MeasurementResult, MeasurementStore};

    #[test]
    fn background_thread_selection_halves_parallelism_and_paces_one_cpu() {
        for (available, expected) in [
            (1, (1, true)),
            (2, (1, false)),
            (3, (1, false)),
            (8, (4, false)),
        ] {
            assert_eq!(background_threads(available, true), expected);
            assert_eq!(background_threads(available, false), (available, false));
        }
    }

    #[test]
    fn rc_agreement_counts_measured_mains_and_respects_manual_upper_limit() {
        let project = Project::from_preset("2.0").unwrap();
        let mut settings = CalibrationSettings {
            rc_hi_basis: secs_types::calibration::RcHiBasis::Agreement,
            ..Default::default()
        };
        let measurement = Measurement {
            samples: Samples::F64(vec![1.0].into()),
            sample_rate: 48000,
            t0: 0.0,
        };
        let mut measurements = MeasurementSet {
            positions: vec![
                (1, vec![("L".into(), measurement.clone())]),
                (2, vec![("L".into(), measurement.clone())]),
                (3, vec![("SW1".into(), measurement.clone())]),
            ],
        };
        let error = check_rc_agreement(&project, &settings, &measurements).unwrap_err();
        assert_eq!(error.code, ErrorCode::EngineInput);
        assert_eq!(error.params["reason"], "rc_agreement_positions");
        assert_eq!(error.params["measured"], "2");
        settings.rc_hi = Some(900.0);
        assert!(check_rc_agreement(&project, &settings, &measurements).is_ok());
        settings.rc_hi = None;
        measurements.positions[2].1.push(("L".into(), measurement));
        assert!(check_rc_agreement(&project, &settings, &measurements).is_ok());
    }

    fn response_source(frequencies: Vec<f32>, gains: Vec<f32>) -> SourceInfo {
        SourceInfo {
            cal: vec![(
                "meterCal".into(),
                CalInfo {
                    inverse_c_selected: false,
                    source_name: "mic.cal".into(),
                    source_path: "mic.cal".into(),
                    freq: Some(frequencies),
                    gain: Some(gains),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..SourceInfo::default()
        }
    }

    fn stored_result(
        role: &str,
        position: i64,
        samples: Vec<f32>,
        source_info: Option<SourceInfo>,
    ) -> MeasurementResult {
        MeasurementResult {
            role: role.into(),
            pos: position,
            ir: samples,
            sample_rate: 48_000,
            t0_s: 0.0,
            spl_db: None,
            spl_offset_db: None,
            snr_db: None,
            uuid: None,
            source: None,
            source_info,
        }
    }

    #[test]
    fn attached_calibration_is_removed_before_the_engine() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "secs-service-calibration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("room.secs");
        let mut store = MeasurementStore::new(&path, Project::default());
        let raw = vec![0.25, -0.5, 0.125, 0.75];
        store
            .record(
                stored_result(
                    "L",
                    1,
                    raw.clone(),
                    Some(response_source(vec![20.0, 20_000.0], vec![6.0, 6.0])),
                ),
                None,
            )
            .unwrap();
        let (corrected, rate) = measured_ir(&mut store, "L", 1).unwrap();
        assert_eq!(rate, 48_000);
        let scale = 10_f64.powf(-6.0 / 20.0);
        for (&actual, &input) in corrected.iter().zip(&raw) {
            let expected = f64::from(input) * scale;
            assert!(
                (f64::from(actual) - expected).abs() <= expected.abs() * 1e-6 + 1e-8,
                "{actual} != {expected}"
            );
        }

        store
            .record(stored_result("R", 1, raw.clone(), None), None)
            .unwrap();
        let untouched = measured_ir(&mut store, "R", 1).unwrap().0;
        assert_eq!(
            untouched
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            raw.iter().map(|value| value.to_bits()).collect::<Vec<_>>()
        );

        store
            .record(
                stored_result(
                    "L",
                    2,
                    raw.clone(),
                    Some(response_source(vec![100.0, 20.0], vec![6.0, 6.0])),
                ),
                None,
            )
            .unwrap();
        let invalid = measured_ir(&mut store, "L", 2).unwrap().0;
        assert_eq!(
            invalid
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            raw.iter().map(|value| value.to_bits()).collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn engine_measurement_errors_preserve_context_and_diagnostic_details() {
        use secs_engine::EngineError;
        use serde_json::json;
        let cases = [
            (
                EngineError::MeasurementSampleRate {
                    position: 3,
                    role: "L".into(),
                    actual_hz: 44100,
                    expected_hz: 48000,
                },
                json!({"reason":"measurement_sample_rate","position":"3","role":"L","actual_hz":"44100","expected_hz":"48000"}),
                "invalid input: position 3 L: sample rate 44100 (expected 48000)",
            ),
            (
                EngineError::InvalidMeasurement {
                    position: 2,
                    role: "SW1".into(),
                },
                json!({"reason":"invalid_measurement","position":"2","role":"SW1"}),
                "invalid input: position 2 SW1: empty or nonfinite IR",
            ),
            (
                EngineError::DuplicateMeasurement {
                    position: 4,
                    role: "R".into(),
                },
                json!({"reason":"duplicate_measurement","position":"4","role":"R"}),
                "invalid input: position 4: duplicate role R",
            ),
            (
                EngineError::MissingReferencePosition { position: 7 },
                json!({"reason":"missing_reference_position","position":"7"}),
                "invalid input: missing main position",
            ),
            (
                EngineError::MissingSubMeasurement {
                    position: 7,
                    role: "SW2".into(),
                },
                json!({"reason":"missing_sub_measurement","position":"7","role":"SW2"}),
                "invalid input: position 7: missing sub measurement SW2",
            ),
            (
                EngineError::MissingMainMeasurement { position: 5 },
                json!({"reason":"missing_main_measurement","position":"5"}),
                "invalid input: no measured main",
            ),
        ];
        for (error, params, detail) in cases {
            let error = engine_error(error);
            assert_eq!(error.code, ErrorCode::EngineInput);
            assert_eq!(serde_json::to_value(error.params).unwrap(), params);
            assert_eq!(error.detail, detail);
            assert!(!error.retryable);
        }
        // A similar-looking legacy string must not acquire an invented reason.
        let legacy = engine_error(EngineError::Input(
            "position 3 L: sample rate 44100 (expected 48000)".into(),
        ));
        assert_eq!(legacy.code, ErrorCode::EngineInput);
        assert!(legacy.params.is_empty());
        assert_eq!(
            legacy.detail,
            "invalid input: position 3 L: sample rate 44100 (expected 48000)"
        );
        let dsp = engine_error(EngineError::Dsp(secs_dsp::DspError::InvalidArgument(
            "invalid FFT length".into(),
        )));
        assert_eq!(dsp.code, ErrorCode::Internal);
        assert_eq!(dsp.detail, "invalid FFT length");
        assert_eq!(
            serde_json::to_value(dsp.params).unwrap(),
            json!({"reason":"dsp"})
        );
        let memory = engine_error(EngineError::Memory {
            requested: 2048,
            total: 1024,
        });
        assert_eq!(memory.code, ErrorCode::EngineInput);
        assert_eq!(
            serde_json::to_value(memory.params).unwrap(),
            json!({"reason":"memory_budget","requested":"2048","total":"1024"})
        );
        let solver = engine_error(EngineError::Solver("singular ridge system".into()));
        assert_eq!(solver.code, ErrorCode::Internal);
        assert_eq!(solver.detail, "singular ridge system");
        assert_eq!(
            serde_json::to_value(solver.params).unwrap(),
            json!({"reason":"solver"})
        );
        let cancelled = engine_error(EngineError::Cancelled);
        assert_eq!(cancelled.code, ErrorCode::Cancelled);
        assert_eq!(cancelled.detail, "cancelled");
        assert!(cancelled.params.is_empty());
    }

    #[test]
    fn unsupported_resampling_rate_has_a_distinct_actionable_reason() {
        let error = engine_error(secs_engine::EngineError::UnsupportedResamplingRate {
            position: 4,
            role: "SW2".into(),
            actual_hz: 1000,
        });
        assert_eq!(error.code, ErrorCode::EngineInput);
        assert_eq!(
            serde_json::to_value(error.params).unwrap(),
            serde_json::json!({
                "reason": "unsupported_resampling_rate", "position": "4", "role": "SW2",
                "actual_hz": "1000", "min_hz": "8000", "max_hz": "384000"
            })
        );
    }

    #[test]
    fn xo_sample_rate_failure_reaches_ipc_with_the_affected_measurement() {
        let project = Project::from_preset("2.1").unwrap();
        let groups = project_groups(&project);
        let inverted = project.config.inverted_mains();
        let view = ProjectView {
            groups: &groups,
            project_xo: &project.xo,
            target: &project.target,
            inverted: &inverted,
        };
        let measurements = MeasurementSet {
            positions: vec![(
                3,
                ["L", "R", "SW1"]
                    .into_iter()
                    .map(|role| {
                        (
                            role.into(),
                            Measurement {
                                samples: Samples::F64(vec![1.0].into()),
                                sample_rate: if role == "L" { 44100 } else { 48000 },
                                t0: 0.0,
                            },
                        )
                    })
                    .collect(),
            )],
        };
        let error = secs_engine::xo::recommend_xo(
            &view,
            &measurements,
            &CalibrationSettings::default(),
            &Tuning::default(),
            &secs_engine::xo::XO_CANDIDATES,
            &Context::serial(),
        )
        .map_err(engine_error)
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::EngineInput);
        assert_eq!(
            serde_json::to_value(error.params).unwrap(),
            serde_json::json!({
                "reason": "measurement_sample_rate", "position": "3", "role": "L",
                "actual_hz": "44100", "expected_hz": "48000",
            })
        );
        assert_eq!(
            error.detail,
            "invalid input: position 3 L: sample rate 44100 (expected 48000)"
        );
    }
}

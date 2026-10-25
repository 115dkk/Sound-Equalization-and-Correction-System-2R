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
    JobStarted, MetricsSummary, MimoParams, MimoSummary, Notice, ResultsSummary, VerdictSummary,
    XoRecommendation,
};

use crate::jobs::JobContext;
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
pub(crate) struct Prepared {
    pub ticket: ProjectTicket,
    pub project: Project,
    pub project_settings: CalibrationSettings,
    pub settings: CalibrationSettings,
    pub measurements: MeasurementSet,
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
        let snapshot = self.snapshot(params.settings)?;
        let subs_only = params.subs_only;
        let session = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        self.jobs.start(JobKind::Calibrate, events, move |job| {
            let prepared = snapshot.load(&job)?;
            let ctx = compute_context(&job, memory);
            let groups = project_groups(&prepared.project);
            let view = ProjectView {
                groups: &groups,
                project_xo: &prepared.project.xo,
                target: &prepared.project.target,
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
            let design = calibration.design;
            let ticket = prepared.ticket;
            let (path, cached) = session::commit(&session, ticket, &job, |open| {
                let mut summary = summarize(&design, &prepared.settings);
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
                let run_key = run_key(
                    &prepared.signature,
                    &prepared.target_key,
                    &prepared.settings.engine_key(),
                    &prepared.config_key,
                );
                let run = CalibrationRun::new(
                    summary,
                    prepared.settings.clone(),
                    prepared.signature,
                    prepared.target_key,
                    prepared.config_key,
                    run_key,
                    Arc::new(design),
                );
                open.results.commit_calibration(run, subs_only);
                let stores_settings =
                    calibration_from_project(open.store.project()) == prepared.project_settings;
                if stores_settings {
                    store_calibration(open.store.project_mut(), &prepared.settings);
                }
                Ok((
                    (open.path.clone(), open.results.cache_snapshot()),
                    stores_settings,
                ))
            })?;
            let owner = if subs_only {
                CacheOwner::Subs
            } else {
                CacheOwner::Calibration
            };
            persist_cache(cached, &path, ticket, &session, owner);
            Ok(JobResult::Calibration)
        })
    }

    pub(crate) fn start_xo(&self, events: Arc<dyn EventSink>) -> Result<JobStarted, IpcError> {
        let snapshot = self.snapshot(None)?;
        let memory = self.deps.memory.available_bytes();
        let session = self.session.clone();
        self.jobs.start(JobKind::RecommendXo, events, move |job| {
            let prepared = snapshot.load(&job)?;
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
            let recommendation = XoRecommendation {
                best: result.best,
                candidates: result.candidates,
            };
            session::commit(&session, prepared.ticket, &job, |open| {
                let mut settings = calibration_from_project(open.store.project());
                for (group, hz) in &recommendation.best {
                    match settings.xo.iter_mut().find(|(name, _)| name == group) {
                        Some((_, current)) => *current = *hz,
                        None => settings.xo.push((group.clone(), *hz)),
                    }
                }
                settings.xo_rec = recommendation.best.clone();
                store_calibration(open.store.project_mut(), &settings);
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
        let compute_key = crate::compute::ComputeIdentity::from(&selection.status);
        let session_guard = lock(&self.session);
        let open = session_guard.require().map_err(IpcError::from)?;
        let ticket = session_guard.ticket()?;
        let settings = params
            .settings
            .unwrap_or_else(|| calibration_from_project(open.store.project()));
        let calibration = open.results.live_calibration().ok_or_else(|| {
            IpcError::new(ErrorCode::EngineInput, "calibration must run before MIMO")
        })?;
        if calibration_stale(calibration, open.store.project()) {
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
            let ctx = compute_context(&job, memory);
            let run = secs_engine::mimo::compute_with_acceleration(
                &design,
                design.replay.measurements(),
                design.replay.config(),
                &settings,
                &Tuning::default(),
                &ctx,
                &selection,
            )
            .map_err(engine_error)?;
            let (path, cached) = session::commit(&session, ticket, &job, |open| {
                let summary = MimoSummary {
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
                        .flat_map(notices)
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
                    compute: run.compute.clone(),
                };
                open.results.commit_mimo(MimoResult {
                    summary,
                    settings_key: settings.mimo_key(),
                    compute_key,
                    calibration_key,
                    run,
                });
                Ok(((open.path.clone(), open.results.cache_snapshot()), false))
            })?;
            persist_cache(cached, &path, ticket, &session, CacheOwner::Mimo);
            Ok(JobResult::Mimo)
        })
    }

    pub(crate) fn results(&self) -> Result<ResultsSummary, IpcError> {
        let compute = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        Ok(open.results.summary(open.store.project(), &compute))
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
        let project = open.store.project().clone();
        let project_settings = calibration_from_project(&project);
        let settings = override_settings.unwrap_or_else(|| project_settings.clone());
        Ok(Snapshot {
            ticket: session.ticket()?,
            path: open.path.clone(),
            project,
            project_settings,
            settings,
        })
    }
}

impl Snapshot {
    /// Reads every IR of the snapshot without the session lock, checking for
    /// cancellation between files. The commit checks the ticket.
    pub(crate) fn load(self, job: &JobContext) -> Result<Prepared, IpcError> {
        let mut store = secs_project::MeasurementStore::new(&self.path, self.project.clone());
        let measurements = load_measurements_cancellable(&mut store, job.cancel())?;
        let project = self.project;
        Ok(Prepared {
            ticket: self.ticket,
            signature: signature_key(&project),
            target_key: target_key_string(&project),
            config_key: config_key(&project),
            project,
            project_settings: self.project_settings,
            settings: self.settings,
            measurements,
        })
    }
}

pub(crate) fn compute_context(job: &JobContext, available: u64) -> Context {
    let threads = std::thread::available_parallelism().map_or(1, |value| value.get());
    Context::new(
        threads,
        MemoryBudget::from_probe(&FixedMemory(available), 0.5, 64 * 1024 * 1024),
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
    load_measurements_cancellable(store, &secs_compute::CancelToken::new())
}

fn load_measurements_cancellable(
    store: &mut secs_project::MeasurementStore,
    cancel: &secs_compute::CancelToken,
) -> Result<MeasurementSet, IpcError> {
    let mut positions = Vec::new();
    for record in store.records() {
        if record.pos < 0 {
            continue;
        }
        let mut roles = Vec::new();
        for role in record.ir.keys() {
            if cancel.is_cancelled() {
                return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
            }
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
    if cancel.is_cancelled() {
        return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
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
        latency_ms: design.chain.latency_ms,
        notes: design.notes.iter().flat_map(notices).collect(),
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
    format!("{:?}:{identities:?}", measurement_signature(project))
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
        EngineError::Input(_) | EngineError::Dsp(_) => failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_engine::Note;
    use secs_engine::SubFilter;
    use secs_types::chain::BandType;

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
        let view = ProjectView {
            groups: &groups,
            project_xo: &project.xo,
            target: &project.target,
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

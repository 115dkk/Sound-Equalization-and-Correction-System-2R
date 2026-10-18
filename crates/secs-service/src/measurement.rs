use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use secs_project::{Project, PyJson};
use secs_types::ipc::SampleLoadParams;
use secs_types::ipc::*;

use crate::measure::json::JsonExt;
use crate::measure::{self, MeasureError, MeasurementSettings, Measurer, plan_for, store_devices};
use crate::{Deps, EventSink, Service, lock, mdat_import, session};

struct Transport(Box<dyn secs_rew::Transport>);
impl secs_rew::Transport for Transport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        self.0.request(request)
    }
}

fn input(field: &str, detail: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::MeasurementInput, detail).with_param("field", field)
}

/// The single measurement/library error boundary. Pinned format rejection never falls back.
pub(crate) fn measure_error(
    error: MeasureError,
    field: &str,
    share: secs_audio::SharePreference,
    rew_license: Option<&Arc<std::sync::Mutex<Option<bool>>>>,
) -> IpcError {
    use secs_audio::AudioError as A;
    use secs_rew::RewError as R;
    let detail = error.to_string();
    match error {
        MeasureError::Input(_) => input(field, detail),
        MeasureError::Project(_) => IpcError::new(ErrorCode::ProjectIo, detail),
        MeasureError::Audio(A::Cancelled) | MeasureError::Rew(R::Cancelled) => {
            IpcError::new(ErrorCode::Cancelled, detail)
        }
        MeasureError::Audio(A::DeviceNotFound(device)) => {
            IpcError::new(ErrorCode::DeviceUnavailable, detail).with_param("device", device)
        }
        MeasureError::Audio(A::Backend(_)) => {
            IpcError::new(ErrorCode::DeviceUnavailable, detail).with_param("device", "backend")
        }
        MeasureError::Audio(A::Recording { issue, .. }) => {
            IpcError::new(ErrorCode::RecordingFailed, detail)
                .with_param("reason", issue.to_string())
        }
        MeasureError::Audio(A::UnsupportedFormat(_))
            if share != secs_audio::SharePreference::Auto =>
        {
            IpcError::new(ErrorCode::ShareModeRefused, detail)
                .with_param("share_mode", share_name(share))
        }
        MeasureError::Audio(A::UnsupportedFormat(_)) => input(field, detail),
        MeasureError::Rew(R::Connection { path, .. }) => {
            IpcError::new(ErrorCode::RewUnreachable, detail).with_param("path", path)
        }
        MeasureError::Rew(R::ProRequired { status, path }) => {
            if let Some(rew_license) = rew_license {
                *lock(rew_license) = Some(false);
            }
            IpcError::new(ErrorCode::RewProRequired, detail)
                .with_param("path", path)
                .with_param("status", status.to_string())
        }
        MeasureError::Rew(R::Http { status, path, .. }) => {
            IpcError::new(ErrorCode::RewFailed, detail)
                .with_param("path", path)
                .with_param("status", status.to_string())
        }
        MeasureError::Rew(R::Schema { path, .. }) => {
            IpcError::new(ErrorCode::RewFailed, detail).with_param("path", path)
        }
    }
}
fn share_name(share: secs_audio::SharePreference) -> &'static str {
    match share {
        secs_audio::SharePreference::Auto => "auto",
        secs_audio::SharePreference::Exclusive => "exclusive",
        secs_audio::SharePreference::Shared => "shared",
    }
}
fn start_delay(project: &Project) -> u64 {
    project
        .extra
        .iter()
        .find(|(key, _)| key == "start_delay")
        .and_then(|(_, value)| value.str())
        .and_then(|value| u64::try_from(secs_project::start_delay_seconds(value)).ok())
        .unwrap_or(0)
}

fn method(project: &Project) -> &str {
    project
        .extra
        .iter()
        .find(|(k, _)| k == "devices")
        .and_then(|(_, v)| v.get("method"))
        .and_then(JsonExt::str)
        .unwrap_or("native")
}
const LENGTHS: [(usize, &str); 7] = [
    (65536, "64k"),
    (131072, "128k"),
    (262144, "256k"),
    (524288, "512k"),
    (1048576, "1M"),
    (2097152, "2M"),
    (4194304, "4M"),
];
fn settings(project: &Project) -> Result<MeasureConfigResult, IpcError> {
    let s = MeasurementSettings::from_project(project)
        .map_err(|e| measure_error(e, "settings", secs_audio::SharePreference::Auto, None))?;
    let p = plan_for(project, &s);
    let channel = |n: i64| {
        u16::try_from(n)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| input("channel", "channel must be in 1..65535"))
    };
    Ok(MeasureConfigResult {
        method: method(project).into(),
        sweep_level_dbfs: Some(p.sweep_level_db),
        input_device: p.input_device.clone(),
        output_device: p.output_device.clone(),
        input_channel: channel(p.input_channel)?,
        roles: p.roles.clone(),
        output_channels: p
            .roles
            .iter()
            .map(|r| channel(p.output_for(r)))
            .collect::<Result<_, _>>()?,
        sweep_length: LENGTHS
            .iter()
            .find(|(_, name)| *name == p.sweep_length)
            .map(|(n, _)| *n)
            .ok_or_else(|| input("sweep_length", "unknown sweep length"))?,
        repetitions: u8::try_from(p.repetitions)
            .map_err(|_| input("repetitions", "invalid repetition count"))?,
        sample_rate: u32::try_from(p.sample_rate)
            .map_err(|_| input("sample_rate", "invalid sample rate"))?,
        sweep_lengths: LENGTHS.iter().map(|(n, _)| *n).collect(),
        timing: p.timing_reference.as_disk_str().into(),
        loopback_input_channel: s.loopback_input_channel,
        timing_ref_output: p.timing_ref_output.map(channel).transpose()?,
        share_mode: share_name(s.share).into(),
    })
}
fn measurer(
    deps: &Deps,
    method: &str,
    settings: MeasurementSettings,
) -> Result<Box<dyn Measurer>, IpcError> {
    match method {
        "native" => Ok(Box::new(measure::NativeMeasurer::from_backend(
            (deps.audio_backend)(),
            settings,
        ))),
        "mock" => Ok(Box::new((deps.mock_measurer)())),
        "rew" => Ok(Box::new(measure::RewMeasurer::new(
            secs_rew::RewClient::new(Transport((deps.rew_transport)())),
            deps.unix_ms.clone(),
        ))),
        _ => Err(input("method", "unknown measurement method")),
    }
}
fn rew_status(deps: &Deps, rew_license: &Arc<std::sync::Mutex<Option<bool>>>) -> RewStatusResult {
    let running = deps
        .process
        .process_names()
        .unwrap_or_default()
        .iter()
        .any(|name| {
            secs_rew::launch::REW_PROCESS_NAMES
                .iter()
                .any(|r| name.eq_ignore_ascii_case(r))
        });
    let mut client = secs_rew::RewClient::new(Transport((deps.rew_transport)()));
    let reachable = client.is_running();
    if !reachable {
        *lock(rew_license) = None;
    }
    RewStatusResult {
        running: running || reachable,
        reachable,
        pro: reachable.then(|| *lock(rew_license)).flatten(),
    }
}

pub(crate) fn measure_methods_for(
    deps: &Deps,
    rew_license: &Arc<std::sync::Mutex<Option<bool>>>,
) -> MeasureMethodsResult {
    let mut native =
        measure::NativeMeasurer::from_backend((deps.audio_backend)(), Default::default());
    let (native_devices, native_error) = match native.devices() {
        Ok(devices) => (
            devices
                .into_iter()
                .map(|d| MeasureDevice {
                    id: d.id,
                    name: d.name,
                    input_channels: d.input_channels,
                    output_channels: d.output_channels,
                })
                .collect::<Vec<_>>(),
            None,
        ),
        Err(e) => (
            vec![],
            Some(measure_error(
                e,
                "devices",
                secs_audio::SharePreference::Auto,
                Some(rew_license),
            )),
        ),
    };
    let rew = rew_status(deps, rew_license);
    let mut methods = vec!["mock".into()];
    if native_devices.iter().any(|d| d.input_channels > 0)
        && native_devices.iter().any(|d| d.output_channels > 0)
    {
        methods.push("native".into());
    }
    if rew.reachable && rew.pro != Some(false) {
        methods.push("rew".into());
    }
    MeasureMethodsResult {
        methods,
        native_devices,
        native_error,
        rew,
        mock_positions: measure::sample::index().positions.clone(),
        mock_roles: measure::sample::index()
            .entries
            .iter()
            .map(|e| e.role.clone())
            .collect(),
    }
}

impl Service {
    pub(crate) fn measure_methods(&self) -> MeasureMethodsResult {
        measure_methods_for(&self.deps, &self.rew_license)
    }

    pub(crate) fn measure_config_get(&self) -> Result<MeasureConfigResult, IpcError> {
        let session = lock(&self.session);
        settings(session.require()?.store.project())
    }

    pub(crate) fn measure_configure(
        &self,
        params: MeasureConfigureParams,
    ) -> Result<MeasureConfigResult, IpcError> {
        if !["none", "native", "rew", "mock", "mdat"].contains(&params.method.as_str()) {
            return Err(input("method", "unknown measurement method"));
        }
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let mut project = open.store.project().clone();
        let mut values = vec![("method".into(), PyJson::Str(params.method))];
        // REW owns its live audio selection. These stored values remain for
        // native measurement and project compatibility; they are not applied to REW.
        for (key, value) in [
            ("input", params.input_device),
            ("output", params.output_device),
        ] {
            if let Some(value) = value {
                values.push((key.into(), PyJson::Str(value)));
            }
        }
        for (key, value) in [
            ("input_channel", params.input_channel),
            ("loopback_input_channel", params.loopback_input_channel),
            ("timing_ref_output", params.timing_ref_output),
        ] {
            if let Some(value) = value {
                if value == 0 {
                    return Err(input(key, "channel must be positive"));
                }
                values.push((key.into(), PyJson::Int(value.to_string())));
            }
        }
        if let Some(length) = params.sweep_length {
            values.push((
                "sweep_length".into(),
                PyJson::Str(
                    LENGTHS
                        .iter()
                        .find(|(n, _)| *n == length)
                        .ok_or_else(|| input("sweep_length", "unsupported sweep length"))?
                        .1
                        .into(),
                ),
            ));
        }
        if let Some(repetitions) = params.repetitions {
            if ![1, 2, 4, 8].contains(&repetitions) {
                return Err(input("repetitions", "unsupported repetition count"));
            }
            values.push(("repetitions".into(), PyJson::Int(repetitions.to_string())));
        }
        if let Some(level) = params.sweep_level_dbfs {
            if !level.is_finite() || !(-120.0..=0.0).contains(&level) {
                return Err(input("sweep_level_dbfs", "level outside -120..0 dBFS"));
            }
            values.push(("sweep_level_db".into(), PyJson::Float(level)));
        }
        if let Some(timing) = params.timing {
            if !["none", "loopback", "acoustic"].contains(&timing.as_str()) {
                return Err(input("timing", "unknown timing reference"));
            }
            values.push(("timing_reference".into(), PyJson::Str(timing)));
        }
        if let Some(share) = params.share_mode {
            if !["auto", "exclusive", "shared"].contains(&share.as_str()) {
                return Err(input("share_mode", "unknown share mode"));
            }
            values.push(("share".into(), PyJson::Str(share)));
        }
        store_devices(&mut project, values);
        if plan_for(&project, &Default::default()).timing_reference
            == secs_project::TimingReference::Acoustic
        {
            store_devices(
                &mut project,
                vec![("repetitions".into(), PyJson::Int("1".into()))],
            );
        }
        if !params.output_channels.is_empty() {
            let plan = plan_for(&project, &Default::default());
            if params.output_channels.len() != plan.roles.len()
                || params.output_channels.contains(&0)
            {
                return Err(input(
                    "output_channels",
                    "one positive output per role is required",
                ));
            }
            let outputs: BTreeMap<_, _> = plan.roles.iter().zip(&params.output_channels).collect();
            for speaker in &mut project.config.speakers {
                let multiple = speaker.bands.len() > 1;
                for band in &mut speaker.bands {
                    let role = if multiple {
                        format!("{}:{}", speaker.role, band.band)
                    } else {
                        speaker.role.clone()
                    };
                    band.out = i64::from(
                        **outputs
                            .get(&role)
                            .ok_or_else(|| input("output_channels", "unknown role"))?,
                    );
                }
            }
        }
        let result = settings(&project)?;
        if result.timing == "loopback"
            && result.method != "rew"
            && result.loopback_input_channel.is_none()
        {
            return Err(input(
                "loopback_input_channel",
                "loopback requires an input channel",
            ));
        }
        *open.store.project_mut() = project;
        open.dirty = true;
        Ok(result)
    }
    pub(crate) fn measurement_records(&self) -> Result<MeasurementRecordsResult, IpcError> {
        let session = lock(&self.session);
        let open = session.require()?;
        let project = open.store.project();
        let planned = plan_for(project, &Default::default()).roles;
        let records = open.store.records();
        Ok(MeasurementRecordsResult {
            records: (1..=measure::position_count(project))
                .map(|position| {
                    let record = records.iter().find(|record| record.pos == position);
                    let roles = planned
                        .iter()
                        .filter(|role| record.is_some_and(|r| r.ir.contains_key(*role)))
                        .cloned()
                        .collect::<Vec<_>>();
                    let sources = roles
                        .iter()
                        .map(|role| {
                            let value = record
                                .and_then(|r| r.source.get(role))
                                .filter(|value| !value.is_empty())
                                .cloned()
                                .unwrap_or_else(|| "measured".into());
                            (role.clone(), value)
                        })
                        .collect();
                    PositionRecord {
                        position,
                        done: roles.len() == planned.len() && !planned.is_empty(),
                        roles,
                        sources,
                    }
                })
                .collect(),
        })
    }

    pub(crate) fn level_apply(
        &self,
        params: LevelApplyParams,
    ) -> Result<MeasureConfigResult, IpcError> {
        let level = params.level;
        if !level.rms_dbfs.is_finite()
            || !level.peak_dbfs.is_finite()
            || level.spl_db.is_some_and(|v| !v.is_finite())
            || !level.recommended_sweep_dbfs.is_finite()
            || !(-120.0..=0.0).contains(&level.recommended_sweep_dbfs)
        {
            return Err(input("level", "invalid level reading"));
        }
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        // Recover the played level from the recommendation so applying a result
        // is idempotent, even if the caller repeats the same level_apply request.
        let (low, high) = secs_audio::level::TARGET_WINDOW;
        let target = (low + high) / 2.0;
        let played = level.recommended_sweep_dbfs - (target - level.rms_dbfs);
        measure::apply_level(
            open.store.project_mut(),
            secs_audio::LevelReport {
                input_dbfs: level.rms_dbfs,
                peak_dbfs: level.peak_dbfs,
                clip: level.peak_dbfs >= 0.0,
                spl_db: level.spl_db,
            },
            played,
        )
        .map_err(|e| measure_error(e, "level", secs_audio::SharePreference::Auto, None))?;
        open.dirty = true;
        settings(open.store.project())
    }
    pub(crate) fn start_level(
        &self,
        params: LevelCheckParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let (project, _) = {
            let session = lock(&self.session);
            let open = session.require()?;
            (open.store.project().clone(), open.identity)
        };
        let s = MeasurementSettings::from_project(&project).map_err(|e| {
            measure_error(e, "settings", secs_audio::SharePreference::Auto, None)
        })?;
        let p = plan_for(&project, &s);
        if !p.roles.contains(&params.role) {
            return Err(input("role", "unknown measurement role"));
        }
        let share = s.share;
        let rew = method(&project) == "rew";
        let rew_license = self.rew_license.clone();
        let mut m = measurer(&self.deps, method(&project), s)?;
        self.jobs.start(JobKind::LevelCheck, events, move |ctx| {
            ctx.role_progress(&params.role, 0, 1);
            let report = m
                .level_check(&p, &params.role, 0.5, ctx.measurement_cancel())
                .map_err(|e| {
                    measure_error(e, "level", share, rew.then_some(&rew_license))
                })?;
            let recommended = secs_audio::recommend_sweep_level(
                report.input_dbfs,
                p.sweep_level_db,
                secs_audio::level::TARGET_WINDOW,
            )
            .map_err(|e| measure_error(e.into(), "level", share, None))?;
            ctx.role_progress(&params.role, 1, 1);
            Ok(JobResult::Level(LevelResult {
                rms_dbfs: report.input_dbfs,
                peak_dbfs: report.peak_dbfs,
                spl_db: report.spl_db,
                recommended_sweep_dbfs: recommended,
            }))
        })
    }
    pub(crate) fn start_sample_load(
        &self,
        params: SampleLoadParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let (roles, identity) = {
            let session = lock(&self.session);
            let open = session.require()?;
            let roles = plan_for(open.store.project(), &Default::default()).roles;
            (roles, open.identity)
        };
        let positions = match params.positions {
            None => vec![1],
            Some(count) if count >= 2 => (1..=count).map(i64::from).collect(),
            Some(_) => return Err(input("positions", "positions must be at least 2")),
        };
        let embedded = params.positions.is_none();
        let session = self.session.clone();
        self.jobs.start(JobKind::SampleLoad, events, move |ctx| {
            ctx.begin_commit()?;
            let total = (roles.len() * positions.len()) as u64;
            let mut done = 0;
            let mut session = lock(&session);
            let open = session.require_mut()?;
            if open.identity != identity {
                return Err(IpcError::new(
                    ErrorCode::StaleInputs,
                    "open project changed",
                ));
            }
            if embedded {
                let existing = secs_project::measured_positions(open.store.project());
                for position in existing {
                    if position != 1 {
                        open.store.clear_position(position);
                    }
                }
            } else {
                let existing = secs_project::measured_positions(open.store.project());
                for position in existing {
                    open.store.clear_position(position);
                }
            }
            for position in positions {
                open.store.start_position(position);
                for role in &roles {
                    if ctx.cancel().is_cancelled() {
                        return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
                    }
                    let (ir, sample_rate, t0_s, source) = if embedded {
                        match measure::sample::ir(role, 1) {
                            Ok((ir, rate, t0)) => (ir, rate, t0, "sample"),
                            Err(_) => {
                                (measure::sample::fallback_ir(role), 48_000, 0.0, "synthetic")
                            }
                        }
                    } else {
                        (
                            measure::sample::position_variation(
                                &measure::sample::fallback_ir(role),
                                u32::try_from(position).unwrap_or(u32::MAX),
                            ),
                            48_000,
                            0.0,
                            "synthetic",
                        )
                    };
                    open.store
                        .record(
                            secs_project::MeasurementResult {
                                role: role.clone(),
                                pos: position,
                                ir,
                                sample_rate,
                                t0_s,
                                spl_db: None,
                                spl_offset_db: None,
                                snr_db: None,
                                uuid: Some(format!("sample-{role}-{position}")),
                                source: Some(source.into()),
                                source_info: None,
                            },
                            Some(&roles),
                        )
                        .map_err(|error| {
                            measure_error(
                                error.into(),
                                "record",
                                secs_audio::SharePreference::Auto,
                                None,
                            )
                        })?;
                    done += 1;
                    ctx.role_progress(role, done, total);
                }
            }
            open.dirty = false;
            Ok(JobResult::Measurement(session::info(open)))
        })
    }

    pub(crate) fn start_measure(
        &self,
        params: MeasurePositionParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let (project, identity) = {
            let session = lock(&self.session);
            let open = session.require()?;
            (open.store.project().clone(), open.identity)
        };
        let active_method = method(&project);
        if matches!(active_method, "none" | "mdat") {
            return Err(input("method", "selected method cannot run a measurement"));
        }
        let mut s = MeasurementSettings::from_project(&project).map_err(|e| {
            measure_error(e, "settings", secs_audio::SharePreference::Auto, None)
        })?;
        if params.position < 1 || params.position > measure::position_count(&project) {
            return Err(input("position", "position outside layout"));
        }
        s.positions = Some(vec![params.position]);
        let p = plan_for(&project, &s);
        if let Some(error) = p.validate().first() {
            return Err(input("plan", format!("{error:?}")));
        }
        let share = s.share;
        let delay = start_delay(&project);
        let rew = method(&project) == "rew";
        let rew_license = self.rew_license.clone();
        let mut m = measurer(&self.deps, method(&project), s)?;
        let session = self.session.clone();
        let uuid = self.deps.uuid.clone();
        let atomic_position = method(&project) != "rew";
        self.jobs.start(JobKind::Measure, events, move |ctx| {
            let countdown_started = std::time::Instant::now();
            let mut seconds_passed = 0;
            while seconds_passed < delay {
                if ctx.cancel().is_cancelled() {
                    return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
                }
                let next = std::time::Duration::from_secs(seconds_passed + 1);
                let remaining = next.saturating_sub(countdown_started.elapsed());
                if !remaining.is_zero() {
                    std::thread::sleep(remaining.min(std::time::Duration::from_millis(10)));
                    continue;
                }
                seconds_passed += 1;
                ctx.report_progress("countdown", seconds_passed, delay);
            }
            if ctx.cancel().is_cancelled() {
                return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
            }
            let mut count = 0;
            let mut failure = None;
            m.measure_recording(
                &p,
                params.position,
                ctx.measurement_cancel(),
                &mut |mut result| {
                    let recorded = (|| -> Result<(), IpcError> {
                        if result.uuid.is_none() {
                            result.uuid = Some(format!(
                                "{:032x}",
                                lock(&uuid).as_mut()().map_err(|e| IpcError::new(
                                    ErrorCode::Internal,
                                    e.to_string()
                                ))?
                            ));
                        }
                        let mut session = lock(&session);
                        let open = session.require_mut()?;
                        if open.identity != identity {
                            return Err(IpcError::new(
                                ErrorCode::StaleInputs,
                                "open project changed",
                            ));
                        }
                        if count == 0 {
                            if atomic_position {
                                ctx.begin_commit()?;
                            }
                            open.store.start_position(params.position);
                        }
                        let role = result.role.clone();
                        let snr = result.snr_db;
                        open.store
                            .record(result, Some(&p.roles))
                            .map_err(|e| measure_error(e.into(), "record", share, None))?;
                        open.dirty = false;
                        count += 1;
                        drop(session);
                        ctx.role_progress(&role, count, p.roles.len() as u64);
                        if let Some(snr) = snr.filter(|v| *v < secs_rew::LOW_SNR_DB) {
                            ctx.log(Notice {
                                code: "measurement_low_snr".into(),
                                params: [
                                    ("role".into(), role),
                                    ("snr_db".into(), snr.to_string()),
                                    ("threshold_db".into(), secs_rew::LOW_SNR_DB.to_string()),
                                ]
                                .into(),
                            });
                        }
                        Ok(())
                    })();
                    if let Err(error) = recorded {
                        failure = Some(error);
                        return Err(MeasureError::Input("record failed".into()));
                    }
                    Ok(())
                },
            )
            .map_err(|e| {
                failure.unwrap_or_else(|| {
                    measure_error(e, "plan", share, rew.then_some(&rew_license))
                })
            })?;
            if rew {
                *lock(&rew_license) = Some(true);
            }
            if ctx.cancel().is_cancelled() {
                return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
            }
            let session = lock(&session);
            let open = session.require()?;
            if open.identity != identity {
                return Err(IpcError::new(
                    ErrorCode::StaleInputs,
                    "open project changed",
                ));
            }
            Ok(JobResult::Measurement(session::info(open)))
        })
    }
    pub(crate) fn position_clear(
        &self,
        params: PositionClearParams,
    ) -> Result<UndoResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let before = open.store.project().measurements.clone();
        let removed = match &before {
            PyJson::Array(entries) => entries
                .iter()
                .enumerate()
                .filter(|(_, v)| position_of(v) == Some(params.position))
                .map(|(i, v)| (i, v.clone()))
                .collect::<Vec<_>>(),
            _ => vec![],
        };
        if removed.is_empty() {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "position has no record",
            ));
        }
        open.store.clear_position(params.position);
        if let Err(error) = open.store.save() {
            open.store.project_mut().measurements = before;
            return Err(measure_error(
                error.into(),
                "position",
                secs_audio::SharePreference::Auto,
                None,
            ));
        }
        let token = session::push_position_undo(open, removed);
        open.dirty = false;
        Ok(UndoResult {
            token,
            project: session::info(open),
        })
    }
    pub(crate) fn undo(&self, params: UndoParams) -> Result<ProjectInfo, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let index = open
            .undo
            .iter()
            .position(|entry| entry.token == params.token)
            .ok_or_else(|| {
                IpcError::new(ErrorCode::InvalidRequest, "unknown or used undo token")
            })?;
        let payload = open.undo[index].payload.clone();
        match payload {
            session::UndoPayload::Position(removed) => {
                let before = open.store.project().measurements.clone();
                let PyJson::Array(entries) = &mut open.store.project_mut().measurements else {
                    return Err(IpcError::new(
                        ErrorCode::InvalidRequest,
                        "measurement list changed",
                    ));
                };
                if removed
                    .iter()
                    .any(|(_, v)| entries.iter().any(|e| position_of(e) == position_of(v)))
                {
                    return Err(IpcError::new(
                        ErrorCode::InvalidRequest,
                        "position was recorded again",
                    ));
                }
                for (index, entry) in removed {
                    entries.insert(index.min(entries.len()), entry);
                }
                if let Err(error) = open.store.save() {
                    open.store.project_mut().measurements = before;
                    return Err(measure_error(
                        error.into(),
                        "undo",
                        secs_audio::SharePreference::Auto,
                        None,
                    ));
                }
                open.dirty = false;
            }
            session::UndoPayload::Measurements(measurements) => {
                if !measurement_entries(open.store.project()).is_empty() {
                    return Err(IpcError::new(
                        ErrorCode::InvalidRequest,
                        "measurements changed after destructive edit",
                    ));
                }
                open.store.project_mut().measurements = measurements;
                open.dirty = true;
            }
        }
        open.undo.remove(index);
        Ok(session::info(open))
    }
    pub(crate) fn rew_info(&self) -> RewInfoResult {
        let mut client = secs_rew::RewClient::new(Transport((self.deps.rew_transport)()));
        let mut info = client.info();
        let current_audio = info.reachable.then(|| client.current_audio()).unwrap_or_default();
        if !info.reachable {
            *lock(&self.rew_license) = None;
        }
        let executable_path =
            secs_rew::launch::find_rew_exe().map(|p| p.to_string_lossy().into_owned());
        info.running |= self
            .deps
            .process
            .process_names()
            .unwrap_or_default()
            .iter()
            .any(|name| {
                secs_rew::launch::REW_PROCESS_NAMES
                    .iter()
                    .any(|rew| name.eq_ignore_ascii_case(rew))
            });
        let devices = |items: Vec<secs_rew::DeviceInfo>| {
            items
                .into_iter()
                .map(|d| RewDeviceInfo {
                    id: d.id,
                    name: d.name,
                    channels: d.channels,
                })
                .collect()
        };
        RewInfoResult {
            executable_path,
            running: info.running,
            reachable: info.reachable,
            version: info.version,
            pro: info.reachable.then(|| *lock(&self.rew_license)).flatten(),
            input_device: current_audio.input_device,
            output_device: current_audio.output_device,
            input: current_audio.input,
            blocking: info.blocking,
            input_calibration_file: info.input_calibration_file,
            input_devices: info.input_devices.map(devices),
            output_devices: info.output_devices.map(devices),
            input_channel_names: info.input_channel_names,
            sample_rate: info.sample_rate,
            sweep: info.sweep.map(|s| RewSweepInfo {
                start_hz: s.start_hz,
                end_hz: s.end_hz,
                length: s.length,
            }),
            level_dbfs: info.level_dbfs,
        }
    }

    pub(crate) fn rew_launch(&self) -> Result<RewStatusResult, IpcError> {
        *lock(&self.rew_license) = None;
        let status = rew_status(&self.deps, &self.rew_license);
        if status.running {
            return Ok(status);
        }
        self.deps.host.launch_rew().map_err(|error| {
            let reason = match &error {
                crate::RewLaunchError::ExeNotFound(_) => "exe_not_found",
                crate::RewLaunchError::CannotLaunch(_) => "cannot_launch",
                crate::RewLaunchError::LaunchFailed(_) => "launch_failed",
            };
            IpcError::new(ErrorCode::RewFailed, error.to_string()).with_param("reason", reason)
        })?;
        Ok(RewStatusResult {
            running: true,
            reachable: false,
            pro: None,
        })
    }
    pub(crate) fn mdat_scan(&self, params: MdatScanParams) -> Result<MdatScanResult, IpcError> {
        let scan = mdat_import::scan(Path::new(&params.path))
            .map_err(|e| measure_error(e, "path", secs_audio::SharePreference::Auto, None))?;
        let roles = scan
            .measurements
            .iter()
            .map(|m| m.role.clone())
            .collect::<Vec<_>>();
        let positions = scan
            .measurements
            .iter()
            .map(|m| m.position)
            .collect::<Vec<_>>();
        Ok(MdatScanResult {
            suggestions: scan
                .measurements
                .iter()
                .enumerate()
                .filter_map(|(index, m)| {
                    m.role.as_ref().map(|role| MdatAssignment {
                        measurement: m.info.title.clone(),
                        role: role.clone(),
                        index: Some(index),
                        position: Some(m.position),
                    })
                })
                .collect(),
            problems: mdat_import::problem_notices(&roles, &positions),
            measurements: scan
                .measurements
                .into_iter()
                .enumerate()
                .map(|(index, m)| MdatScanMeasurement {
                    index,
                    position_tagged: mdat_import::split_position(&m.info.title).0
                        != m.info.title.trim(),
                    title: m.info.title,
                    role: m.role,
                    position: m.position,
                    looks_like_sub: m.looks_like_sub,
                })
                .collect(),
        })
    }
    pub(crate) fn start_mdat(
        &self,
        params: MdatImportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let (project, identity) = {
            let session = lock(&self.session);
            let open = session.require()?;
            (open.store.project().clone(), open.identity)
        };
        let session = self.session.clone();
        self.jobs.start(JobKind::MdatImport, events, move |ctx| {
            let scan = mdat_import::scan(Path::new(&params.path))
                .map_err(|e| measure_error(e, "path", secs_audio::SharePreference::Auto, None))?;
            let plan = plan_for(&project, &Default::default());
            let mut assignments = vec![
                mdat_import::Assignment {
                    role: None,
                    position: None
                };
                scan.measurements.len()
            ];
            for a in params.assignments {
                let matches = scan
                    .measurements
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.info.title == a.measurement)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let index = a
                    .index
                    .or_else(|| {
                        if matches.len() == 1 {
                            Some(matches[0])
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| input("assignments", "title missing or ambiguous"))?;
                if index >= assignments.len()
                    || scan.measurements[index].info.title != a.measurement
                    || assignments[index].role.is_some()
                    || !plan.roles.contains(&a.role)
                {
                    return Err(input("assignments", "invalid row or role"));
                }
                let position = a.position.unwrap_or(params.position);
                if !(1..=measure::position_count(&project)).contains(&position) {
                    return Err(input("position", "position outside layout"));
                }
                assignments[index] = mdat_import::Assignment {
                    role: Some(a.role),
                    position: Some(position),
                };
            }
            let problems = mdat_import::problem_notices(
                &assignments
                    .iter()
                    .map(|a| a.role.clone())
                    .collect::<Vec<_>>(),
                &assignments
                    .iter()
                    .map(|a| a.position.unwrap_or(params.position))
                    .collect::<Vec<_>>(),
            );
            if let Some(problem) = problems.first() {
                let mut error = input("assignments", &problem.code);
                error.params.extend(problem.params.clone());
                return Err(error);
            }
            let mut session = lock(&session);
            let open = session.require_mut()?;
            if open.identity != identity {
                return Err(IpcError::new(
                    ErrorCode::StaleInputs,
                    "open project changed",
                ));
            }
            ctx.begin_commit()?;
            let count = mdat_import::import(
                &mut open.store,
                Path::new(&params.path),
                Some(&assignments),
                Some(params.position),
            )
            .map_err(|e| measure_error(e, "path", secs_audio::SharePreference::Auto, None))?;
            open.dirty = false;
            let info = session::info(open);
            drop(session);
            ctx.report_progress("mdat_import", count as u64, count as u64);
            Ok(JobResult::MdatImport(info))
        })
    }
}
fn measurement_entries(project: &Project) -> &[PyJson] {
    match &project.measurements {
        PyJson::Array(entries) => entries,
        _ => &[],
    }
}

fn position_of(value: &PyJson) -> Option<i64> {
    match value.get("pos") {
        Some(PyJson::Int(v) | PyJson::Str(v)) => v.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measurement_error_mapping_covers_every_variant() {
        use secs_audio::{AudioError as A, SharePreference as S};
        use secs_rew::RewError as R;
        let cases = [
            (
                MeasureError::Input("bad".into()),
                S::Auto,
                ErrorCode::MeasurementInput,
                "field",
                "plan",
            ),
            (
                A::DeviceNotFound("gone".into()).into(),
                S::Auto,
                ErrorCode::DeviceUnavailable,
                "device",
                "gone",
            ),
            (
                A::Backend("gone".into()).into(),
                S::Auto,
                ErrorCode::DeviceUnavailable,
                "device",
                "backend",
            ),
            (
                A::Recording {
                    issue: secs_audio::RecordingIssue::Gap,
                    detail: "lost samples".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RecordingFailed,
                "reason",
                "gap",
            ),
            (
                A::Recording {
                    issue: secs_audio::RecordingIssue::Underrun,
                    detail: "late render".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RecordingFailed,
                "reason",
                "underrun",
            ),
            (
                A::Recording {
                    issue: secs_audio::RecordingIssue::NonFinite,
                    detail: "bad sample".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RecordingFailed,
                "reason",
                "nonfinite",
            ),
            (
                A::UnsupportedFormat("bad".into()).into(),
                S::Auto,
                ErrorCode::MeasurementInput,
                "field",
                "plan",
            ),
            (
                A::UnsupportedFormat("refused".into()).into(),
                S::Exclusive,
                ErrorCode::ShareModeRefused,
                "share_mode",
                "exclusive",
            ),
            (
                A::UnsupportedFormat("refused".into()).into(),
                S::Shared,
                ErrorCode::ShareModeRefused,
                "share_mode",
                "shared",
            ),
            (
                R::Connection {
                    path: "/version".into(),
                    message: "offline".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RewUnreachable,
                "path",
                "/version",
            ),
            (
                R::Http {
                    path: "/measure".into(),
                    status: 500,
                    message: "failure".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RewFailed,
                "status",
                "500",
            ),
            (
                R::Schema {
                    path: "/ir".into(),
                    message: "bad".into(),
                }
                .into(),
                S::Auto,
                ErrorCode::RewFailed,
                "path",
                "/ir",
            ),
            (
                R::ProRequired {
                    path: "/measure/command".into(),
                    status: 403,
                }
                .into(),
                S::Auto,
                ErrorCode::RewProRequired,
                "status",
                "403",
            ),
        ];
        for (error, share, code, key, value) in cases {
            let got = measure_error(error, "plan", share, None);
            assert_eq!(got.code, code);
            assert_eq!(got.params[key], value);
        }
        for error in [
            MeasureError::Audio(A::Cancelled),
            MeasureError::Rew(R::Cancelled),
        ] {
            assert_eq!(
                measure_error(error, "plan", S::Auto, None).code,
                ErrorCode::Cancelled
            );
        }
        let error = secs_project::Project::load_text("not json").unwrap_err();
        assert_eq!(
            measure_error(error.into(), "project", S::Auto, None).code,
            ErrorCode::ProjectIo
        );
    }
}

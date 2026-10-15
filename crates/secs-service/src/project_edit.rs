use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use secs_project::{
    DspProfile, Project, PyJson, SpeakerConfig, TargetCurve, TargetJson, calibration_from_project,
    dumps, loads, position_seconds, store_calibration,
};
use secs_types::calibration::{CalibrationSettings, mode_params};
use secs_types::ipc::*;
use secs_types::role::{role_group, role_label};
use serde::{Deserialize, Serialize};

use crate::measure::json::JsonExt;
use crate::measure::store_devices;
use crate::{Service, lock, session};

const FILE_LIMIT: u64 = 1024 * 1024;
const APP_SETTINGS_VERSION: u32 = 1;
const ARRANGEMENTS: [usize; 4] = [1, 9, 13, 17];
const CONFIG_GROUP_ORDER: [&str; 9] = [
    "프론트",
    "센터",
    "서브",
    "서라운드",
    "백",
    "와이드",
    "탑",
    "하이트",
    "기타",
];

fn group_id(key: &str) -> &'static str {
    match key {
        "프론트" => "front",
        "센터" => "center",
        "서브" => "sub",
        "서라운드" => "surround",
        "백" => "back",
        "와이드" => "wide",
        "탑" => "top",
        "하이트" => "height",
        _ => "other",
    }
}

fn invalid(field: &str, detail: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::InvalidRequest, detail).with_param("field", field)
}

fn project_io(path: &Path, error: impl ToString) -> IpcError {
    IpcError::new(ErrorCode::ProjectIo, error.to_string())
        .with_param("path", path.to_string_lossy())
}

fn measurement_count(project: &Project) -> usize {
    match &project.measurements {
        PyJson::Array(entries) => entries.len(),
        _ => 0,
    }
}

fn confirm_required(project: &Project) -> IpcError {
    IpcError::new(
        ErrorCode::ConfirmRequired,
        "configuration change clears measurements",
    )
    .with_param("measurements", measurement_count(project).to_string())
}

fn clear_measurements(open: &mut session::OpenProject) -> Option<String> {
    let count = measurement_count(open.store.project());
    if count == 0 {
        return None;
    }
    let previous = std::mem::replace(
        &mut open.store.project_mut().measurements,
        PyJson::Array(Vec::new()),
    );
    Some(session::push_measurements_undo(open, previous))
}

fn config_result(project: &Project) -> ConfigResult {
    let config = &project.config;
    let speakers = config
        .speakers
        .iter()
        .map(|speaker| ConfigSpeaker {
            role: speaker.role.clone(),
            size: speaker.size.clone(),
            bands: speaker
                .bands
                .iter()
                .map(|band| ConfigBand {
                    band: band.band.clone(),
                    out: band.out,
                    pol: band.pol.clone(),
                })
                .collect(),
        })
        .collect();
    let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, speaker) in config.speakers.iter().enumerate() {
        let key = match role_group(&speaker.role) {
            "sub" => "서브",
            key => key,
        }
        .to_owned();
        if let Some((_, members)) = grouped.iter_mut().find(|(name, _)| *name == key) {
            members.push(index);
        } else {
            grouped.push((key, vec![index]));
        }
    }
    let mut groups = Vec::new();
    for key in CONFIG_GROUP_ORDER {
        if let Some((_, speaker_indices)) = grouped.iter().find(|(name, _)| name == key) {
            groups.push(ConfigGroup {
                id: group_id(key).into(),
                key: key.to_owned(),
                speaker_indices: speaker_indices.clone(),
            });
        }
    }
    for (key, speaker_indices) in grouped {
        if !CONFIG_GROUP_ORDER.contains(&key.as_str()) {
            groups.push(ConfigGroup {
                id: "other".into(),
                key,
                speaker_indices,
            });
        }
    }
    let outputs: Vec<_> = config
        .speakers
        .iter()
        .flat_map(|speaker| speaker.bands.iter().map(|band| band.out))
        .collect();
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for output in &outputs {
        if !seen.insert(*output) {
            duplicates.insert(*output);
        }
    }
    ConfigResult {
        speakers,
        preset: config.preset.clone(),
        device: config.device.clone(),
        groups,
        summary: ConfigSummary {
            speaker_count: config.speakers.len(),
            max_channel: outputs.iter().copied().max().unwrap_or(0),
            outputs,
            duplicate_outputs: duplicates.into_iter().collect(),
            sub_count: config.sub_count(),
        },
    }
}

fn dsp_fields(profile: &DspProfile) -> DspFields {
    DspFields {
        fir_taps: profile.fir_taps,
        fir_budget: profile.fir_budget,
        iir_bands: profile.iir_bands,
        allpass_bands: profile.allpass_bands,
        bm_path_delays: profile.bm_path_delays,
    }
}

fn dsp_result(project: &Project) -> DspResult {
    let fields = dsp_fields(&project.dsp);
    let taps = project.dsp.fir_taps;
    DspResult {
        profile: project.dsp.profile.clone(),
        name: project.dsp.name.clone(),
        fields,
        presets: DspProfile::presets()
            .into_iter()
            .map(|(key, preset)| DspPreset {
                key: key.into(),
                name: preset.name.clone(),
                fields: dsp_fields(&preset),
            })
            .collect(),
        window_ms: taps as f64 / 48_000.0 * 1000.0,
        resolution_hz: 48_000.0 / taps.max(1) as f64,
    }
}

fn layout_points(count: usize) -> Vec<LayoutPoint> {
    const RING1: [(f64, f64, i8); 8] = [
        (35.0, 0.0, 0),
        (-35.0, 0.0, 0),
        (0.0, 35.0, 0),
        (0.0, -30.0, 0),
        (25.0, 25.0, 1),
        (-25.0, 25.0, 1),
        (25.0, -20.0, -1),
        (-25.0, -20.0, -1),
    ];
    const RING2: [(f64, f64, i8); 4] = [
        (70.0, 0.0, 0),
        (-70.0, 0.0, 0),
        (0.0, 70.0, 1),
        (0.0, -55.0, 0),
    ];
    const RING3: [(f64, f64, i8); 4] = [
        (55.0, 50.0, 0),
        (-55.0, 50.0, 0),
        (55.0, -50.0, -1),
        (-55.0, -50.0, -1),
    ];
    let mut raw = vec![(0.0, 0.0, 0)];
    raw.extend(RING1);
    if count >= 13 {
        raw.extend(RING2);
    }
    if count >= 17 {
        raw.extend(RING3);
    }
    raw.into_iter()
        .take(count)
        .enumerate()
        .map(|(index, (x_cm, y_cm, height))| LayoutPoint {
            position: index + 1,
            x_cm,
            y_cm,
            height,
        })
        .collect()
}

fn extra<'a>(project: &'a Project, key: &str) -> Option<&'a PyJson> {
    project
        .extra
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

fn string(value: Option<&PyJson>, default: &str) -> String {
    match value {
        Some(PyJson::Str(value)) => value.clone(),
        Some(PyJson::Int(value)) => value.clone(),
        _ => default.into(),
    }
}

fn integer(value: Option<&PyJson>, default: i64) -> i64 {
    match value {
        Some(PyJson::Int(value) | PyJson::Str(value)) => value.parse().unwrap_or(default),
        Some(PyJson::Float(value)) if value.is_finite() => *value as i64,
        _ => default,
    }
}

fn layout_result(project: &Project) -> LayoutResult {
    let devices = extra(project, "devices").and_then(PyJson::object);
    let get = |key: &str| {
        devices.and_then(|values| {
            values
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        })
    };
    let count = integer(extra(project, "array"), 9) as usize;
    let stored_role = string(get("timing_ref_role"), "");
    let roles = secs_project::measurement_roles(project);
    let role = if roles.iter().any(|(role, _)| role == &stored_role) {
        stored_role
    } else {
        secs_project::default_ref_role(project)
    };
    LayoutResult {
        count: if ARRANGEMENTS.contains(&count) {
            count
        } else {
            9
        },
        start_delay: string(extra(project, "start_delay"), "끄기"),
        timing: LayoutTiming {
            reference: string(get("timing_reference"), "acoustic"),
            role,
            output: integer(get("timing_ref_output"), 1),
        },
        arrangements: ARRANGEMENTS
            .into_iter()
            .map(|count| LayoutArrangement {
                count,
                points: layout_points(count),
            })
            .collect(),
    }
}

fn set_extra(project: &mut Project, key: &str, value: PyJson) {
    if let Some((_, old)) = project.extra.iter_mut().find(|(name, _)| name == key) {
        *old = value;
    } else {
        project.extra.push((key.into(), value));
    }
}

fn target_preset(project: &Project) -> Option<String> {
    project
        .bc
        .get("target_preset")
        .and_then(PyJson::str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn set_target_preset(project: &mut Project, preset: Option<&str>) {
    if !matches!(project.bc, PyJson::Object(_)) {
        project.bc = PyJson::Object(Vec::new());
    }
    let PyJson::Object(values) = &mut project.bc else {
        unreachable!()
    };
    let value = PyJson::Str(preset.unwrap_or("").into());
    if let Some((_, old)) = values.iter_mut().find(|(name, _)| name == "target_preset") {
        *old = value;
    } else {
        values.push(("target_preset".into(), value));
    }
}

fn target_result(project: &Project) -> TargetResult {
    TargetResult {
        points: project.target.points.clone(),
        preset: target_preset(project),
        presets: TargetCurve::presets()
            .into_iter()
            .map(|(key, target)| (key.into(), target.points))
            .collect(),
    }
}

fn checked_target(points: Vec<(f64, f64)>) -> Result<TargetCurve, IpcError> {
    if points.len() < 2 {
        return Err(invalid("points", "at least two target points are required"));
    }
    if points
        .iter()
        .any(|(frequency, level)| !frequency.is_finite() || !level.is_finite())
    {
        return Err(invalid("points", "target points must be finite"));
    }
    Ok(TargetCurve::new(
        points
            .into_iter()
            .map(|(frequency, level)| (frequency.clamp(10.0, 24_000.0), level.clamp(-30.0, 30.0)))
            .collect(),
    ))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, IpcError> {
    let file = fs::File::open(path).map_err(|error| project_io(path, error))?;
    let length = file
        .metadata()
        .map_err(|error| project_io(path, error))?
        .len();
    if length > FILE_LIMIT {
        return Err(project_io(path, "file exceeds 1 MiB"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| project_io(path, error))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(project_io(path, "file exceeds 1 MiB"));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), IpcError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| project_io(path, error))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let temporary = parent.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(project_io(path, error));
    }
    Ok(())
}

impl Service {
    pub(crate) fn config_get(&self) -> Result<ConfigResult, IpcError> {
        let session = lock(&self.session);
        Ok(config_result(session.require()?.store.project()))
    }

    pub(crate) fn config_apply_preset(
        &self,
        params: ConfigPresetParams,
    ) -> Result<ConfigMutationResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        if measurement_count(open.store.project()) > 0 && !params.confirm {
            return Err(confirm_required(open.store.project()));
        }
        let current = &open.store.project().config;
        let config = if params.preset == "custom" {
            SpeakerConfig {
                speakers: Vec::new(),
                device: current.device.clone(),
                preset: "custom".into(),
                biamp_default: current.biamp_default,
            }
        } else {
            SpeakerConfig::from_preset(&params.preset, &current.device, current.biamp_default)
                .map_err(|error| invalid("preset", error.to_string()))?
        };
        let undo_token = clear_measurements(open);
        open.store.project_mut().config = config;
        open.dirty = true;
        Ok(ConfigMutationResult {
            config: config_result(open.store.project()),
            undo_token,
        })
    }

    pub(crate) fn config_set_sub_count(
        &self,
        params: ConfigSubCountParams,
    ) -> Result<ConfigMutationResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let requested = params.count.clamp(0, 8) as usize;
        let changed = open.store.project().config.sub_count() != requested;
        if changed && measurement_count(open.store.project()) > 0 && !params.confirm {
            return Err(confirm_required(open.store.project()));
        }
        let undo_token = if changed {
            clear_measurements(open)
        } else {
            None
        };
        open.store.project_mut().config.set_sub_count(params.count);
        open.dirty = true;
        Ok(ConfigMutationResult {
            config: config_result(open.store.project()),
            undo_token,
        })
    }

    pub(crate) fn config_set_speaker(
        &self,
        params: ConfigSetSpeakerParams,
    ) -> Result<ConfigResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store
            .project_mut()
            .config
            .set_speaker(
                params.index,
                params.role,
                params.size,
                params.band_index,
                params.out,
                params.pol,
            )
            .map_err(|error| invalid("speaker", error.to_string()))?;
        open.dirty = true;
        Ok(config_result(open.store.project()))
    }

    pub(crate) fn config_add_speaker(&self) -> Result<ConfigResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store
            .project_mut()
            .config
            .add_speaker()
            .map_err(|error| invalid("speaker", error.to_string()))?;
        open.dirty = true;
        Ok(config_result(open.store.project()))
    }

    pub(crate) fn config_remove_speaker(
        &self,
        params: ConfigIndexParams,
    ) -> Result<ConfigResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store
            .project_mut()
            .config
            .remove_speaker(params.index)
            .map_err(|error| invalid("index", error.to_string()))?;
        open.dirty = true;
        Ok(config_result(open.store.project()))
    }

    pub(crate) fn config_renumber(&self) -> Result<ConfigResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store.project_mut().config.renumber();
        open.dirty = true;
        Ok(config_result(open.store.project()))
    }

    pub(crate) fn config_validate(&self) -> Result<Vec<Notice>, IpcError> {
        let session = lock(&self.session);
        Ok(session
            .require()?
            .store
            .project()
            .config
            .validation_notices())
    }

    pub(crate) fn patch_sheet_csv(&self, params: PathParams) -> Result<PathResult, IpcError> {
        let session = lock(&self.session);
        let project = session.require()?.store.project();
        let mut text = String::from("역할,이름,크기,대역,측정 출력,극성\n");
        for speaker in &project.config.speakers {
            for band in &speaker.bands {
                text.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    speaker.role,
                    role_label(&speaker.role),
                    speaker.size,
                    band.band,
                    band.out,
                    band.pol
                ));
            }
        }
        let path = PathBuf::from(params.path);
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(text.as_bytes());
        atomic_write(&path, &bytes)?;
        Ok(PathResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub(crate) fn dsp_get(&self) -> Result<DspResult, IpcError> {
        let session = lock(&self.session);
        Ok(dsp_result(session.require()?.store.project()))
    }

    pub(crate) fn dsp_set(&self, params: DspSetParams) -> Result<DspResult, IpcError> {
        if params.preset.is_some() == params.fields.is_some() {
            return Err(invalid(
                "dsp",
                "exactly one of preset or fields is required",
            ));
        }
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        if let Some(preset) = params.preset {
            let mut presets = DspProfile::presets();
            open.store.project_mut().dsp = presets
                .remove(preset.as_str())
                .ok_or_else(|| invalid("preset", "unknown DSP preset"))?;
        } else if let Some(fields) = params.fields {
            if [
                fields.fir_taps,
                fields.fir_budget,
                fields.iir_bands,
                fields.allpass_bands,
                fields.bm_path_delays,
            ]
            .into_iter()
            .any(|value| value < 0)
            {
                return Err(invalid("fields", "DSP fields must be non-negative"));
            }
            let dsp = &mut open.store.project_mut().dsp;
            dsp.profile = "custom".into();
            dsp.fir_taps = fields.fir_taps;
            dsp.fir_budget = fields.fir_budget;
            dsp.iir_bands = fields.iir_bands;
            dsp.allpass_bands = fields.allpass_bands;
            dsp.bm_path_delays = fields.bm_path_delays;
        }
        open.dirty = true;
        Ok(dsp_result(open.store.project()))
    }

    pub(crate) fn layout_get(&self) -> Result<LayoutResult, IpcError> {
        let session = lock(&self.session);
        Ok(layout_result(session.require()?.store.project()))
    }

    pub(crate) fn layout_set(
        &self,
        params: LayoutSetParams,
    ) -> Result<LayoutMutationResult, IpcError> {
        if let Some(count) = params.count
            && !ARRANGEMENTS.contains(&count)
        {
            return Err(invalid("count", "arrangement must be 1, 9, 13, or 17"));
        }
        if let Some(delay) = params.start_delay.as_deref()
            && !["끄기", "5 초", "10 초", "15 초"].contains(&delay)
        {
            return Err(invalid("start_delay", "unknown start delay"));
        }
        if let Some(timing) = &params.timing {
            if !["acoustic", "loopback", "none"].contains(&timing.reference.as_str()) {
                return Err(invalid("timing.reference", "unknown timing reference"));
            }
            if timing.output < 1 {
                return Err(invalid("timing.output", "output must be positive"));
            }
        }
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let old_count = layout_result(open.store.project()).count;
        let arrangement_changed = params.count.is_some_and(|count| count != old_count);
        if arrangement_changed && measurement_count(open.store.project()) > 0 && !params.confirm {
            return Err(confirm_required(open.store.project()));
        }
        let undo_token = if arrangement_changed {
            clear_measurements(open)
        } else {
            None
        };
        let project = open.store.project_mut();
        if let Some(count) = params.count {
            set_extra(project, "array", PyJson::Int(count.to_string()));
        }
        if let Some(delay) = params.start_delay {
            set_extra(project, "start_delay", PyJson::Str(delay));
        }
        if let Some(timing) = params.timing {
            store_devices(
                project,
                vec![
                    ("timing_reference".into(), PyJson::Str(timing.reference)),
                    ("timing_ref_role".into(), PyJson::Str(timing.role)),
                    (
                        "timing_ref_output".into(),
                        PyJson::Int(timing.output.to_string()),
                    ),
                ],
            );
        }
        open.dirty = true;
        Ok(LayoutMutationResult {
            layout: layout_result(open.store.project()),
            undo_token,
        })
    }

    pub(crate) fn position_estimate(&self) -> Result<PositionEstimateResult, IpcError> {
        let session = lock(&self.session);
        let project = session.require()?.store.project();
        let positions = layout_result(project).count;
        let per_position_seconds = position_seconds(project);
        Ok(PositionEstimateResult {
            positions,
            per_position_seconds,
            total_seconds: positions as f64 * per_position_seconds,
        })
    }

    pub(crate) fn target_get(&self) -> Result<TargetResult, IpcError> {
        let session = lock(&self.session);
        Ok(target_result(session.require()?.store.project()))
    }

    pub(crate) fn target_set_points(
        &self,
        params: TargetPointsParams,
    ) -> Result<TargetResult, IpcError> {
        let target = checked_target(params.points)?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store.project_mut().target = target;
        set_target_preset(open.store.project_mut(), None);
        open.dirty = true;
        Ok(target_result(open.store.project()))
    }

    pub(crate) fn target_apply_preset(
        &self,
        params: TargetPresetParams,
    ) -> Result<TargetResult, IpcError> {
        let mut presets = TargetCurve::presets();
        let target = presets
            .remove(params.preset.as_str())
            .ok_or_else(|| invalid("preset", "unknown target preset"))?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store.project_mut().target = target;
        set_target_preset(open.store.project_mut(), Some(&params.preset));
        open.dirty = true;
        Ok(target_result(open.store.project()))
    }

    pub(crate) fn target_save(&self, params: PathParams) -> Result<PathResult, IpcError> {
        let session = lock(&self.session);
        let project = session.require()?.store.project();
        let path = PathBuf::from(params.path);
        let text = dumps(&project.target.to_json());
        atomic_write(&path, text.as_bytes())?;
        Ok(PathResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub(crate) fn target_load(&self, params: PathParams) -> Result<TargetResult, IpcError> {
        let path = PathBuf::from(params.path);
        let bytes = read_bounded(&path)?;
        let text = std::str::from_utf8(&bytes).map_err(|error| project_io(&path, error))?;
        let raw = loads(text).map_err(|error| project_io(&path, error))?;
        let parsed = TargetCurve::from_json(raw).map_err(|error| project_io(&path, error))?;
        let target = checked_target(parsed.points)?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        open.store.project_mut().target = target;
        set_target_preset(open.store.project_mut(), None);
        open.dirty = true;
        Ok(target_result(open.store.project()))
    }

    pub(crate) fn calibration_settings_get(&self) -> Result<CalibrationSettings, IpcError> {
        let session = lock(&self.session);
        Ok(calibration_from_project(session.require()?.store.project()))
    }

    pub(crate) fn calibration_settings_set(
        &self,
        params: CalibrationSettingsParams,
    ) -> Result<CalibrationSettings, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        store_calibration(open.store.project_mut(), &params.settings);
        open.dirty = true;
        Ok(calibration_from_project(open.store.project()))
    }

    pub(crate) fn mode_spec(&self, params: ModeSpecParams) -> ModeSpecResult {
        let values = mode_params(params.mode, params.ultra_ms.unwrap_or(200.0));
        ModeSpecResult {
            rc_filter: if values.taps == 0 {
                "iir".into()
            } else {
                "iir_fir".into()
            },
            fir_taps: values.taps,
            peq_bands: values.peq_bands,
            mimo_filter: "iir".into(),
            lift_ms: values.lift_ms,
            phase_frac: values.phase_frac,
            max_latency_ms: values.max_latency_ms,
        }
    }

    pub(crate) fn app_settings_get(&self) -> AppSettings {
        lock(&self.app_settings).clone()
    }

    pub(crate) fn app_settings_set(
        &self,
        mut params: AppSettingsParams,
    ) -> Result<AppSettings, IpcError> {
        params.settings.recent_projects.truncate(10);
        if let Some((width, height)) = params.settings.last_window_size
            && (width == 0 || height == 0)
        {
            return Err(invalid(
                "last_window_size",
                "window dimensions must be positive",
            ));
        }
        if let Some(path) = self.deps.app_settings_path.as_deref() {
            save_app_settings(path, &params.settings)?;
        }
        *lock(&self.app_settings) = params.settings.clone();
        Ok(params.settings)
    }
}

#[derive(Serialize, Deserialize)]
struct AppSettingsFile {
    version: u32,
    settings: AppSettings,
}

pub(crate) fn load_app_settings(path: Option<&Path>) -> AppSettings {
    let Some(path) = path else {
        return AppSettings::default();
    };
    let Ok(bytes) = read_bounded(path) else {
        return AppSettings::default();
    };
    let Ok(file) = serde_json::from_slice::<AppSettingsFile>(&bytes) else {
        return AppSettings::default();
    };
    if file.version != APP_SETTINGS_VERSION {
        return AppSettings::default();
    }
    let mut settings = file.settings;
    settings.recent_projects.truncate(10);
    settings
}

fn save_app_settings(path: &Path, settings: &AppSettings) -> Result<(), IpcError> {
    let bytes = serde_json::to_vec_pretty(&AppSettingsFile {
        version: APP_SETTINGS_VERSION,
        settings: settings.clone(),
    })
    .map_err(|error| project_io(path, error))?;
    atomic_write(path, &bytes)
}

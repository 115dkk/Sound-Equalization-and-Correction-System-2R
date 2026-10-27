// Port of wizard/step5_measure.py::plan and step1_devices.py::_store (SECS 2, GPL-3.0-only).
use super::json::JsonExt;
use super::{MeasureError, positive};
use secs_project::{MeasurementPlan, Project, PyJson, TimingReference};

/// Additional native options stay beside upstream device options, never in a
/// second service-owned settings file. REW's acoustic reference is not confused
/// with a native loopback input channel. The native path no longer reads that
/// channel (U15): the screen shows the timing reference only for REW, and a
/// native session already keeps every channel's arrival time on one clock.
#[derive(Debug)]
pub struct MeasurementSettings {
    pub positions: Option<Vec<i64>>,
    pub loopback_input_channel: Option<u16>,
    pub share: secs_audio::SharePreference,
    pub window: secs_audio::MeasurementWindow,
    pub calibration: Option<secs_audio::MicCalibration>,
    pub calibration_file: Option<secs_mdat::CalFile>,
    pub spl_reference: Option<secs_audio::SplReference>,
}
impl Clone for MeasurementSettings {
    fn clone(&self) -> Self {
        Self {
            positions: self.positions.clone(),
            loopback_input_channel: self.loopback_input_channel,
            share: self.share,
            window: self.window,
            calibration: self.calibration.clone(),
            calibration_file: self
                .calibration_file
                .as_ref()
                .map(|file| secs_mdat::CalFile {
                    path: file.path.clone(),
                    text: file.text.clone(),
                }),
            spl_reference: self.spl_reference,
        }
    }
}
impl Default for MeasurementSettings {
    fn default() -> Self {
        Self {
            positions: None,
            loopback_input_channel: None,
            share: secs_audio::SharePreference::Auto,
            window: Default::default(),
            calibration: None,
            calibration_file: None,
            spl_reference: None,
        }
    }
}
impl MeasurementSettings {
    pub fn from_project(project: &Project) -> Result<Self, MeasureError> {
        let mut settings = Self::default();
        let d = devices(project);
        let get = |key| d.and_then(|d| d.get(key));
        settings.loopback_input_channel = get("loopback_input_channel")
            .map(|v| number(Some(v), 0.0) as i64)
            .map(|v| {
                u16::try_from(positive(v, "loopback input")?)
                    .map_err(|_| MeasureError::Input("loopback input exceeds u16".into()))
            })
            .transpose()?;
        settings.share = match get("share").and_then(PyJson::str) {
            Some("exclusive") => secs_audio::SharePreference::Exclusive,
            Some("shared") => secs_audio::SharePreference::Shared,
            _ => secs_audio::SharePreference::Auto,
        };
        let w = get("window");
        let n = |key, default| number(w.and_then(|w| w.get(key)), default);
        settings.window.gap_seconds = n("gap_seconds", settings.window.gap_seconds);
        settings.window.pre_roll_seconds = n("pre_roll_seconds", settings.window.pre_roll_seconds);
        settings.window.post_onset_seconds =
            n("post_onset_seconds", settings.window.post_onset_seconds);
        settings.window.max_latency_seconds =
            n("max_latency_seconds", settings.window.max_latency_seconds);
        settings.window.noise_seconds = n("noise_seconds", settings.window.noise_seconds);
        if let Some(text) = get("mic_calibration").and_then(PyJson::str) {
            settings.calibration = Some(secs_audio::MicCalibration::parse(text)?);
            let path = get("mic_calibration_path")
                .and_then(PyJson::str)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    get("mic_calibration_name")
                        .and_then(PyJson::str)
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or("mic calibration")
                .to_owned();
            settings.calibration_file = Some(secs_mdat::CalFile {
                path,
                text: Some(text.as_bytes().to_vec()),
            });
        }
        if let Some(v) = get("spl_offset_db") {
            let offset_db = number(Some(v), f64::NAN);
            if !offset_db.is_finite() {
                return Err(MeasureError::Input("invalid SPL offset".into()));
            }
            settings.spl_reference = Some(secs_audio::SplReference { offset_db });
        }
        Ok(settings)
    }
}
fn devices(project: &Project) -> Option<&PyJson> {
    project
        .extra
        .iter()
        .find(|(k, _)| k == "devices")
        .map(|(_, v)| v)
}
fn number(value: Option<&PyJson>, default: f64) -> f64 {
    match value {
        Some(PyJson::Float(v)) => *v,
        Some(PyJson::Int(v) | PyJson::Str(v)) => v.parse().unwrap_or(default),
        _ => default,
    }
}
fn string(value: Option<&PyJson>, default: &str) -> String {
    value
        .and_then(PyJson::str)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .into()
}
pub fn position_count(project: &Project) -> i64 {
    let value = project
        .extra
        .iter()
        .find(|(k, _)| k == "array")
        .map(|(_, v)| v);
    let count = number(value, 9.0) as i64;
    if [1, 9, 13, 17].contains(&count) {
        count
    } else {
        9
    }
}
pub fn plan_for(project: &Project, settings: &MeasurementSettings) -> MeasurementPlan {
    let mut plan = secs_project::plan_from_project(
        project,
        settings
            .positions
            .clone()
            .unwrap_or_else(|| (1..=position_count(project)).collect()),
    );
    // step5 includes a speaker with no bands on output 1 as well.
    plan.roles.clear();
    plan.outputs.clear();
    for speaker in secs_project::ordered_speakers(&project.config.speakers, true) {
        if speaker.bands.len() <= 1 {
            plan.roles.push(speaker.role.clone());
            plan.outputs.insert(
                speaker.role.clone(),
                speaker.bands.first().map_or(1, |b| b.out),
            );
        } else {
            for b in &speaker.bands {
                let role = format!("{}:{}", speaker.role, b.band);
                plan.roles.push(role.clone());
                plan.outputs.insert(role, b.out);
            }
        }
    }
    let d = devices(project);
    let get = |key| d.and_then(|d| d.get(key));
    plan.output_device = string(get("output"), &project.config.device);
    plan.input_device = string(get("input"), "");
    plan.input_channel = number(get("input_channel"), 1.0) as i64;
    if plan.input_channel == 0 {
        plan.input_channel = 1;
    }
    plan.sweep_length = string(get("sweep_length"), "256k");
    (plan.timing_reference, plan.timing_ref_output) = secs_project::timing_settings(project);
    let repeats = number(get("repetitions"), 1.0) as i64;
    plan.repetitions =
        if plan.timing_reference == TimingReference::Acoustic || ![1, 2, 4, 8].contains(&repeats) {
            1
        } else {
            repeats
        };
    // U1 connects the previously disconnected level control. Rate/frequency
    // options are applied here so native and REW receive the same values.
    plan.sweep_level_db = number(get("sweep_level_db"), -20.0);
    plan.sample_rate = number(get("sample_rate"), 48_000.0) as i64;
    plan.start_frequency = number(get("start_frequency"), 20.0);
    plan.end_frequency = number(get("end_frequency"), 20_000.0);
    plan
}
/// Merge device keys without removing timing or unknown upstream settings.
pub fn store_devices(project: &mut Project, values: Vec<(String, PyJson)>) {
    let index = project
        .extra
        .iter()
        .position(|(k, _)| k == "devices")
        .unwrap_or_else(|| {
            project
                .extra
                .push(("devices".into(), PyJson::Object(vec![])));
            project.extra.len() - 1
        });
    if project.extra[index].1.object().is_none() {
        project.extra[index].1 = PyJson::Object(vec![]);
    }
    let object = project.extra[index].1.object_mut().unwrap();
    for (key, value) in values {
        if key == "output"
            && let Some(s) = value.str()
            && !s.is_empty()
        {
            project.config.device = s.into();
        }
        if let Some((_, old)) = object.iter_mut().find(|(k, _)| k == &key) {
            *old = value;
        } else {
            object.push((key, value));
        }
    }
}
pub fn to_rew_plan(p: &MeasurementPlan) -> Result<secs_rew::MeasurementPlan, MeasureError> {
    Ok(secs_rew::MeasurementPlan {
        roles: p.roles.clone(),
        outputs: p
            .outputs
            .iter()
            .map(|(k, v)| Ok((k.clone(), positive(*v, "output")?)))
            .collect::<Result<_, MeasureError>>()?,
        positions: p
            .positions
            .iter()
            .map(|p| positive(*p, "position"))
            .collect::<Result<_, _>>()?,
        output_device: p.output_device.clone(),
        input_device: p.input_device.clone(),
        input_channel: positive(p.input_channel, "input")?,
        sweep_level_db: p.sweep_level_db,
        sweep_length: p.sweep_length.clone(),
        repetitions: positive(p.repetitions, "repetitions")?,
        timing_reference: p.timing_reference.as_disk_str().into(),
        timing_ref_output: p
            .timing_ref_output
            .map(|v| positive(v, "reference output"))
            .transpose()?,
        naming: p.naming.clone(),
        start_frequency: p.start_frequency,
        end_frequency: p.end_frequency,
        sample_rate: positive(p.sample_rate, "sample rate")?,
    })
}
pub fn to_audio_plan(
    p: &MeasurementPlan,
    endpoints: &[secs_audio::Endpoint],
    settings: &MeasurementSettings,
) -> Result<secs_audio::MeasurementPlan, MeasureError> {
    let choose = |id: &str, input: bool| {
        endpoints
            .iter()
            .find(|e| {
                (if input {
                    e.max_input_channels
                } else {
                    e.max_output_channels
                }) > 0
                    && (e.id == id
                        || e.name == id
                        || id.is_empty()
                            && if input {
                                e.is_default_input
                            } else {
                                e.is_default_output
                            })
            })
            .cloned()
            .ok_or_else(|| MeasureError::Audio(secs_audio::AudioError::DeviceNotFound(id.into())))
    };
    let channel = |v, name| {
        u16::try_from(positive(v, name)?)
            .map_err(|_| MeasureError::Input(format!("{name} exceeds u16")))
    };
    // The timing reference and the repetitions are REW's (U15). A native
    // session plays every channel of a position against one clock, so their
    // arrival times need no reference, and it records one sweep per channel.
    // Values an REW setup stored stay in the project for REW and are not read
    // here: the screen does not show them on this path.
    Ok(secs_audio::MeasurementPlan {
        output: choose(&p.output_device, false)?,
        input: choose(&p.input_device, true)?,
        roles: p
            .roles
            .iter()
            .map(|role| {
                Ok(secs_audio::RoleOutput {
                    role: role.clone(),
                    channel: channel(p.output_for(role), "output")?,
                })
            })
            .collect::<Result<_, MeasureError>>()?,
        input_channel: channel(p.input_channel, "input")?,
        loopback_input_channel: None,
        sample_rate: positive(p.sample_rate, "sample rate")?,
        sweep_length: p.sweep_length.clone(),
        start_frequency: p.start_frequency,
        end_frequency: p.end_frequency,
        level_dbfs: p.sweep_level_db,
        share: settings.share,
        window: settings.window,
    })
}

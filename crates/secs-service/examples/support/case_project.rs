use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use secs_engine::Samples;
use secs_project::{Band, MeasurementResult, Project, Speaker, SpeakerConfig};
use secs_types::calibration::CalibrationSettings;

pub fn write(path: &Path, name: &str) -> (PathBuf, CalibrationSettings) {
    write_with(path, name, |_| {})
}

/// [`write`] with the case's calibration settings adjusted before they are
/// stored in the project (the example turns MIMO on with it).
pub fn write_with(
    path: &Path,
    name: &str,
    adjust: impl FnOnce(&mut CalibrationSettings),
) -> (PathBuf, CalibrationSettings) {
    let case = secs_engine::test_support::replay_case(name);
    let mut settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    adjust(&mut settings);
    let roles: Vec<_> = case
        .config
        .mains
        .iter()
        .chain(&case.config.subs)
        .cloned()
        .collect();
    let role_set: BTreeSet<_> = roles.iter().map(String::as_str).collect();
    let speakers = roles
        .iter()
        .enumerate()
        .map(|(index, role)| Speaker {
            role: role.clone(),
            size: if case.config.subs.contains(role) {
                "sub"
            } else {
                "small"
            }
            .into(),
            bands: vec![Band {
                band: String::new(),
                out: index as i64 + 1,
                pol: "+".into(),
            }],
        })
        .collect();
    let path = if path
        .extension()
        .is_some_and(|extension| extension == "secs")
    {
        path.to_owned()
    } else {
        path.join(format!("{name}.secs"))
    };
    let mut project = Project {
        name: name.into(),
        config: SpeakerConfig {
            speakers,
            device: String::new(),
            preset: "custom".into(),
            biamp_default: false,
        },
        target: case.config.target.clone(),
        xo: case.config.xo.clone(),
        ..Project::default()
    };
    secs_project::store_calibration(&mut project, &settings);
    let mut store = secs_project::MeasurementStore::new(&path, project);
    for (position, measurements) in case.measurements.positions {
        store.start_position(i64::from(position));
        for (role, measurement) in measurements {
            if !role_set.contains(role.as_str()) {
                continue;
            }
            let samples = match measurement.samples {
                Samples::F32(samples) => samples.to_vec(),
                Samples::F64(samples) => samples.iter().map(|&value| value as f32).collect(),
            };
            store
                .record(
                    MeasurementResult {
                        role: role.clone(),
                        pos: i64::from(position),
                        ir: samples,
                        sample_rate: measurement.sample_rate,
                        t0_s: measurement.t0,
                        spl_db: None,
                        spl_offset_db: None,
                        snr_db: None,
                        uuid: Some(format!("{name}-{position}-{role}")),
                        source: Some("oracle".into()),
                    },
                    Some(&roles),
                )
                .unwrap();
        }
    }
    store.save().unwrap();
    (path, settings)
}

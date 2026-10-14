use crate::{Project, PyJson};
use secs_types::role::{RoleKind, role_kind};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MeasurementSignatureEntry {
    pub pos: String,
    pub roles: Vec<String>,
    pub t0: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct FloatKey(pub f64);

pub fn measured_positions(project: &Project) -> Vec<i64> {
    let mut out = project
        .measurements
        .array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|record| {
            let ir = record.get("ir")?.object()?;
            if ir.is_empty() {
                return None;
            }
            crate::project::integer(record.get("pos"), 0, "measurement.pos").ok()
        })
        .collect::<Vec<_>>();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn measurement_signature(project: &Project) -> Vec<MeasurementSignatureEntry> {
    let mut out = project
        .measurements
        .array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|record| {
            let object = record.object()?;
            let pos = object
                .iter()
                .find(|(key, _)| key == "pos")
                .map(|(_, value)| python_str(value))
                .unwrap_or_else(|| "None".into());
            let mut roles = record
                .get("ir")
                .and_then(PyJson::object)
                .map_or_else(Vec::new, |value| {
                    value.iter().map(|(role, _)| role.clone()).collect()
                });
            roles.sort();
            let mut t0 = record
                .get("t0")
                .and_then(PyJson::object)
                .map_or_else(Vec::new, |value| {
                    value
                        .iter()
                        .map(|(role, value)| (role.clone(), python_repr(value)))
                        .collect()
                });
            t0.sort();
            Some(MeasurementSignatureEntry { pos, roles, t0 })
        })
        .collect::<Vec<_>>();
    out.sort();
    out
}

/// Median REW SPL offset in dB across every project measurement entry.
pub fn project_spl_offset(project: &Project) -> Option<f64> {
    let mut values: Vec<_> = project
        .measurements
        .array()
        .unwrap_or(&[])
        .iter()
        .filter_map(PyJson::object)
        .filter_map(|record| {
            record
                .iter()
                .find(|(key, _)| key == "spl_offset")
                .and_then(|(_, value)| value.object())
        })
        .flat_map(|offsets| {
            offsets.iter().filter_map(|(_, value)| match value {
                PyJson::Float(value) => Some(*value),
                PyJson::Int(value) => value.parse().ok(),
                _ => None,
            })
        })
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

pub fn target_key(project: &Project) -> Vec<(FloatKey, FloatKey)> {
    let mut points = project.target.points.clone();
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    points
        .into_iter()
        .map(|(frequency, level)| (FloatKey(frequency), FloatKey(level)))
        .collect()
}

pub fn estimate_seconds(project: &Project, mode: &str) -> f64 {
    let mains = project
        .config
        .speakers
        .iter()
        .filter(|speaker| role_kind(&speaker.role) != RoleKind::Sub)
        .count();
    let subs = project.config.speakers.len() - mains;
    let positions = measured_positions(project).len().max(1);
    let per = if mode == "zero" { 4.1 } else { 6.5 } * (0.4 + 0.6 * positions.min(9) as f64 / 9.0);
    (per * mains.max(1) as f64 + if subs > 1 { 10.0 } else { 0.0 }).max(5.0)
}

pub fn estimate_subs_seconds(project: &Project) -> f64 {
    let subs = project
        .config
        .speakers
        .iter()
        .filter(|speaker| role_kind(&speaker.role) == RoleKind::Sub)
        .count();
    let positions = measured_positions(project).len().max(1);
    (18.0 * (0.4 + 0.6 * positions.min(9) as f64 / 9.0) * if subs > 1 { 1.0 } else { 0.3 }).max(3.0)
}

pub fn estimate_xo_seconds(project: &Project, mode: &str, candidates: usize) -> f64 {
    let workers = candidates
        .min(std::thread::available_parallelism().map_or(1, |value| value.get() / 2))
        .max(1);
    let rounds = candidates.div_ceil(workers);
    estimate_seconds(project, mode) * rounds as f64 * if rounds < candidates { 1.1 } else { 1.0 }
}

fn python_str(value: &PyJson) -> String {
    match value {
        PyJson::Null => "None".into(),
        PyJson::Bool(true) => "True".into(),
        PyJson::Bool(false) => "False".into(),
        PyJson::Int(value) | PyJson::Str(value) => value.clone(),
        PyJson::Float(value) => python_float(*value),
        PyJson::Array(_) | PyJson::Object(_) => crate::dumps(value),
    }
}

fn python_repr(value: &PyJson) -> String {
    match value {
        PyJson::Null => "None".into(),
        PyJson::Bool(true) => "True".into(),
        PyJson::Bool(false) => "False".into(),
        PyJson::Int(value) => value.clone(),
        PyJson::Float(value) => python_float(*value),
        PyJson::Str(value) => format!("'{value}'"),
        PyJson::Array(_) | PyJson::Object(_) => crate::dumps(value),
    }
}

fn python_float(value: f64) -> String {
    if value.is_nan() {
        "nan".into()
    } else if value == f64::INFINITY {
        "inf".into()
    } else if value == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        let mut text = format!("{value:?}");
        if let Some((mantissa, exponent)) = text.split_once('e') {
            let sign = exponent.starts_with(['+', '-']);
            let digits = exponent.trim_start_matches(['+', '-']);
            if digits.len() == 1 {
                text = format!(
                    "{mantissa}e{}0{digits}",
                    if sign { &exponent[..1] } else { "+" }
                );
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(values: &[f64]) -> Project {
        Project {
            measurements: PyJson::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        PyJson::Object(vec![(
                            "spl_offset".into(),
                            PyJson::Object(vec![(format!("R{index}"), PyJson::Float(*value))]),
                        )])
                    })
                    .collect(),
            ),
            ..Project::default()
        }
    }

    #[test]
    fn project_spl_offset_matches_numpy_median() {
        assert_eq!(project_spl_offset(&project(&[4.0])), Some(4.0));
        assert_eq!(project_spl_offset(&project(&[8.0, 2.0])), Some(5.0));
        assert_eq!(project_spl_offset(&project(&[8.0, 2.0, 4.0])), Some(4.0));
        assert_eq!(project_spl_offset(&project(&[])), None);
    }
}

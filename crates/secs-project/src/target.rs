use crate::pyjson::Shape;
use crate::{ProjectError, PyJson};
pub use secs_types::target::TargetCurve;

/// The `target` object [`TargetJson::to_json`] writes; `points` is replaced (F16).
pub(crate) static TARGET_SHAPE: Shape = Shape::Record {
    optional: &[],
    nested: &[],
};

/// `.secs` conversion of [`TargetCurve`], whose arithmetic lives in
/// `secs-types` so the engine can sample it without depending on file formats.
pub trait TargetJson: Sized {
    /// The `.secs` value SECS 2 writes for the curve.
    fn to_json(&self) -> PyJson;
    /// Reads the `.secs` value (an object with `points`, a bare array, or null).
    fn from_json(value: PyJson) -> Result<Self, ProjectError>;
}

impl TargetJson for TargetCurve {
    fn to_json(&self) -> PyJson {
        PyJson::Object(vec![(
            "points".into(),
            PyJson::Array(
                self.points
                    .iter()
                    .map(|(frequency, level)| {
                        PyJson::Array(vec![PyJson::Float(*frequency), PyJson::Float(*level)])
                    })
                    .collect(),
            ),
        )])
    }

    fn from_json(value: PyJson) -> Result<Self, ProjectError> {
        let points = match &value {
            PyJson::Null => &[][..],
            PyJson::Array(points) => points.as_slice(),
            PyJson::Object(_) => value
                .get("points")
                .map_or(&[][..], |value| value.array().unwrap_or(&[])),
            _ => {
                return Err(ProjectError::Invalid(
                    "target must be an object or array".into(),
                ));
            }
        };
        Ok(Self::new(
            points
                .iter()
                .map(|point| {
                    let pair = point.array().ok_or_else(|| {
                        ProjectError::Invalid("target point must be a pair".into())
                    })?;
                    if pair.len() != 2 {
                        return Err(ProjectError::Invalid(
                            "target point must contain two numbers".into(),
                        ));
                    }
                    Ok((
                        crate::project::float(&pair[0])?,
                        crate::project::float(&pair[1])?,
                    ))
                })
                .collect::<Result<Vec<_>, ProjectError>>()?,
        ))
    }
}

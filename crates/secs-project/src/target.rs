use crate::{ProjectError, PyJson};
use std::collections::BTreeMap;

fn python_sort_run(points: &mut [(f64, f64)]) -> usize {
    if points.len() < 2 {
        return points.len();
    }
    let mut n = 1;
    while n < points.len()
        && points[n].0.partial_cmp(&points[n - 1].0) != Some(std::cmp::Ordering::Less)
    {
        n += 1;
    }
    if n == points.len() {
        return n;
    }
    if n > 1 {
        if points[0].0 < points[n - 1].0 {
            return n;
        }
        points[..n].reverse();
    }
    n += 1;
    let mut equal_tail = 0;
    while n < points.len() {
        if points[n].0 < points[n - 1].0 {
            if equal_tail != 0 {
                points[n - equal_tail - 1..n].reverse();
                equal_tail = 0;
            }
        } else if points[n - 1].0 < points[n].0 {
            break;
        } else {
            equal_tail += 1;
        }
        n += 1;
    }
    if equal_tail != 0 {
        points[n - equal_tail - 1..n].reverse();
    }
    points[..n].reverse();
    while n < points.len()
        && points[n].0.partial_cmp(&points[n - 1].0) != Some(std::cmp::Ordering::Less)
    {
        n += 1;
    }
    n
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetCurve {
    pub points: Vec<(f64, f64)>,
}
impl Default for TargetCurve {
    fn default() -> Self {
        Self::new(vec![
            (25., 2.),
            (80., 2.),
            (200., 0.),
            (1000., 0.),
            (16000., -2.),
        ])
    }
}
impl TargetCurve {
    pub fn to_json(&self) -> PyJson {
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

    pub fn from_json(value: PyJson) -> Result<Self, ProjectError> {
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
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        let mut s = Self { points };
        s.sort();
        s
    }
    pub fn sort(&mut self) {
        // Target curves are shorter than CPython's TimSort minimum run. Match
        // its natural-run detection before extending that run by binary sort;
        // this comparison order matters for NaN, while +0/-0 remain stable.
        let sorted = python_sort_run(&mut self.points);
        for index in sorted..self.points.len() {
            let point = self.points[index];
            let mut lo = 0;
            let mut hi = index;
            while lo < hi {
                let mid = (lo + hi) / 2;
                if point.0 < self.points[mid].0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            self.points[lo..=index].rotate_right(1);
        }
    }
    pub fn at(&self, f: f64) -> f64 {
        if self.points.is_empty() {
            return 0.;
        }
        if f <= self.points[0].0 {
            return self.points[0].1;
        }
        if f >= self.points[self.points.len() - 1].0 {
            return self.points[self.points.len() - 1].1;
        }
        for w in self.points.windows(2) {
            let ((fa, da), (fb, db)) = (w[0], w[1]);
            if fa <= f && f <= fb {
                let t = (f.ln() - fa.ln()) / (fb.ln() - fa.ln());
                let u = t * t * (3. - 2. * t);
                return da + (db - da) * u;
            }
        }
        0.
    }
    pub fn sample(&self, freqs: &[f64]) -> Vec<f64> {
        freqs.iter().map(|&f| self.at(f)).collect()
    }
    pub fn presets() -> BTreeMap<&'static str, Self> {
        [
            (
                "flat",
                vec![(25., 0.), (200., 0.), (1000., 0.), (16000., 0.)],
            ),
            (
                "harman",
                vec![(25., 6.), (60., 5.), (120., 1.), (1000., 0.), (16000., -3.)],
            ),
            (
                "cinema",
                vec![
                    (25., 4.),
                    (80., 3.),
                    (200., 0.),
                    (1000., -1.),
                    (16000., -5.),
                ],
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k, Self::new(v)))
        .collect()
    }
}

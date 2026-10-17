//! Port of wizard/step5_measure.py and mdat_loader.py (SECS 2, GPL-3.0-only).
use crate::measure::json::JsonExt;
use crate::measure::{MeasureError, MeasurementSettings, plan_for, position_count};
use secs_project::{MeasurementResult, MeasurementStore, PyJson};
use secs_types::role::{ROLES, role_code, role_from_title};
use std::{collections::BTreeSet, path::Path};

pub fn split_position(title: &str) -> (String, i64) {
    let text = title.trim();
    let digit_start = text.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digit_start < text.len()
        && let Ok(pos) = text[digit_start..].parse::<i64>()
    {
        let prefix = &text[..digit_start];
        if let Some(base) = prefix.strip_suffix(['P', 'p']) {
            let base = base
                .trim_end_matches(|c: char| c.is_whitespace() || c == '_' || c == '-')
                .trim();
            if !base.is_empty() {
                return (base.into(), pos.max(1));
            }
        }
        if prefix.ends_with(char::is_whitespace) {
            let base = prefix.trim();
            if role_from_title(base, None).is_some() {
                return (base.into(), pos.max(1));
            }
        }
    }
    (text.into(), 1)
}
pub fn match_role(title: &str, roles: &[String], used: &BTreeSet<String>) -> Option<String> {
    let text = title.trim().to_uppercase();
    let base = roles
        .iter()
        .map(|r| r.split(':').next().unwrap().to_uppercase())
        .filter(|base| {
            text.strip_prefix(base)
                .is_some_and(|rest| !rest.starts_with(char::is_alphanumeric))
        })
        .max_by_key(String::len)?;
    let candidates = roles
        .iter()
        .filter(|r| r.split(':').next().unwrap().to_uppercase() == base)
        .collect::<Vec<_>>();
    let tokens = text[base.len()..]
        .split(|c: char| !c.is_ascii_uppercase() && !c.is_ascii_digit())
        .collect::<Vec<_>>();
    candidates
        .iter()
        .find(|r| {
            r.split(':').nth(1).is_some_and(|band| {
                !band.is_empty() && tokens.contains(&band.to_uppercase().as_str())
            })
        })
        .or_else(|| candidates.iter().find(|r| !used.contains(r.as_str())))
        .or_else(|| candidates.first())
        .map(|r| (*r).clone())
}
pub fn suggest_assignment(titles: &[String], roles: &[String]) -> Vec<Option<String>> {
    let wanted = roles.iter().map(String::as_str).collect::<Vec<_>>();
    let mut used = BTreeSet::new();
    titles
        .iter()
        .map(|title| {
            let base = role_from_title(title, Some(&wanted))?;
            let role = roles
                .iter()
                .find(|r| r.split(':').next() == Some(base) && !used.contains(r.as_str()))?
                .clone();
            used.insert(role.clone());
            Some(role)
        })
        .collect()
}
pub fn auto_assign(
    titles: &[String],
    roles: &[String],
    flags: &[Option<bool>],
) -> Vec<Option<String>> {
    let mut suggested = suggest_assignment(titles, roles);
    if suggested.iter().any(Option::is_some) {
        let free = roles
            .iter()
            .filter(|r| !suggested.contains(&Some((*r).clone())) && is_sub(r))
            .cloned()
            .collect::<Vec<_>>();
        let loose = suggested
            .iter()
            .enumerate()
            .filter(|(i, r)| r.is_none() && flags.get(*i) == Some(&Some(true)))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if !loose.is_empty() && loose.len() <= free.len() {
            for (i, r) in loose.into_iter().zip(free) {
                suggested[i] = Some(r);
            }
        }
        return suggested;
    }
    let mut used = BTreeSet::new();
    titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let role = match_role(title, roles, &used).or_else(|| {
                roles
                    .iter()
                    .find(|r| {
                        !used.contains(r.as_str())
                            && flags
                                .get(i)
                                .copied()
                                .flatten()
                                .is_none_or(|flag| is_sub(r) == flag)
                    })
                    .cloned()
            })?;
            if !used.insert(role.clone()) {
                return None;
            }
            Some(role)
        })
        .collect()
}
fn is_sub(role: &str) -> bool {
    role.split(':')
        .next()
        .unwrap()
        .to_uppercase()
        .starts_with("SW")
}

/// Radix-2 real-input spectrum power, for the upstream two-band classifier.
/// Kept local to avoid adding a public numerical API to the service seam.
pub fn looks_like_sub(samples: &[f64], sample_rate: i64) -> bool {
    if samples.len() < 64 || samples.iter().all(|x| *x == 0.0) || sample_rate <= 0 {
        return false;
    }
    let n = samples.len().max(8192).next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..samples.len()].copy_from_slice(samples);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut width = 2;
    while width <= n {
        let angle = -2.0 * std::f64::consts::PI / width as f64;
        let (wi, wr) = angle.sin_cos();
        for start in (0..n).step_by(width) {
            let (mut ur, mut ui) = (1.0, 0.0);
            for k in 0..width / 2 {
                let a = start + k;
                let b = a + width / 2;
                let (vr, vi) = (re[b] * ur - im[b] * ui, re[b] * ui + im[b] * ur);
                re[b] = re[a] - vr;
                im[b] = im[a] - vi;
                re[a] += vr;
                im[a] += vi;
                (ur, ui) = (ur * wr - ui * wi, ur * wi + ui * wr);
            }
        }
        width *= 2;
    }
    let (mut low, mut high, mut nl, mut nh) = (0.0, 0.0, 0, 0);
    for i in 0..=n / 2 {
        let f = i as f64 * sample_rate as f64 / n as f64;
        let power = re[i] * re[i] + im[i] * im[i];
        if (20.0..100.0).contains(&f) {
            low += power;
            nl += 1;
        }
        if (500.0..5000.0).contains(&f) {
            high += power;
            nh += 1;
        }
    }
    nl > 0
        && nh > 0
        && 10.0 * ((high / f64::from(nh) + 1e-30) / (low / f64::from(nl) + 1e-30)).log10() < -30.0
}

pub fn problems(assignment: &[Option<String>], positions: &[i64]) -> String {
    let chosen = assignment
        .iter()
        .zip(positions)
        .filter_map(|(r, p)| r.as_ref().filter(|r| !r.is_empty()).map(|r| (*p, r)))
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut doubles = BTreeSet::new();
    for item in &chosen {
        if !seen.insert(*item) {
            doubles.insert(*item);
        }
    }
    if !doubles.is_empty() {
        let several = chosen.iter().map(|(p, _)| p).collect::<BTreeSet<_>>().len() > 1;
        let names = doubles
            .iter()
            .map(|(p, r)| {
                if several {
                    format!("{} P{p}", role_code(r))
                } else {
                    role_code(r).into()
                }
            })
            .collect::<Vec<String>>();
        return format!("같은 역할이 두 번 배정됐습니다: {}", names.join(", "));
    }
    if chosen.is_empty() {
        return "불러올 측정이 없습니다. 하나 이상 역할을 고르세요.".into();
    }
    String::new()
}
/// Presentation-neutral twin of the upstream assignment-dialog validation.
pub fn problem_notices(
    assignment: &[Option<String>],
    positions: &[i64],
) -> Vec<secs_types::ipc::Notice> {
    let mut seen = BTreeSet::new();
    let mut doubles = BTreeSet::new();
    for (role, pos) in assignment.iter().zip(positions) {
        if let Some(role) = role.as_ref().filter(|r| !r.is_empty())
            && !seen.insert((*pos, role.clone()))
        {
            doubles.insert((*pos, role.clone()));
        }
    }
    if !doubles.is_empty() {
        return vec![secs_types::ipc::Notice {
            code: "mdat_duplicate_roles".into(),
            params: [(
                "roles".into(),
                serde_json::to_string(&doubles).expect("role pairs serialize"),
            )]
            .into(),
        }];
    }
    if seen.is_empty() {
        return vec![secs_types::ipc::Notice {
            code: "mdat_nothing_selected".into(),
            params: Default::default(),
        }];
    }
    vec![]
}

#[derive(Debug, Clone)]
pub struct MdatMeasurement {
    pub info: secs_mdat::MeasurementInfo,
    pub title_base: String,
    pub position: i64,
    pub role: Option<String>,
    pub looks_like_sub: Option<bool>,
}
#[derive(Debug, Clone)]
pub struct MdatScan {
    pub measurements: Vec<MdatMeasurement>,
    pub problems: String,
}

fn load(path: &Path) -> Result<secs_mdat::LoadedMdatProject, MeasureError> {
    use std::io::Read;
    // Bound input allocation before the parser's own graph/array budgets.
    const LIMIT: u64 = 256 * 1024 * 1024;
    let file = std::fs::File::open(path).map_err(|e| MeasureError::Input(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| MeasureError::Input(e.to_string()))?;
    if bytes.len() as u64 > LIMIT {
        return Err(MeasureError::Input("mdat exceeds file budget".into()));
    }
    secs_mdat::load_mdat_project(&bytes, &Default::default())
        .map_err(|e| MeasureError::Input(e.to_string()))
}
pub fn scan(path: &Path) -> Result<MdatScan, MeasureError> {
    scan_for(
        path,
        &ROLES.iter().map(|r| (*r).into()).collect::<Vec<_>>(),
        None,
        1,
    )
}
pub fn scan_for(
    path: &Path,
    roles: &[String],
    position: Option<i64>,
    current_position: i64,
) -> Result<MdatScan, MeasureError> {
    Ok(scan_loaded(&load(path)?, roles, position, current_position))
}
fn scan_loaded(
    loaded: &secs_mdat::LoadedMdatProject,
    roles: &[String],
    position: Option<i64>,
    current: i64,
) -> MdatScan {
    let mut measurements = loaded
        .measurements
        .iter()
        .map(|info| {
            let (base, pos) = split_position(&info.title);
            let flag = loaded
                .ir_provider
                .get_ir(&info.uuid)
                .map(|(ir, rate)| looks_like_sub(&ir, rate));
            MdatMeasurement {
                info: info.clone(),
                title_base: base,
                position: pos,
                role: None,
                looks_like_sub: flag,
            }
        })
        .collect::<Vec<_>>();
    let has_tags = measurements
        .iter()
        .any(|m| m.title_base != m.info.title.trim());
    for m in &mut measurements {
        if let Some(p) = position {
            m.position = p;
        } else if !has_tags {
            m.position = current.max(1);
        }
    }
    for pos in measurements
        .iter()
        .map(|m| m.position)
        .collect::<BTreeSet<_>>()
    {
        let indices = measurements
            .iter()
            .enumerate()
            .filter(|(_, m)| m.position == pos)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let titles = indices
            .iter()
            .map(|i| measurements[*i].title_base.clone())
            .collect::<Vec<_>>();
        let flags = indices
            .iter()
            .map(|i| measurements[*i].looks_like_sub)
            .collect::<Vec<_>>();
        for (i, r) in indices.into_iter().zip(auto_assign(&titles, roles, &flags)) {
            measurements[i].role = r;
        }
    }
    let problem = problems(
        &measurements
            .iter()
            .map(|m| m.role.clone())
            .collect::<Vec<_>>(),
        &measurements.iter().map(|m| m.position).collect::<Vec<_>>(),
    );
    MdatScan {
        measurements,
        problems: problem,
    }
}
/// Per-row positions let the assignment dialog override filename guesses.
#[derive(Debug, Clone)]
pub struct Assignment {
    pub role: Option<String>,
    pub position: Option<i64>,
}
pub fn import(
    store: &mut MeasurementStore,
    path: &Path,
    assignment: Option<&[Assignment]>,
    position: Option<i64>,
) -> Result<usize, MeasureError> {
    let loaded = load(path)?;
    let plan = plan_for(store.project(), &MeasurementSettings::default());
    let count = position_count(store.project());
    let current = store
        .pending_positions(&plan.positions, &plan.roles)
        .first()
        .copied()
        .unwrap_or(1);
    let scan = scan_loaded(&loaded, &plan.roles, position, current);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut used = BTreeSet::new();
    let mut saved = 0;
    for (i, m) in scan.measurements.iter().enumerate() {
        let role = match assignment {
            Some(a) => a.get(i).and_then(|a| a.role.as_ref()),
            None => m.role.as_ref(),
        };
        let pos = assignment
            .and_then(|a| a.get(i))
            .and_then(|a| a.position)
            .unwrap_or(m.position);
        let Some(role) = role.filter(|r| plan.roles.contains(r)) else {
            continue;
        };
        if pos > count || !used.insert((pos, role.clone())) {
            continue;
        }
        let Some((ir, rate)) = loaded.ir_provider.get_ir(&m.info.uuid) else {
            continue;
        };
        let rate = crate::measure::positive(rate, "sample rate")?;
        store.record(
            MeasurementResult {
                role: role.clone(),
                pos,
                ir: ir.into_iter().map(|v| v as f32).collect(),
                sample_rate: rate,
                t0_s: m.info.start_time_s,
                spl_db: None,
                spl_offset_db: None,
                snr_db: None,
                uuid: Some(m.info.uuid.clone()),
                source: Some(name.clone()),
                source_info: Some(m.info.source_info.clone()),
            },
            Some(&plan.roles),
        )?;
        // MeasurementStore follows upstream session.record; the importer owns
        // the source field, as step5.load_mdat does after recording each IR.
        if let PyJson::Array(entries) = &mut store.project_mut().measurements {
            for entry in entries {
                if entry
                    .get("pos")
                    .is_some_and(|v| *v == PyJson::Int(pos.to_string()))
                {
                    let object = entry.object_mut().unwrap();
                    let index = object
                        .iter()
                        .position(|(k, _)| k == "source")
                        .unwrap_or_else(|| {
                            object.push(("source".into(), PyJson::Object(vec![])));
                            object.len() - 1
                        });
                    if object[index].1.object().is_none() {
                        object[index].1 = PyJson::Object(vec![]);
                    }
                    let sources = object[index].1.object_mut().unwrap();
                    if let Some((_, value)) = sources.iter_mut().find(|(k, _)| k == role) {
                        *value = PyJson::Str(name.clone());
                    } else {
                        sources.push((role.clone(), PyJson::Str(name.clone())));
                    }
                }
            }
        }
        store.save()?;
        saved += 1;
    }
    Ok(saved)
}

pub fn mdat_sources(store: &MeasurementStore) -> std::collections::BTreeMap<i64, String> {
    let mut out = std::collections::BTreeMap::new();
    if let PyJson::Array(entries) = &store.project().measurements {
        for entry in entries {
            let pos = match entry.get("pos") {
                Some(PyJson::Int(v)) => v.parse::<i64>().unwrap_or(-1),
                _ => -1,
            };
            if pos <= 0 {
                continue;
            }
            let Some(irs) = entry.get("ir").and_then(JsonExt::object) else {
                continue;
            };
            let mut names = Vec::new();
            for (role, path) in irs {
                if path.str().is_none_or(str::is_empty) {
                    continue;
                }
                let name = entry
                    .get("source")
                    .and_then(|s| s.get(role))
                    .and_then(JsonExt::str)
                    .unwrap_or("");
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            if names.is_empty() {
                continue;
            }
            out.insert(
                pos,
                if names.iter().all(|s| s.is_empty()) {
                    String::new()
                } else {
                    names
                        .iter()
                        .map(|s| if s.is_empty() { "직접 측정" } else { s })
                        .collect::<Vec<_>>()
                        .join(" + ")
                },
            );
        }
    }
    out
}

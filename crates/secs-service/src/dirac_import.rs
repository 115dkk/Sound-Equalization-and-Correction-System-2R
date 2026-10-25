//! Dirac Live `.liveproject` measurements through the mdat import flow.
//!
//! This is new in SECS 2R and is not present in SECS 2. The format is
//! documented in `docs/DIRAC_LIVEPROJECT.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use secs_dirac::{ChannelKind, Limits, LiveProject};
use secs_project::MeasurementStore;
use secs_types::role::SUB_ROLES;
use sha2::{Digest, Sha256};

use crate::mdat_import;
use crate::measure::{MeasureError, MeasurementSettings, plan_for};
use crate::measurement_file::{self, Assignment, FileRow, ImportedIr, MeasurementReader};

fn content_uuid(samples: &[f64], sample_rate: u32, t0_s: f64) -> String {
    let mut hasher = Sha256::new();
    for sample in samples {
        hasher.update((*sample as f32).to_le_bytes());
    }
    hasher.update(sample_rate.to_le_bytes());
    hasher.update(t0_s.to_bits().to_le_bytes());
    let digest = hasher.finalize();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x50;
    id[8] = (id[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        id[0],
        id[1],
        id[2],
        id[3],
        id[4],
        id[5],
        id[6],
        id[7],
        id[8],
        id[9],
        id[10],
        id[11],
        id[12],
        id[13],
        id[14],
        id[15]
    )
}

/// Returns whether the path has the Dirac Live project extension.
pub fn is_liveproject(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("liveproject"))
}

fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous = None;
    for character in name.chars() {
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            previous = None;
            continue;
        }
        if previous
            .is_some_and(|previous: char| previous.is_lowercase() && character.is_uppercase())
            && !word.is_empty()
        {
            words.push(std::mem::take(&mut word));
        }
        word.extend(character.to_lowercase());
        previous = Some(character);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn subwoofer_words(words: &[String]) -> bool {
    words
        .iter()
        .any(|word| matches!(word.as_str(), "sub" | "subwoofer" | "lfe"))
}

/// Base role (an entry of `ROLES`) for a Dirac channel; subwoofers return `None`
/// here and get their SW number from their order in the file.
pub fn base_role(name: &str) -> Option<&'static str> {
    let words = words(name);
    if words.is_empty() || subwoofer_words(&words) {
        return None;
    }
    let has = |wanted: &str| words.iter().any(|word| word == wanted);

    let center = has("center") || has("centre");
    if center
        && words
            .iter()
            .all(|word| matches!(word.as_str(), "center" | "centre" | "front"))
    {
        return Some("C");
    }

    let left = has("left");
    let right = has("right");
    let side = match (left, right) {
        (true, false) => true,
        (false, true) => false,
        _ => return None,
    };
    let pair = |left_role, right_role| if side { left_role } else { right_role };

    if has("top") {
        if has("front") {
            return Some(pair("Tfl", "Tfr"));
        }
        if has("middle") || has("mid") {
            return Some(pair("Tml", "Tmr"));
        }
        if has("rear") || has("back") {
            return Some(pair("Tbl", "Tbr"));
        }
        return None;
    }
    if has("height") {
        if has("rear") || has("back") {
            return Some(pair("Hbl", "Hbr"));
        }
        return Some(pair("Hfl", "Hfr"));
    }
    if has("wide") {
        return Some(pair("Lw", "Rw"));
    }
    if has("back") || has("rear") {
        return Some(pair("Lb", "Rb"));
    }
    if has("surround") || has("side") {
        return Some(pair("Ls", "Rs"));
    }
    if words
        .iter()
        .all(|word| matches!(word.as_str(), "left" | "right" | "front"))
    {
        return Some(pair("L", "R"));
    }
    None
}

fn channel_roles(project: &LiveProject) -> Vec<Option<&'static str>> {
    let mut subwoofer = 0usize;
    project
        .channels
        .iter()
        .map(|channel| {
            let channel_words = words(&channel.name);
            if channel.kind == ChannelKind::Subwoofer || subwoofer_words(&channel_words) {
                let role = SUB_ROLES.get(subwoofer).copied();
                subwoofer += 1;
                role
            } else {
                base_role(&channel.name)
            }
        })
        .collect()
}

fn channel_titles(project: &LiveProject) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for channel in &project.channels {
        *counts.entry(channel.name.as_str()).or_insert(0usize) += 1;
    }
    let mut occurrences = BTreeMap::new();
    project
        .channels
        .iter()
        .map(|channel| {
            if counts[channel.name.as_str()] == 1 {
                return channel.name.clone();
            }
            let occurrence = occurrences.entry(channel.name.as_str()).or_insert(0usize);
            *occurrence += 1;
            format!("{} {}", channel.name, occurrence)
        })
        .collect()
}

/// A Dirac project parsed and classified once, as the import flow's reader.
#[derive(Debug)]
pub(crate) struct DiracReader {
    project: LiveProject,
    rows: Vec<FileRow>,
    /// The Dirac position (from 0) and channel of each row.
    measured: Vec<(usize, usize)>,
}

impl DiracReader {
    /// Reads `path` (256 MiB limit) and lists its measured channels per
    /// position. Dirac position p is position p + 1; roles come from the
    /// channel names, subwoofers from SW1 in order (ADR 0013).
    pub(crate) fn read(path: &Path, roles: &[String]) -> Result<Self, MeasureError> {
        let bytes = measurement_file::read(path, "Dirac project")?;
        let project = secs_dirac::parse(&bytes, &Limits::default()).map_err(|error| {
            let message = if error == secs_dirac::DiracError::NoMeasurements {
                "Dirac project holds no measured impulse responses".into()
            } else {
                error.to_string()
            };
            MeasureError::Input(message)
        })?;
        let channel_roles = channel_roles(&project);
        let channel_titles = channel_titles(&project);
        let mut used: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
        let mut measured = Vec::new();
        let rows = project
            .measured()
            .map(|row| {
                let position =
                    i64::try_from(row.position).expect("Dirac position limit fits i64") + 1;
                let used = used.entry(position).or_default();
                let role = channel_roles[row.channel].and_then(|base| {
                    let role = roles.iter().find(|role| {
                        role.split(':').next() == Some(base) && !used.contains(role.as_str())
                    })?;
                    used.insert(role.clone());
                    Some(role.clone())
                });
                measured.push((row.position, row.channel));
                FileRow {
                    title: format!("{} P{position}", channel_titles[row.channel]),
                    position,
                    role,
                    looks_like_sub: Some(mdat_import::looks_like_sub(
                        row.samples,
                        i64::from(project.sample_rate),
                    )),
                    position_tagged: true,
                }
            })
            .collect();
        Ok(Self {
            project,
            rows,
            measured,
        })
    }
}

impl MeasurementReader for DiracReader {
    fn rows(&self) -> &[FileRow] {
        &self.rows
    }

    fn take(&self, row: usize) -> Result<ImportedIr, MeasureError> {
        let missing = || MeasureError::Input("Dirac measurement row is missing its IR".into());
        let (position, channel) = *self.measured.get(row).ok_or_else(missing)?;
        let samples = self
            .project
            .response(position, channel)
            .ok_or_else(missing)?;
        let sample_rate =
            crate::measure::positive(i64::from(self.project.sample_rate), "sample rate")?;
        let t0_s = -secs_dirac::NOMINAL_ARRIVAL_S;
        Ok(ImportedIr {
            samples: samples.iter().map(|sample| *sample as f32).collect(),
            sample_rate,
            t0_s,
            uuid: content_uuid(samples, self.project.sample_rate, t0_s),
            source_info: None,
        })
    }
}

/// Imports the assigned rows of the Dirac project at `path`, one
/// assignment per row, through the measurement-file import flow.
pub fn import(
    store: &mut MeasurementStore,
    path: &Path,
    assignments: &[Assignment],
) -> Result<usize, MeasureError> {
    let plan = plan_for(store.project(), &MeasurementSettings::default());
    let reader = DiracReader::read(path, &plan.roles)?;
    measurement_file::import(
        store,
        &reader,
        &measurement_file::file_name(path),
        assignments,
    )
}

// Port of the IR extraction in src/secs/mdat_loader.py (SECS 2, GPL-3.0-only).
use std::collections::BTreeMap;

use serde::Serialize;

use crate::{AllocationBudget, MdatError, MdatFile, REW_MDAT_IR_SCALE, SourceInfo, source_info};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MeasurementInfo {
    pub uuid: String,
    pub title: String,
    pub notes: String,
    pub sample_rate: i64,
    pub start_freq: f64,
    pub end_freq: f64,
    pub date: String,
    pub role: String,
    pub start_time_s: f64,
    pub measurement_delay_s: f64,
    pub timing_reference: String,
    #[serde(skip)]
    pub source_info: SourceInfo,
}

#[derive(Clone, Debug, Default)]
pub struct MdatIRProvider {
    impulse_responses: BTreeMap<String, (Vec<f64>, i64)>,
}

impl MdatIRProvider {
    /// Return an independent copy, as the upstream read-only provider does.
    pub fn get_ir(&self, measurement_id: &str) -> Option<(Vec<f64>, i64)> {
        self.impulse_responses.get(measurement_id).cloned()
    }
}

#[derive(Debug)]
pub struct LoadedMdatProject {
    pub mdat: MdatFile,
    pub measurements: Vec<MeasurementInfo>,
    pub ir_provider: MdatIRProvider,
}

/// Parse, adapt and extract. Adaptation and extraction each get a budget of
/// `limits.max_file_bytes`: extraction copies what adaptation already holds
/// (each IR once), so a file that adapts within the budget also extracts,
/// while one array referenced hundreds of times still exceeds it (TD-18).
pub fn load_mdat_project(
    bytes: &[u8],
    limits: &secs_jos::Limits,
) -> Result<LoadedMdatProject, MdatError> {
    let mdat = crate::load_with_budget(bytes, limits, &mut AllocationBudget::new(limits))?;
    extract_ir(mdat, &mut AllocationBudget::new(limits))
}

/// Copy each measurement's metadata, source information and scaled IR out of
/// the adapted file, charging `budget` before every copy. [`load_mdat_project`]
/// passes a fresh budget of `max_file_bytes`; a budget that runs out returns
/// the adapter's allocation-limit error.
pub fn extract_ir(
    mdat: MdatFile,
    budget: &mut AllocationBudget,
) -> Result<LoadedMdatProject, MdatError> {
    budget.charge(
        mdat.measurements
            .len()
            .saturating_mul(size_of::<MeasurementInfo>()),
    )?;
    let mut measurements = Vec::with_capacity(mdat.measurements.len());
    let mut impulse_responses = BTreeMap::new();
    for (index, measurement) in mdat.measurements.iter().enumerate() {
        let notes = &measurement.notes;
        let samples = &measurement.impulse_response.samples;
        // A fixed 1 KiB covers the uuid and its map key, a default title, the
        // role and the provider entry. timing_reference keeps a (byte offset,
        // char) pair and a folded byte per character, and returns at most
        // notes.len() bytes. Source info charges its own copies.
        budget.charge(1024)?;
        budget.charge(measurement.name.len())?;
        budget.charge(notes.len())?;
        budget.charge(notes.len().saturating_mul(size_of::<(usize, char)>() + 1))?;
        budget.charge(notes.len())?;
        budget.charge(samples.len().saturating_mul(size_of::<f64>()))?;
        let uuid = format!("mdat:{index}");
        measurements.push(MeasurementInfo {
            uuid: uuid.clone(),
            title: if measurement.name.is_empty() {
                format!("Measurement {}", index + 1)
            } else {
                measurement.name.clone()
            },
            notes: notes.clone(),
            sample_rate: measurement.sample_rate_hz,
            start_freq: measurement.frequency_response.spacing.valid_start_hz,
            end_freq: measurement.frequency_response.spacing.valid_end_hz,
            date: String::new(),
            role: "Ignore".into(),
            start_time_s: measurement.impulse_response.start_time_s,
            measurement_delay_s: measurement.impulse_response.measurement_delay_s,
            timing_reference: timing_reference(notes),
            source_info: source_info::extract(
                &mdat.stream,
                measurement.source_object,
                notes,
                budget,
            )?,
        });
        // NumPy 2 scalar promotion keeps this division in float32, not f64 x100.
        let samples = samples
            .iter()
            .map(|v| f64::from(*v as f32 / REW_MDAT_IR_SCALE as f32))
            .collect();
        impulse_responses.insert(uuid, (samples, measurement.sample_rate_hz));
    }
    Ok(LoadedMdatProject {
        mdat,
        measurements,
        ir_provider: MdatIRProvider { impulse_responses },
    })
}

fn timing_reference(notes: &str) -> String {
    // Match Python re.IGNORECASE's ASCII literals, including its four Unicode
    // additions; retain byte offsets so the captured reference is unchanged.
    let chars: Vec<_> = notes.char_indices().collect();
    let folded: String = chars
        .iter()
        .map(|(_, c)| match c {
            '\u{130}' | '\u{131}' => 'i',
            '\u{17f}' => 's',
            '\u{212a}' => 'k',
            c if c.is_ascii() => c.to_ascii_lowercase(),
            _ => '\0',
        })
        .collect();
    let prefix = "acoustic reference played from ";
    let suffixes = [
        " with timing offset",
        " with no timing offset",
        " with a timing offset",
    ];
    let mut offset = 0;
    while let Some(found) = folded[offset..].find(prefix) {
        let start = offset + found + prefix.len();
        let line_end = folded[start..]
            .find('\n')
            .map_or(folded.len(), |n| start + n);
        // (.+?) requires at least one character and stops at the first suffix.
        if start < line_end {
            for end in start + 1..line_end {
                if suffixes
                    .iter()
                    .any(|suffix| folded[end..].starts_with(suffix))
                {
                    let begin_byte = chars[start].0;
                    let end_byte = chars[end].0;
                    return notes[begin_byte..end_byte].trim().to_string();
                }
            }
        }
        offset = start;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_note_matches_upstream_pattern() {
        assert_eq!(
            timing_reference("Acoustic reference played from L with no timing offset"),
            "L"
        );
        assert_eq!(
            timing_reference("acouſtİc reference played from  천장 🔊  with a timing offset"),
            "천장 🔊"
        );
        assert_eq!(
            timing_reference(
                "Acoustic reference played from A with timing offset then B with timing offset"
            ),
            "A"
        );
        assert_eq!(
            timing_reference("Acoustic reference played from A\nB with timing offset"),
            ""
        );
        assert_eq!(
            timing_reference("Acoustic reference played from  with timing offset"),
            ""
        );
    }

    #[test]
    fn provider_returns_copies_and_preserves_float32_division() {
        let value = 0.003_f32;
        assert_ne!(f64::from(value / 0.01_f32), f64::from(value) / 0.01);
        let mut provider = MdatIRProvider::default();
        provider
            .impulse_responses
            .insert("mdat:0".into(), (vec![f64::from(value / 0.01_f32)], 48000));
        let (mut first, _) = provider.get_ir("mdat:0").unwrap();
        first[0] = 0.0;
        assert_ne!(provider.get_ir("mdat:0").unwrap().0, first);
        assert!(provider.get_ir("missing").is_none());
    }
}

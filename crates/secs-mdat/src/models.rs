// Port of src/secs/rew_mdat/models.py (SECS 2, GPL-3.0-only).
use std::collections::BTreeMap;

use serde::Serialize;

use crate::MdatError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldStatus {
    Confirmed,
    Inferred,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldMeaning {
    pub status: FieldStatus,
    pub source_field: Option<String>,
    pub note: Option<String>,
}

pub type Meanings = BTreeMap<String, FieldMeaning>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RewVersion {
    pub major: i64,
    pub minor: i64,
    pub suffix: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RoomDimensions {
    pub length_m: f64,
    pub width_m: f64,
    pub height_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrequencySpacing {
    pub count: i64,
    pub start_hz: f64,
    pub end_hz: f64,
    pub step_hz: f64,
    pub is_log_spaced: bool,
    pub points_per_octave: i64,
    pub valid_start_hz: f64,
    pub valid_end_hz: f64,
}

impl FrequencySpacing {
    /// NumPy arange/geomspace semantics, with an explicit allocation bound.
    pub fn frequencies(&self, max_elements: usize) -> Result<Vec<f64>, MdatError> {
        let count = if self.is_log_spaced && self.count <= 1 {
            1
        } else {
            usize::try_from(self.count.max(0))
                .map_err(|_| MdatError::Schema("frequency count is too large".into()))?
        };
        if count > max_elements {
            return Err(MdatError::Schema("frequency count exceeds limit".into()));
        }
        if !self.is_log_spaced {
            return Ok((0..count)
                .map(|i| self.start_hz + i as f64 * self.step_hz)
                .collect());
        }
        if count == 1 {
            return Ok(vec![self.start_hz]);
        }
        if self.start_hz == 0.0 || self.end_hz == 0.0 {
            return Err(MdatError::Schema(
                "geometric frequency endpoints cannot be zero".into(),
            ));
        }
        let sign = if self.start_hz < 0.0 && self.end_hz < 0.0 {
            -1.0
        } else {
            1.0
        };
        let start = (self.start_hz * sign).log10();
        let step = ((self.end_hz * sign).log10() - start) / (count - 1) as f64;
        let mut values: Vec<_> = (0..count)
            .map(|i| sign * 10.0_f64.powf(start + i as f64 * step))
            .collect();
        values[0] = self.start_hz;
        values[count - 1] = self.end_hz;
        Ok(values)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrequencyResponse {
    pub spacing: FrequencySpacing,
    pub spl_db: Vec<f64>,
    pub phase_deg: Vec<f64>,
    pub unwrapped_phase_deg: Vec<f64>,
    pub group_delay_s: Vec<f64>,
    pub meanings: Meanings,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImpulseResponse {
    pub samples: Vec<f64>,
    pub sample_interval_s: f64,
    pub start_time_s: f64,
    pub measurement_delay_s: f64,
    pub spl_offset_db: f64,
    pub peak_index: i64,
    pub meanings: Meanings,
}

impl ImpulseResponse {
    pub fn sample_rate_hz(&self) -> f64 {
        1.0 / self.sample_interval_s
    }
    pub fn time_s(&self) -> Vec<f64> {
        (0..self.samples.len())
            .map(|i| self.start_time_s + i as f64 * self.sample_interval_s)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DistortionDataset {
    pub frequency_hz: Vec<f64>,
    pub harmonic_arrays: Vec<Vec<f64>>,
    pub points_per_octave: i64,
    pub noise_floor_spl_db: Option<Vec<f64>>,
    pub meanings: Meanings,
    /// Names in upstream all_fields() order; raw values live in MdatFile::stream.
    pub unknown_fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Measurement {
    pub name: String,
    pub sample_rate_hz: i64,
    pub rew_version: RewVersion,
    pub room_dimensions: RoomDimensions,
    pub frequency_response: FrequencyResponse,
    pub impulse_response: ImpulseResponse,
    pub distortion: Option<DistortionDataset>,
    pub notes: String,
    pub unknown_fields: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MdatFile {
    pub title: String,
    pub rew_version_major: i32,
    pub rew_version_minor: i32,
    pub notes: String,
    pub preview_count: i32,
    pub measurements: Vec<Measurement>,
    pub bytes_consumed: usize,
    pub file_size: usize,
    pub top_level_count: usize,
    pub handle_count: usize,
    #[serde(skip)]
    pub raw_top_level: Vec<secs_jos::Value>,
    #[serde(skip)]
    pub unknown_top_level: Vec<secs_jos::Value>,
    /// Arena owning the IDs in raw_top_level and unknown_top_level.
    #[serde(skip)]
    pub stream: secs_jos::Stream,
}

impl MdatFile {
    pub fn trailing_bytes(&self) -> usize {
        self.file_size.saturating_sub(self.bytes_consumed)
    }
}

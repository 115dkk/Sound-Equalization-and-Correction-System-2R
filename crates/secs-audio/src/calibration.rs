//! REW calibration semantics:
//! <https://www.roomeqwizard.com/help/help_en-GB/html/calfiles.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/inputcal.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/splmeter.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/graph_impulse.html>
//!
//! Files describe measured response, not its inverse. REW subtracts that
//! response for frequency/SPL analysis, but does NOT merge it into the IR by
//! default. We likewise expose the curve and leave measured IRs unchanged.
//! Analog `Sensitivity ... dBFS` is input RMS at 94 dB SPL. USB `Sens Factor`
//! also requires device-specific volume-to-gain information: parsing that
//! header alone never asserts an absolute SPL calibration.
use crate::{AudioError, invalid};
use secs_dsp::fft;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationPoint {
    pub frequency: f64,
    pub gain_db: f64,
    pub phase_degrees: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MicCalibration {
    points: Vec<CalibrationPoint>,
    usb_sens_factor_db: Option<f64>,
    sensitivity_dbfs_at_94: Option<f64>,
}

impl MicCalibration {
    pub fn points(&self) -> &[CalibrationPoint] {
        &self.points
    }
    pub fn usb_sens_factor_db(&self) -> Option<f64> {
        self.usb_sens_factor_db
    }
    pub fn sensitivity_dbfs_at_94(&self) -> Option<f64> {
        self.sensitivity_dbfs_at_94
    }
    /// Strict numeric data, comments (#, ;, //, *), blank lines and quoted USB
    /// metadata. Malformed rows are errors, never silently skipped as data.
    pub fn parse(text: &str) -> Result<Self, AudioError> {
        if text.len() > 4 * 1024 * 1024 {
            return Err(invalid("calibration file too large"));
        }
        let mut out = Self {
            points: Vec::new(),
            usb_sens_factor_db: None,
            sensitivity_dbfs_at_94: None,
        };
        for (line_number, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || ["#", ";", "//", "*"].iter().any(|p| line.starts_with(p)) {
                continue;
            }
            let line = line.trim_matches('"');
            let error = || invalid(&format!("invalid calibration line {}", line_number + 1));
            if let Some(rest) = line.strip_prefix("Sens Factor") {
                let rest = rest.trim().strip_prefix('=').ok_or_else(error)?.trim();
                let end = rest.find("dB").ok_or_else(error)?;
                let value: f64 = rest[..end].trim().parse().map_err(|_| error())?;
                if !value.is_finite() || out.usb_sens_factor_db.replace(value).is_some() {
                    return Err(error());
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("Sensitivity") {
                let value: f64 = rest
                    .trim()
                    .strip_suffix("dBFS")
                    .ok_or_else(error)?
                    .trim()
                    .parse()
                    .map_err(|_| error())?;
                if !value.is_finite()
                    || value > 0.0
                    || out.sensitivity_dbfs_at_94.replace(value).is_some()
                {
                    return Err(error());
                }
                continue;
            }
            let line = line.split(['#', ';']).next().unwrap_or_default();
            let columns: Vec<_> = line
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .collect();
            if !(2..=3).contains(&columns.len()) {
                return Err(error());
            }
            let numbers = columns
                .iter()
                .map(|x| x.parse::<f64>().map_err(|_| error()))
                .collect::<Result<Vec<_>, _>>()?;
            if numbers.iter().any(|x| !x.is_finite())
                || numbers[0] <= 0.0
                || out.points.last().is_some_and(|p| p.frequency >= numbers[0])
                || numbers[1].abs() > 200.0
            {
                return Err(error());
            }
            out.points.push(CalibrationPoint {
                frequency: numbers[0],
                gain_db: numbers[1],
                phase_degrees: numbers.get(2).copied(),
            });
        }
        if out.points.len() < 2
            || (out.usb_sens_factor_db.is_some() && out.sensitivity_dbfs_at_94.is_some())
        {
            return Err(invalid(
                "calibration needs two ordered points and one sensitivity convention",
            ));
        }
        Ok(out)
    }

    /// Log-frequency interpolation; endpoint gains outside the tabulated range.
    pub fn gain_db(&self, frequency: f64) -> f64 {
        let i = self.points.partition_point(|p| p.frequency < frequency);
        if i == 0 {
            return self.points[0].gain_db;
        }
        if i == self.points.len() {
            return self.points[i - 1].gain_db;
        }
        let a = self.points[i - 1];
        let b = self.points[i];
        let t = (frequency / a.frequency).ln() / (b.frequency / a.frequency).ln();
        a.gain_db + t * (b.gain_db - a.gain_db)
    }

    pub fn spl_reference(&self) -> Option<SplReference> {
        self.sensitivity_dbfs_at_94.map(|s| SplReference {
            offset_db: 94.0 - s,
        })
    }

    /// Unweighted calibrated RMS; separate from the raw dBFS/clip meter.
    /// DC is excluded. Boost is bounded to 20 dB (REW exposes this as a setting).
    pub fn corrected_rms_dbfs(&self, samples: &[f64], sample_rate: u32) -> Result<f64, AudioError> {
        if samples.is_empty() || sample_rate == 0 || !samples.iter().all(|x| x.is_finite()) {
            return Err(invalid("invalid SPL samples"));
        }
        let spectrum = fft::rfft(samples);
        let mut energy = 0.0;
        for (i, bin) in spectrum.iter().enumerate().skip(1) {
            let frequency = i as f64 * f64::from(sample_rate) / samples.len() as f64;
            let correction = (-self.gain_db(frequency)).min(20.0);
            let weight = if samples.len().is_multiple_of(2) && i == samples.len() / 2 {
                1.0
            } else {
                2.0
            };
            energy += weight * bin.norm_sqr() * 10_f64.powf(correction / 10.0);
        }
        Ok(10.0 * (energy / (samples.len() as f64).powi(2)).log10())
    }
}

/// Valid only at the input device/gain/volume used for calibration. The caller
/// must invalidate this value when any of them changes (as REW does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplReference {
    pub offset_db: f64,
}
impl SplReference {
    pub fn from_reference(measured_dbfs: f64, reference_spl_db: f64) -> Result<Self, AudioError> {
        if !measured_dbfs.is_finite() || !reference_spl_db.is_finite() {
            return Err(invalid("nonfinite SPL reference"));
        }
        Ok(Self {
            offset_db: reference_spl_db - measured_dbfs,
        })
    }
    /// Only for a device whose USB sensitivity convention and input gain have
    /// been independently established. No vendor mapping is guessed here.
    pub fn from_usb(calibration: &MicCalibration, input_gain_db: f64) -> Result<Self, AudioError> {
        let sensitivity = calibration
            .usb_sens_factor_db
            .ok_or_else(|| invalid("no USB sensitivity"))?;
        Self::from_reference(sensitivity + input_gain_db, 94.0)
    }
}

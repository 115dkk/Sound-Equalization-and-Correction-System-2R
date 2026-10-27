//! REW calibration semantics:
//! <https://www.roomeqwizard.com/help/help_en-GB/html/calfiles.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/inputcal.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/splmeter.html>
//! <https://www.roomeqwizard.com/help/help_en-GB/html/graph_impulse.html>
//!
//! Files describe measured response, not its inverse. REW subtracts that
//! response for frequency/SPL analysis, but does NOT merge it into the IR by
//! default. Stored IRs stay unchanged; `ResponseCorrection` removes attached
//! curves from a copy for calculation and display (ADR 0015).
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationIssue {
    TooLarge,
    Header,
    Row,
    Order,
    Range,
    TooFewPoints,
    BothConventions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("calibration {issue:?} at line {line:?}")]
pub struct CalibrationError {
    pub issue: CalibrationIssue,
    /// One-based text line or point index; `None` for file-wide issues.
    pub line: Option<usize>,
}

// A named conversion, not `From`: a second `From<_> for AudioError` makes
// `?` on closures returning `Result<_, AudioError>` ambiguous downstream
// (secs-sys's cpal playback loop).
fn audio_error(error: CalibrationError) -> AudioError {
    match error.issue {
        CalibrationIssue::TooLarge => invalid("calibration file too large"),
        CalibrationIssue::TooFewPoints | CalibrationIssue::BothConventions => {
            invalid("calibration needs two ordered points and one sensitivity convention")
        }
        _ => invalid(&format!(
            "invalid calibration line {}",
            error.line.unwrap_or_default()
        )),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MicCalibration {
    points: Vec<CalibrationPoint>,
    usb_sens_factor_db: Option<f64>,
    sensitivity_dbfs_at_94: Option<f64>,
    serial: Option<String>,
    analog_gain_db: Option<f64>,
    notes: Vec<String>,
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
    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }
    pub fn analog_gain_db(&self) -> Option<f64> {
        self.analog_gain_db
    }
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
    pub fn has_phase(&self) -> bool {
        self.points.iter().any(|p| p.phase_degrees.is_some())
    }

    /// Strict numeric rows with REW headings, comments and quoted USB metadata.
    /// Malformed numeric rows and sensitivity headers remain errors.
    pub fn parse(text: &str) -> Result<Self, AudioError> {
        Self::parse_checked(text).map_err(audio_error)
    }

    /// Like `parse`, with a structured issue and a one-based offending line.
    pub fn parse_checked(text: &str) -> Result<Self, CalibrationError> {
        if text.len() > 4 * 1024 * 1024 {
            return Err(CalibrationError {
                issue: CalibrationIssue::TooLarge,
                line: None,
            });
        }
        let mut out = Self {
            points: Vec::new(),
            usb_sens_factor_db: None,
            sensitivity_dbfs_at_94: None,
            serial: None,
            analog_gain_db: None,
            notes: Vec::new(),
        };
        for (line_number, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}').trim();
            if line.is_empty() || ["#", ";", "//", "*"].iter().any(|p| line.starts_with(p)) {
                continue;
            }
            let quoted = line.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
            let line = if let Some(metadata) = quoted {
                if !metadata.starts_with("Sens Factor") {
                    if out.notes.len() < 8 {
                        out.notes.push(metadata.chars().take(200).collect());
                    }
                    continue;
                }
                metadata
            } else {
                line
            };
            let error = |issue| CalibrationError {
                issue,
                line: Some(line_number + 1),
            };
            if let Some(rest) = line.strip_prefix("Sens Factor") {
                let header_error = || error(CalibrationIssue::Header);
                let mut fields = rest.split(',');
                let value =
                    parse_header_db(fields.next().unwrap_or_default()).ok_or_else(header_error)?;
                if out.usb_sens_factor_db.replace(value).is_some() {
                    return Err(header_error());
                }
                for field in fields.map(str::trim) {
                    if let Some(rest) = field.strip_prefix("AGain") {
                        out.analog_gain_db = Some(parse_header_db(rest).ok_or_else(header_error)?);
                    } else if let Some(rest) = field.strip_prefix("SERNO") {
                        let serial = rest.trim().strip_prefix(':').ok_or_else(header_error)?;
                        out.serial = Some(serial.trim().chars().take(64).collect());
                    }
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("Sensitivity") {
                let header_error = || error(CalibrationIssue::Header);
                let value: f64 = rest
                    .trim()
                    .strip_suffix("dBFS")
                    .ok_or_else(header_error)?
                    .trim()
                    .parse()
                    .map_err(|_| header_error())?;
                if !value.is_finite()
                    || value > 0.0
                    || out.sensitivity_dbfs_at_94.replace(value).is_some()
                {
                    return Err(header_error());
                }
                continue;
            }
            if !line.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.')) {
                continue;
            }
            let line = line.split(['#', ';']).next().unwrap_or_default();
            let columns: Vec<_> = line
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .collect();
            if !(2..=3).contains(&columns.len()) {
                return Err(error(CalibrationIssue::Row));
            }
            let numbers = columns
                .iter()
                .map(|x| x.parse::<f64>().map_err(|_| error(CalibrationIssue::Row)))
                .collect::<Result<Vec<_>, _>>()?;
            let point = CalibrationPoint {
                frequency: numbers[0],
                gain_db: numbers[1],
                phase_degrees: numbers.get(2).copied(),
            };
            validate_point(&point, out.points.last(), line_number + 1)?;
            out.points.push(point);
        }
        if out.points.len() < 2 {
            return Err(CalibrationError {
                issue: CalibrationIssue::TooFewPoints,
                line: None,
            });
        }
        if out.usb_sens_factor_db.is_some() && out.sensitivity_dbfs_at_94.is_some() {
            return Err(CalibrationError {
                issue: CalibrationIssue::BothConventions,
                line: None,
            });
        }
        Ok(out)
    }

    /// An embedded REW curve, with the same point rules as `parse` and no headers.
    /// Errors identify the offending point by its one-based index.
    pub fn from_points(points: Vec<CalibrationPoint>) -> Result<Self, CalibrationError> {
        for (i, point) in points.iter().enumerate() {
            validate_point(point, i.checked_sub(1).map(|j| &points[j]), i + 1)?;
        }
        if points.len() < 2 {
            return Err(CalibrationError {
                issue: CalibrationIssue::TooFewPoints,
                line: None,
            });
        }
        Ok(Self {
            points,
            usb_sens_factor_db: None,
            sensitivity_dbfs_at_94: None,
            serial: None,
            analog_gain_db: None,
            notes: Vec::new(),
        })
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

    /// Log-frequency interpolation; absent phase values contribute zero.
    fn phase_degrees(&self, frequency: f64) -> f64 {
        let i = self.points.partition_point(|p| p.frequency < frequency);
        if i == 0 {
            return self.points[0].phase_degrees.unwrap_or_default();
        }
        if i == self.points.len() {
            return self.points[i - 1].phase_degrees.unwrap_or_default();
        }
        let a = self.points[i - 1];
        let b = self.points[i];
        let t = (frequency / a.frequency).ln() / (b.frequency / a.frequency).ln();
        (1.0 - t) * a.phase_degrees.unwrap_or_default() + t * b.phase_degrees.unwrap_or_default()
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

fn parse_header_db(text: &str) -> Option<f64> {
    let value: f64 = text
        .trim()
        .strip_prefix('=')?
        .trim()
        .strip_suffix("dB")?
        .trim()
        .parse()
        .ok()?;
    value.is_finite().then_some(value)
}

fn validate_point(
    point: &CalibrationPoint,
    previous: Option<&CalibrationPoint>,
    line: usize,
) -> Result<(), CalibrationError> {
    let issue = if !point.frequency.is_finite()
        || !point.gain_db.is_finite()
        || point.phase_degrees.is_some_and(|phase| !phase.is_finite())
    {
        CalibrationIssue::Row
    } else if point.frequency <= 0.0 || previous.is_some_and(|p| p.frequency >= point.frequency) {
        CalibrationIssue::Order
    } else if point.gain_db.abs() > 200.0 {
        CalibrationIssue::Range
    } else {
        return Ok(());
    };
    Err(CalibrationError {
        issue,
        line: Some(line),
    })
}

/// The most a correction raises any frequency, as REW limits calibration boost.
pub const MAX_CORRECTION_BOOST_DB: f64 = 20.0;

/// IEC 61672-1 C weighting in dB (0 at 1 kHz within 0.01 dB; -inf at 0 Hz).
pub fn c_weighting_db(frequency: f64) -> f64 {
    // Rc(f) = 12194^2 f^2 / ((f^2 + 20.6^2)(f^2 + 12194^2)).
    // Ratios avoid the fourth-power intermediate; +0.06 dB normalizes at 1 kHz.
    0.06 - 20.0
        * ((1.0 + (20.6 / frequency).powi(2)).log10()
            + (1.0 + (frequency / 12_194.0).powi(2)).log10())
}

/// Attached microphone and soundcard responses, plus optional C weighting,
/// removed from a measurement for calculation without changing its stored IR.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResponseCorrection {
    meter: Option<MicCalibration>,
    others: Vec<MicCalibration>,
    c_weighted: bool,
}

impl ResponseCorrection {
    /// `meter` is the mic or SPL meter's curve (REW `meterCal`), `others` the
    /// rest (`scCal`). For a C-weighted SPL meter REW compensates C weighting
    /// only outside the frequency range of the meter's file, and everywhere
    /// when there is none (calfiles.html).
    pub fn new(
        meter: Option<MicCalibration>,
        others: Vec<MicCalibration>,
        c_weighted: bool,
    ) -> Self {
        Self {
            meter,
            others,
            c_weighted,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.meter.is_none() && self.others.is_empty() && !self.c_weighted
    }

    /// Summed response to remove: gain in dB and phase in degrees.
    /// Curves without phase contribute zero phase; headers do not affect gain.
    pub fn response(&self, frequency: f64) -> (f64, f64) {
        let mut gain = 0.0;
        let mut phase = 0.0;
        for curve in self.meter.iter().chain(&self.others) {
            gain += curve.gain_db(frequency);
            phase += curve.phase_degrees(frequency);
        }
        let outside_meter = self.meter.as_ref().is_none_or(|meter| {
            let points = meter.points();
            frequency < points[0].frequency || frequency > points[points.len() - 1].frequency
        });
        if self.c_weighted && outside_meter {
            gain += c_weighting_db(frequency);
        }
        (gain, phase)
    }

    /// Remove the response with a twice-length, zero-padded f64 FFT, then crop
    /// back to the input length. An empty correction copies valid IRs bit-exactly.
    /// Empty IRs, a zero sample rate and nonfinite samples are errors.
    pub fn apply(&self, ir: &[f32], sample_rate: u32) -> Result<Vec<f32>, AudioError> {
        if ir.is_empty() || sample_rate == 0 || !ir.iter().all(|x| x.is_finite()) {
            return Err(invalid(
                "invalid response correction samples or sample rate",
            ));
        }
        if self.is_empty() {
            return Ok(ir.to_vec());
        }
        let n = ir
            .len()
            .checked_mul(2)
            .ok_or_else(|| invalid("response correction IR too large"))?;
        let mut padded = vec![0.0; n];
        for (out, &sample) in padded.iter_mut().zip(ir) {
            *out = f64::from(sample);
        }
        let mut spectrum = fft::rfft(&padded);
        for (k, bin) in spectrum.iter_mut().enumerate() {
            let frequency = k as f64 * f64::from(sample_rate) / n as f64;
            let (gain, phase) = self.response(frequency);
            let correction_db = (-gain).min(MAX_CORRECTION_BOOST_DB);
            let phase = if k == 0 || k == n / 2 {
                0.0
            } else {
                -phase.to_radians()
            };
            *bin *= fft::Complex64::from_polar(10_f64.powf(correction_db / 20.0), phase);
        }
        Ok(fft::irfft(&spectrum, n)
            .into_iter()
            .take(ir.len())
            .map(|x| x as f32)
            .collect())
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

//! Corrected phase and cancellation checks between main-channel pairs (ADR 0035).

use crate::Note;
use secs_dsp::Complex64;

/// ADR 0035. Inspection range in hertz.
pub const BAND_HZ: (f64, f64) = (40.0, 200.0);
/// One-third-octave centre frequencies in the inspection range.
pub const CENTERS_HZ: [f64; 6] = [50.0, 63.0, 80.0, 100.0, 125.0, 160.0];
/// Cancellation threshold in decibels.
pub const LOSS_DB: f64 = -6.0;
/// Maximum number of reported pairs.
pub const MAX_PAIRS: usize = 6;
/// Lower bound for cancellation values in decibels.
pub const FLOOR_DB: f64 = -30.0;

/// One band's `(low Hz, high Hz, phase degrees, cancellation dB)` result.
pub type Band = (f64, f64, f64, f64);

/// Measure phase difference and cancellation in each one-third-octave band.
///
/// Phase is the angle of `sum(a * conj(b))` in degrees in `(-180, 180]`.
/// Cancellation is
/// `10 * log10(sum(|a + b|^2) / sum((|a| + |b|)^2))`, bounded below by
/// [`FLOOR_DB`]. Bands without a bin in all three inputs are omitted.
pub fn bands(f: &[f64], a: &[Complex64], b: &[Complex64]) -> Vec<Band> {
    let edge = 2.0_f64.powf(1.0 / 6.0);
    CENTERS_HZ
        .iter()
        .filter_map(|&center| {
            let lo = (center / edge).max(BAND_HZ.0);
            let hi = (center * edge).min(BAND_HZ.1);
            let bins = f
                .iter()
                .zip(a)
                .zip(b)
                .filter(|((frequency, _), _)| lo <= **frequency && **frequency < hi)
                .map(|((_, a), b)| (*a, *b))
                .collect::<Vec<_>>();
            if bins.is_empty() {
                return None;
            }

            let cross = bins.iter().map(|(a, b)| *a * b.conj()).sum::<Complex64>();
            let mut phase_deg = cross.arg().to_degrees();
            if phase_deg <= -180.0 {
                phase_deg = 180.0;
            }

            let combined = bins.iter().map(|(a, b)| (*a + *b).norm_sqr()).sum::<f64>();
            let separate = bins
                .iter()
                .map(|(a, b)| (a.norm() + b.norm()).powi(2))
                .sum::<f64>();
            let loss_db = if separate == 0.0 {
                0.0
            } else {
                (10.0 * (combined / separate).log10()).max(FLOOR_DB)
            };
            Some((lo, hi, phase_deg, loss_db))
        })
        .collect()
}

/// Report main pairs with at least one band at or below [`LOSS_DB`].
///
/// `responses` is in design role order. Reports are sorted by each pair's
/// worst cancellation, then by design order, and limited to [`MAX_PAIRS`].
pub fn check(f: &[f64], responses: &[(String, Vec<Complex64>)]) -> Vec<Note> {
    let mut candidates = Vec::new();
    for left in 0..responses.len() {
        for right in left + 1..responses.len() {
            let affected = bands(f, &responses[left].1, &responses[right].1)
                .into_iter()
                .filter(|band| band.3 <= LOSS_DB)
                .collect::<Vec<_>>();
            if let Some(worst) = affected.iter().map(|band| band.3).min_by(f64::total_cmp) {
                candidates.push((worst, left, right, affected));
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    candidates
        .into_iter()
        .take(MAX_PAIRS)
        .map(|(_, left, right, bands)| Note::PairPhase {
            a: responses[left].0.clone(),
            b: responses[right].0.clone(),
            bands,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn response(phase_deg: f64) -> Vec<Complex64> {
        CENTERS_HZ
            .iter()
            .map(|_| Complex64::from_polar(1.0, -phase_deg * PI / 180.0))
            .collect()
    }

    fn assert_all_close(values: &[Band], phase_deg: f64, loss_db: f64) {
        assert_eq!(values.len(), CENTERS_HZ.len());
        for band in values {
            assert!((band.2 - phase_deg).abs() < 1e-10, "{band:?}");
            assert!((band.3 - loss_db).abs() < 0.03, "{band:?}");
        }
    }

    #[test]
    fn pair_phase_bands_measure_phase_difference_and_loss() {
        let same = response(0.0);
        assert_all_close(&bands(&CENTERS_HZ, &same, &same), 0.0, 0.0);
        assert_all_close(
            &bands(&CENTERS_HZ, &same, &response(90.0)),
            90.0,
            -3.010_299_956_639_812,
        );
        assert_all_close(
            &bands(&CENTERS_HZ, &same, &response(120.0)),
            120.0,
            -6.020_599_913_279_624,
        );
        assert_all_close(
            &bands(&CENTERS_HZ, &same, &response(180.0)),
            180.0,
            FLOOR_DB,
        );
    }

    #[test]
    fn pair_phase_check_reports_only_cancelling_pairs_worst_first() {
        let responses = vec![
            ("A".into(), response(0.0)),
            ("B".into(), response(121.0)),
            ("C".into(), response(60.0)),
        ];
        let notes = check(&CENTERS_HZ, &responses);
        assert!(matches!(
            notes.as_slice(),
            [Note::PairPhase { a, b, bands }]
                if a == "A" && b == "B" && bands.len() == CENTERS_HZ.len()
        ));

        let responses = [0.0, 30.0, 60.0, 180.0, 210.0, 240.0]
            .into_iter()
            .enumerate()
            .map(|(index, phase)| (format!("R{index}"), response(phase)))
            .collect::<Vec<_>>();
        let notes = check(&CENTERS_HZ, &responses);
        assert_eq!(notes.len(), MAX_PAIRS);
        let worst = notes
            .iter()
            .map(|note| match note {
                Note::PairPhase { bands, .. } => bands
                    .iter()
                    .map(|band| band.3)
                    .min_by(f64::total_cmp)
                    .unwrap(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert!(worst.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(worst.iter().take(3).all(|value| *value == FLOOR_DB));
    }
}

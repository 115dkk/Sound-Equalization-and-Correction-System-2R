//! Relative main-speaker polarity check (ADR 0033).

use crate::{MeasurementSet, Note, Samples, levels, measurement};
use secs_dsp::{Complex64, fft};
use std::collections::VecDeque;
use std::f64::consts::{PI, SQRT_2};

/// ADR 0033. Relative-polarity band in hertz.
pub const BAND_HZ: (f64, f64) = (300.0, 3000.0);
/// Direct-sound window before and after the peak, in milliseconds.
pub const WINDOW_MS: (f64, f64) = (1.5, 4.5);
/// Search length after onset, in milliseconds.
pub const PEAK_SEARCH_MS: f64 = 5.0;
/// Correlation lag search radius, in milliseconds.
pub const LAG_MS: f64 = 1.0;
/// Minimum decided absolute weighted correlation.
pub const DECISION: f64 = 0.5;

const FFT_LEN: usize = 4096;

fn samples(milliseconds: f64, sample_rate: u32) -> usize {
    (milliseconds * f64::from(sample_rate) / 1000.0).round() as usize
}

fn direct_window(ir: &Samples, sample_rate: u32) -> Option<Vec<f64>> {
    if ir.is_empty() || sample_rate == 0 {
        return None;
    }
    let onset = measurement::onset_index(ir);
    let search_end = onset
        .saturating_add(samples(PEAK_SEARCH_MS, sample_rate))
        .min(ir.len().saturating_sub(1));
    let peak = (onset..=search_end)
        .max_by(|&left, &right| ir.at(left).abs().total_cmp(&ir.at(right).abs()))?;
    let before = samples(WINDOW_MS.0, sample_rate);
    let length = samples(WINDOW_MS.0 + WINDOW_MS.1, sample_rate);
    if length == 0 || length > FFT_LEN {
        return None;
    }
    let start = peak as isize - before as isize;
    let mut out = vec![0.0; FFT_LEN];
    for (offset, value) in out[..length].iter_mut().enumerate() {
        let index = start + offset as isize;
        if index >= 0 && (index as usize) < ir.len() {
            *value = ir.at(index as usize);
        }
    }
    let taper = ((length as f64 * 0.1).round() as usize).max(1);
    for index in 0..taper.min(length) {
        let gain = 0.5 * (1.0 - (PI * index as f64 / taper as f64).cos());
        out[index] *= gain;
        out[length - 1 - index] *= gain;
    }
    Some(out)
}

fn band_weight(frequency: f64) -> f64 {
    let lower_zero = BAND_HZ.0 / SQRT_2;
    let upper_zero = BAND_HZ.1 * SQRT_2;
    if !(lower_zero..=upper_zero).contains(&frequency) {
        0.0
    } else if frequency < BAND_HZ.0 {
        let phase = (frequency / lower_zero).log2() * 2.0 * PI;
        0.5 * (1.0 - phase.cos())
    } else if frequency <= BAND_HZ.1 {
        1.0
    } else {
        let phase = (frequency / BAND_HZ.1).log2() * 2.0 * PI;
        0.5 * (1.0 + phase.cos())
    }
}

fn weighted_energy(spectrum: &[Complex64], weights: &[f64]) -> f64 {
    spectrum
        .iter()
        .zip(weights)
        .enumerate()
        .map(|(index, (value, weight))| {
            let multiplicity = if index == 0 || index * 2 == FFT_LEN {
                1.0
            } else {
                2.0
            };
            multiplicity * value.norm_sqr() * weight
        })
        .sum::<f64>()
        / FFT_LEN as f64
}

/// Return the strongest normalized band-limited direct-sound correlation within
/// ±[`LAG_MS`], or `None` when either response cannot supply a usable window.
pub fn pair_correlation(a: &Samples, b: &Samples, sample_rate: u32) -> Option<f64> {
    let a = fft::rfft(&direct_window(a, sample_rate)?);
    let b = fft::rfft(&direct_window(b, sample_rate)?);
    let weights = (0..a.len())
        .map(|index| band_weight(index as f64 * f64::from(sample_rate) / FFT_LEN as f64))
        .collect::<Vec<_>>();
    let energy = (weighted_energy(&a, &weights) * weighted_energy(&b, &weights)).sqrt();
    if !energy.is_finite() || energy <= f64::EPSILON {
        return None;
    }
    let cross = a
        .iter()
        .zip(&b)
        .zip(&weights)
        .map(|((a, b), weight)| a * b.conj() * *weight)
        .collect::<Vec<_>>();
    let correlation = fft::irfft(&cross, FFT_LEN);
    let lag = samples(LAG_MS, sample_rate).min(FFT_LEN / 2);
    (-isize::try_from(lag).ok()?..=isize::try_from(lag).ok()?)
        .map(|offset| {
            let index = if offset < 0 {
                FFT_LEN - offset.unsigned_abs()
            } else {
                offset as usize
            };
            correlation[index] / energy
        })
        .max_by(|left, right| left.abs().total_cmp(&right.abs()))
        .map(|value| value.clamp(-1.0, 1.0))
}

fn pair_results(
    set: &MeasurementSet,
    mains: &[String],
    main_position: u32,
) -> Vec<Vec<Option<f64>>> {
    let mut result = vec![vec![None; mains.len()]; mains.len()];
    for left in 0..mains.len() {
        for right in left + 1..mains.len() {
            let mut total = 0.0;
            let mut total_weight = 0.0;
            for (position, _) in &set.positions {
                let (Some(a), Some(b)) = (
                    set.get(*position, &mains[left]),
                    set.get(*position, &mains[right]),
                ) else {
                    continue;
                };
                if a.sample_rate != b.sample_rate {
                    continue;
                }
                if let Some(value) = pair_correlation(&a.samples, &b.samples, a.sample_rate) {
                    let weight = levels::weights(&[*position], main_position)[0];
                    total += weight * value;
                    total_weight += weight;
                }
            }
            if total_weight > 0.0 {
                let average = total / total_weight;
                if average.abs() >= DECISION {
                    result[left][right] = Some(average);
                    result[right][left] = Some(average);
                }
            }
        }
    }
    result
}

/// One connected set of mains joined by decided pairs, with the sign each
/// main gets from the root; `None` when the decided signs contradict each
/// other (a triangle of negative pairs), in which case no verdict is given.
fn component(
    pairs: &[Vec<Option<f64>>],
    root: usize,
    seen: &mut [bool],
) -> Option<(Vec<usize>, Vec<i8>)> {
    let mut signs = vec![0_i8; pairs.len()];
    signs[root] = 1;
    seen[root] = true;
    let mut members = vec![root];
    let mut queue = VecDeque::from([root]);
    let mut consistent = true;
    while let Some(left) = queue.pop_front() {
        for right in 0..pairs.len() {
            let Some(value) = pairs[left][right] else {
                continue;
            };
            let expected = if value < 0.0 {
                -signs[left]
            } else {
                signs[left]
            };
            if signs[right] == 0 {
                signs[right] = expected;
                seen[right] = true;
                members.push(right);
                queue.push_back(right);
            } else if signs[right] != expected {
                consistent = false;
            }
        }
    }
    members.sort_unstable();
    consistent.then_some((members, signs))
}

/// Check relative polarity among main channels after configured inversions.
pub fn check(set: &MeasurementSet, mains: &[String], main_position: u32) -> Vec<Note> {
    if mains.len() < 2 {
        return Vec::new();
    }
    let pairs = pair_results(set, mains, main_position);
    // Each set of mains joined by decided pairs is judged on its own: a main
    // nobody could correlate with that set carries no vote on it. Inside a
    // set the decided signs split the mains in two; the smaller side is the
    // reversed one, and equal sides leave the choice open.
    let mut seen = vec![false; mains.len()];
    let mut notes = Vec::new();
    for root in 0..mains.len() {
        if seen[root] || !pairs[root].iter().any(Option::is_some) {
            continue;
        }
        let Some((members, signs)) = component(&pairs, root, &mut seen) else {
            continue;
        };
        let first_sign = signs[members[0]];
        let (a, b): (Vec<usize>, Vec<usize>) = members
            .iter()
            .copied()
            .partition(|&index| signs[index] == first_sign);
        if b.is_empty() {
            continue;
        }
        let reversed = if a.len() < b.len() {
            Some(&a)
        } else if b.len() < a.len() {
            Some(&b)
        } else {
            None
        };
        if let Some(reversed) = reversed {
            notes.extend(reversed.iter().map(|&index| {
                let values = pairs[index].iter().flatten().copied().collect::<Vec<_>>();
                Note::PolarityReversed {
                    role: mains[index].clone(),
                    confidence: levels::median(values.iter().map(|value| value.abs()).collect()),
                    compared: values.len(),
                    opposite: values.iter().filter(|value| **value < 0.0).count(),
                }
            }));
            continue;
        }
        let confidence = members
            .iter()
            .enumerate()
            .flat_map(|(k, &left)| members[k + 1..].iter().map(move |&right| (left, right)))
            .filter(|(left, right)| signs[*left] != signs[*right])
            .filter_map(|(left, right)| pairs[left][right].map(f64::abs))
            .collect::<Vec<_>>();
        notes.push(Note::PolaritySplit {
            groups: (
                a.iter().map(|&index| mains[index].clone()).collect(),
                b.iter().map(|&index| mains[index].clone()).collect(),
            ),
            confidence: levels::median(confidence),
        });
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Measurement;
    use std::sync::Arc;

    fn pulse(sign: f64, delay: usize) -> Samples {
        let mut values = vec![0.0; 1024];
        let shape = [1.0, 0.8, 0.64, 0.45, 0.3, 0.18, 0.1];
        for (index, value) in shape.into_iter().enumerate() {
            values[256 + delay + index] = sign * value;
        }
        Samples::F64(values.into())
    }

    fn measurement(samples: Samples) -> Measurement {
        Measurement {
            samples,
            sample_rate: 48_000,
            t0: 0.0,
        }
    }

    fn set(roles: &[(&str, f64)]) -> MeasurementSet {
        MeasurementSet {
            positions: vec![(
                1,
                roles
                    .iter()
                    .map(|(role, sign)| ((*role).into(), measurement(pulse(*sign, 0))))
                    .collect(),
            )],
        }
    }

    #[test]
    fn polarity_pair_correlation_finds_sign_and_confidence() {
        let base = pulse(1.0, 0);
        let same = pulse(1.0, 24);
        let opposite = pulse(-1.0, 24);
        // The normalization is exact: a response against itself is +1.
        assert!((pair_correlation(&base, &base, 48_000).unwrap() - 1.0).abs() < 1e-9);
        assert!(pair_correlation(&base, &same, 48_000).unwrap() >= 0.9);
        assert!(pair_correlation(&base, &opposite, 48_000).unwrap() <= -0.9);

        let mut spectrum = fft::rfft(&direct_window(&base, 48_000).unwrap());
        for (index, value) in spectrum.iter_mut().enumerate() {
            if index as f64 * 48_000.0 / FFT_LEN as f64 > 2000.0 {
                *value = -*value;
            }
        }
        let tweeter_reversed = Samples::F64(Arc::from(fft::irfft(&spectrum, FFT_LEN)));
        assert!(
            pair_correlation(&base, &tweeter_reversed, 48_000)
                .unwrap()
                .abs()
                >= DECISION
        );
    }

    #[test]
    fn polarity_check_flags_a_minority_and_splits_a_pair() {
        let mains = ["L", "R", "C", "Ls"].map(String::from);
        let notes = check(
            &set(&[("L", 1.0), ("R", 1.0), ("C", 1.0), ("Ls", -1.0)]),
            &mains,
            1,
        );
        assert!(matches!(
            notes.as_slice(),
            [Note::PolarityReversed { role, compared: 3, opposite: 3, .. }] if role == "Ls"
        ));

        let pair = ["L", "R"].map(String::from);
        let notes = check(&set(&[("L", 1.0), ("R", -1.0)]), &pair, 1);
        assert!(matches!(notes.as_slice(), [Note::PolaritySplit { .. }]));

        let notes = check(
            &set(&[("L", 1.0), ("R", 1.0), ("C", -1.0), ("Ls", -1.0)]),
            &mains,
            1,
        );
        assert!(
            matches!(notes.as_slice(), [Note::PolaritySplit { groups, .. }]
            if groups.0 == ["L", "R"] && groups.1 == ["C", "Ls"])
        );
    }

    #[test]
    fn polarity_check_judges_the_majority_among_decided_mains() {
        // Only L and R can be correlated: the three silent mains carry no vote,
        // so the pair is a split, not two reversed mains.
        let mains = ["L", "R", "C", "Ls", "Rs"].map(String::from);
        let silent = || Samples::F64(vec![0.0; 1024].into());
        let notes = check(
            &MeasurementSet {
                positions: vec![(
                    1,
                    vec![
                        ("L".into(), measurement(pulse(1.0, 0))),
                        ("R".into(), measurement(pulse(-1.0, 0))),
                        ("C".into(), measurement(silent())),
                        ("Ls".into(), measurement(silent())),
                        ("Rs".into(), measurement(silent())),
                    ],
                )],
            },
            &mains,
            1,
        );
        assert!(
            matches!(notes.as_slice(), [Note::PolaritySplit { groups, .. }]
            if groups.0 == ["L"] && groups.1 == ["R"])
        );

        // Two reversed mains among five: each is opposite to the three normal
        // ones and agrees with the other reversed one.
        let notes = check(
            &set(&[
                ("L", 1.0),
                ("R", 1.0),
                ("C", 1.0),
                ("Ls", -1.0),
                ("Rs", -1.0),
            ]),
            &mains,
            1,
        );
        assert!(matches!(
            notes.as_slice(),
            [
                Note::PolarityReversed { role: a, compared: 4, opposite: 3, .. },
                Note::PolarityReversed { role: b, compared: 4, opposite: 3, .. },
            ] if a == "Ls" && b == "Rs"
        ));
    }

    #[test]
    fn polarity_check_judges_each_connected_component() {
        // L and R were measured at one position, C, Ls and Rs at another, so
        // nothing joins the two sets: L against R is a split of its own, and
        // the agreeing trio says nothing about it.
        let mains = ["L", "R", "C", "Ls", "Rs"].map(String::from);
        let notes = check(
            &MeasurementSet {
                positions: vec![
                    (
                        1,
                        vec![
                            ("L".into(), measurement(pulse(1.0, 0))),
                            ("R".into(), measurement(pulse(-1.0, 0))),
                        ],
                    ),
                    (
                        2,
                        vec![
                            ("C".into(), measurement(pulse(1.0, 0))),
                            ("Ls".into(), measurement(pulse(1.0, 0))),
                            ("Rs".into(), measurement(pulse(1.0, 0))),
                        ],
                    ),
                ],
            },
            &mains,
            1,
        );
        assert!(
            matches!(notes.as_slice(), [Note::PolaritySplit { groups, .. }]
            if groups.0 == ["L"] && groups.1 == ["R"])
        );
    }

    #[test]
    fn polarity_check_withholds_a_component_with_contradicting_signs() {
        // Every pair measured at its own position and every pair opposite:
        // no assignment of signs fits, so there is no verdict.
        let mains = ["L", "R", "C"].map(String::from);
        let notes = check(
            &MeasurementSet {
                positions: vec![
                    (
                        1,
                        vec![
                            ("L".into(), measurement(pulse(1.0, 0))),
                            ("R".into(), measurement(pulse(-1.0, 0))),
                        ],
                    ),
                    (
                        2,
                        vec![
                            ("L".into(), measurement(pulse(1.0, 0))),
                            ("C".into(), measurement(pulse(-1.0, 0))),
                        ],
                    ),
                    (
                        3,
                        vec![
                            ("R".into(), measurement(pulse(1.0, 0))),
                            ("C".into(), measurement(pulse(-1.0, 0))),
                        ],
                    ),
                ],
            },
            &mains,
            1,
        );
        assert!(notes.is_empty());
    }

    #[test]
    fn polarity_check_reverses_the_smaller_sign_group_not_the_lonely_main() {
        // Decided pairs: L/R same, L/Rs opposite, Rs/Ls same, Ls/C opposite.
        // The signs split {L, R, C} from {Ls, Rs}: the pair is reversed even
        // though C is the only main whose decided pairs are all negative.
        let mains = ["L", "R", "C", "Ls", "Rs"].map(String::from);
        let position = |index: u32, pair: [(&str, f64); 2]| {
            (
                index,
                pair.iter()
                    .map(|(role, sign)| ((*role).into(), measurement(pulse(*sign, 0))))
                    .collect(),
            )
        };
        let notes = check(
            &MeasurementSet {
                positions: vec![
                    position(1, [("L", 1.0), ("R", 1.0)]),
                    position(2, [("L", 1.0), ("Rs", -1.0)]),
                    position(3, [("Rs", 1.0), ("Ls", 1.0)]),
                    position(4, [("Ls", 1.0), ("C", -1.0)]),
                ],
            },
            &mains,
            1,
        );
        assert!(matches!(
            notes.as_slice(),
            [
                Note::PolarityReversed { role: a, compared: 2, opposite: 1, .. },
                Note::PolarityReversed { role: b, compared: 2, opposite: 1, .. },
            ] if a == "Ls" && b == "Rs"
        ));
    }

    #[test]
    fn polarity_check_averages_positions_and_skips_undecided_pairs() {
        let mut seed = 7_u64;
        let mut noisy = || {
            let mut values = vec![0.0; 1024];
            for value in &mut values[250..550] {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                *value = ((seed >> 33) as f64 / u32::MAX as f64 - 0.5) * 2.0;
            }
            values
        };
        let roles = ["L", "R"].map(String::from);
        let mut positions = vec![(
            1,
            vec![
                ("L".into(), measurement(pulse(1.0, 0))),
                ("R".into(), measurement(pulse(-1.0, 0))),
            ],
        )];
        for position in 2..=4 {
            let a = pulse(1.0, 0);
            let mut b = noisy();
            for (index, value) in b.iter_mut().enumerate().take(a.len()) {
                *value += 0.15 * a.at(index);
            }
            positions.push((
                position,
                vec![
                    ("L".into(), measurement(a)),
                    ("R".into(), measurement(Samples::F64(b.into()))),
                ],
            ));
        }
        assert!(matches!(
            check(&MeasurementSet { positions }, &roles, 1).as_slice(),
            [Note::PolaritySplit { .. }]
        ));

        let undecided = MeasurementSet {
            positions: vec![(
                1,
                vec![
                    ("L".into(), measurement(pulse(1.0, 0))),
                    (
                        "R".into(),
                        measurement(Samples::F64(vec![0.0; 1024].into())),
                    ),
                ],
            )],
        };
        assert!(check(&undecided, &roles, 1).is_empty());
    }
}

//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/peaks.rs` (MIT, Copyright (c) 2024- 115dkk).
//! See docs/THIRD_PARTY.md for attribution and licenses.
//! Height-only peak selection and the 2.x impulse onset rule.

/// Mirrors scipy.signal.find_peaks(x, height=height), pinned by p05_peaks_*.json.
/// https://docs.scipy.org/doc/scipy-1.16.1/reference/generated/scipy.signal.find_peaks.html
/// Returns ascending indices, inclusive height, floor midpoint for plateaus,
/// and excludes endpoints. NaN comparisons are false, so NaNs break candidates;
/// an NaN height excludes all peaks. Infinities retain ordinary comparison rules.
pub fn find_peaks(x: &[f64], height: Option<f64>) -> Vec<usize> {
    let mut peaks = Vec::new();
    let mut i = 1;
    while i + 1 < x.len() {
        if x[i - 1] < x[i] {
            let mut right = i;
            while right + 1 < x.len() && x[right + 1] == x[i] {
                right += 1;
            }
            if right + 1 < x.len() && x[right + 1] < x[i] && height.is_none_or(|h| x[i] >= h) {
                peaks.push(i + (right - i) / 2);
            }
            i = right;
        }
        i += 1;
    }
    peaks
}

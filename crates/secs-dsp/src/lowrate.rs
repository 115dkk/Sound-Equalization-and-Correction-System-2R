//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! Upstream MIMO low-rate projection. D3 interpolation/taper is preserved.
use crate::{Complex64, DspError, fft, grid, invalid, numeric::interp};

pub fn to_low(h: &[Complex64], f: &[f64], n: usize) -> Result<Vec<f64>, DspError> {
    if n == 0 || f.is_empty() || h.len() != f.len() {
        return Err(invalid("invalid low-rate input shape"));
    }
    let re: Vec<_> = h.iter().map(|x| x.re).collect();
    let im: Vec<_> = h.iter().map(|x| x.im).collect();
    let spec: Vec<_> = grid::freqs(2 * n, 600.)?
        .iter()
        .map(|&fl| {
            (interp(fl, f, &re) + Complex64::new(0., 1.) * Complex64::new(interp(fl, f, &im), 0.))
                * ((285. - fl) / 60.).clamp(0., 1.)
        })
        .collect();
    let mut x = fft::irfft(&spec, 2 * n);
    x.truncate(n);
    Ok(x)
}

/// Row-major n × taps convolution matrix. Preserves Python slice/broadcast
/// failures when g is short or taps extends into a nonempty negative slice.
pub fn conv_matrix(g: &[f64], taps: usize, n: usize) -> Result<Vec<f64>, DspError> {
    let size = n
        .checked_mul(taps)
        .ok_or_else(|| invalid("matrix size overflow"))?;
    let mut a = vec![0.; size];
    for k in 0..taps {
        let end = if k <= n {
            (n - k).min(g.len())
        } else {
            g.len().saturating_sub(k - n)
        };
        let rows = n.saturating_sub(k);
        if end != rows && end != 1 {
            return Err(invalid(
                "convolution slice cannot broadcast into matrix column",
            ));
        }
        for r in k..n {
            a[r * taps + k] = g[if end == 1 { 0 } else { r - k }];
        }
    }
    Ok(a)
}

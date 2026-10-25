//! The mode name and the ASCII header note that the CamillaDSP and Equalizer
//! APO exports write, built here once for the service and the tests (audit
//! #92 C2). Upstream builds them in `calibration.py` (`export_camilladsp`,
//! `_filter_plan`, `export_eqapo`).

use secs_types::calibration::{Mode, mode_tag};
use secs_types::chain::MimoPaths;

use crate::pyfmt::{format_fixed, format_g};

/// The `<name>` in `secs_<name>`: the mode tag, with the ultra-mode budget
/// appended (`ultra200`).
pub fn mode_name(mode: Mode, ultra_ms: f64) -> String {
    let tag = mode_tag(mode);
    if mode == Mode::Ultra {
        format!("{tag}{}", format_g(ultra_ms, false))
    } else {
        tag.into()
    }
}

/// What a header note says about the MIMO support paths an export carries.
#[derive(Debug, Clone, PartialEq)]
pub struct MimoNote {
    /// `FIR` when any path is a FIR, otherwise `IIR`.
    pub fir: bool,
    /// Number of support paths.
    pub paths: usize,
    /// Support crossover frequency.
    pub fsiso: f64,
    /// Per-input support cap the run used, 0 for none (upstream
    /// `MimoRun.max_supports`).
    pub max_supports: usize,
}

impl MimoNote {
    /// The note for these paths; `None` when there are none, which the notes
    /// write as `no MIMO`.
    pub fn for_paths(paths: &MimoPaths, max_supports: usize) -> Option<Self> {
        (!paths.paths.is_empty()).then(|| Self {
            fir: paths.paths.iter().any(|path| path.filter.is_fir()),
            paths: paths.paths.len(),
            fsiso: paths.fsiso,
            max_supports,
        })
    }

    fn text(&self, with_cap: bool) -> String {
        let kind = if self.fir { "FIR" } else { "IIR" };
        let cap = if with_cap && self.max_supports != 0 {
            format!(", max {} per input", self.max_supports)
        } else {
            String::new()
        };
        format!(
            "MIMO {kind} support paths {} (Fsiso {} Hz{cap})",
            self.paths,
            format_g(self.fsiso, false)
        )
    }
}

/// CamillaDSP's note names the plain mode tag and the per-input support cap.
pub fn camilla_note(mode: Mode, latency_ms: f64, rc_hi_hz: f64, mimo: Option<&MimoNote>) -> String {
    note(
        mode_tag(mode),
        latency_ms,
        rc_hi_hz,
        mimo.map(|mimo| mimo.text(true)),
    )
}

/// Equalizer APO's note names the mode name (with the ultra budget) and no
/// support cap.
pub fn eqapo_note(
    mode_name: &str,
    latency_ms: f64,
    rc_hi_hz: f64,
    mimo: Option<&MimoNote>,
) -> String {
    note(
        mode_name,
        latency_ms,
        rc_hi_hz,
        mimo.map(|mimo| mimo.text(false)),
    )
}

fn note(mode: &str, latency_ms: f64, rc_hi_hz: f64, mimo: Option<String>) -> String {
    format!(
        "mode {mode}, total latency {} ms, RC upper {} Hz; {}",
        format_fixed(latency_ms, 1, false),
        format_fixed(rc_hi_hz, 0, false),
        mimo.as_deref().unwrap_or("no MIMO")
    )
}

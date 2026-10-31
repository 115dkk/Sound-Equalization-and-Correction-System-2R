use secs_camilla::RunLabels;
use secs_camilla::note::{self, MimoNote};
use secs_export::{ExportRun, MimoKind, MimoResponses};
use secs_filters::FilterRun;
use secs_project::Project;
use secs_types::calibration::{CalibrationSettings, Mode, RcBand, mode_label, mode_tag};
use secs_types::chain::{MimoPaths, SignalChain};
use secs_types::ipc::{ErrorCode, IpcError};

use crate::LocalTime;
use crate::compute::ComputeIdentity;
use crate::results::{MimoView, ResultBook, RunView};

#[cfg(test)]
#[path = "export_run_tests.rs"]
mod tests;

/// Names and annotations resolved once, independently of live/cached storage.
#[derive(Debug, PartialEq)]
pub(crate) struct ExportDescription {
    mode: Mode,
    mode_name: String,
    camilla_note: String,
    filter_note: String,
    ultra_ms: f64,
    label: String,
    latency_ms: f64,
    rc_hi_hz: f64,
    rc_bands: Vec<RcBand>,
    rc_pieces: Vec<String>,
    max_supports: usize,
    mimo: Option<MimoDescription>,
    stamp: String,
}

#[derive(Debug, PartialEq)]
struct MimoDescription {
    kind: MimoKind,
    fsiso: f64,
    supports: usize,
    cap_text: String,
    /// What the CamillaDSP and Equalizer APO header notes say.
    note: MimoNote,
}

pub(crate) fn describe(
    book: &ResultBook,
    project: &Project,
    compute: &ComputeIdentity,
    now: LocalTime,
) -> Result<ExportDescription, IpcError> {
    let settings = secs_project::calibration_from_project(project);
    let mut description = match book.calibration(project).ok_or_else(|| {
        IpcError::new(ErrorCode::EngineInput, "calibration must run before export")
    })? {
        RunView::Live { run, .. } => {
            let design = run.design();
            ExportDescription {
                mode: run.settings.mode,
                mode_name: String::new(),
                camilla_note: String::new(),
                filter_note: String::new(),
                ultra_ms: run.settings.ultra_ms,
                label: mode_label(run.settings.mode, Some(run.settings.ultra_ms)),
                latency_ms: design.chain.latency_ms,
                rc_hi_hz: design.rc_hi,
                rc_bands: run.settings.rc_bands.clone(),
                rc_pieces: secs_export::rc_pieces(&ExportRun {
                    design,
                    settings: &run.settings,
                }),
                max_supports: run.settings.mimo_max_supports.max(0) as usize,
                mimo: None,
                stamp: now.minute_stamp(),
            }
        }
        RunView::Cached { cache, .. } => {
            let rc_values = cache
                .calibration
                .notes
                .iter()
                .find(|notice| notice.code == "rc_limit")
                .and_then(|notice| {
                    Some((
                        notice.params.get("hz")?.parse::<f64>().ok()?,
                        notice.params.get("auto")?.parse::<bool>().ok()?,
                        notice
                            .params
                            .get("schroeder_hz")
                            .and_then(|value| value.parse::<f64>().ok()),
                    ))
                });
            let (rc_hi_hz, rc_auto, schroeder_hz, legacy_fallback) = match rc_values {
                Some((hz, auto, schroeder)) => (hz, auto, schroeder, false),
                None => (cache.labels.rc_hi_hz, settings.rc_hi.is_none(), None, true),
            };
            ExportDescription {
                mode: cache.calibration.mode,
                mode_name: String::new(),
                camilla_note: String::new(),
                filter_note: String::new(),
                ultra_ms: cache.labels.ultra_ms,
                label: mode_label(cache.calibration.mode, Some(cache.calibration.ultra_ms)),
                latency_ms: cache.labels.latency_ms,
                rc_hi_hz,
                rc_bands: cache.labels.rc_bands.clone(),
                rc_pieces: {
                    // The run's optional correction switches come from the cache labels:
                    // the stored settings may have changed since the run.
                    let mut pieces = secs_export::rc_pieces_from_values_with_bands(
                        rc_hi_hz,
                        rc_auto,
                        schroeder_hz,
                        settings.room_volume_m3,
                        legacy_fallback,
                        &cache.labels.rc_bands,
                    );
                    pieces.extend(secs_export::lf_correction_pieces(&CalibrationSettings {
                        mode: cache.calibration.mode,
                        lf_align: cache.labels.lf_align,
                        mode_decay: cache.labels.mode_decay,
                        hf_phase: cache.labels.hf_phase,
                        ..CalibrationSettings::default()
                    }));
                    pieces
                },
                max_supports: cache.labels.mimo_max_supports,
                mimo: None,
                stamp: now.minute_stamp(),
            }
        }
    };
    description.mimo = book.mimo(project, compute).exported().map(|view| {
        let (paths, kind, max_supports) = match view {
            MimoView::Live(result) => (
                &result.run.paths,
                if result.run.fir {
                    MimoKind::Fir
                } else {
                    MimoKind::Iir
                },
                result.run.key.max_supports,
            ),
            MimoView::Cached(cache) => {
                let paths = cache.mimo_paths.as_ref().expect("exported MIMO has paths");
                (
                    paths,
                    if paths.paths.iter().any(|path| path.filter.is_fir()) {
                        MimoKind::Fir
                    } else {
                        MimoKind::Iir
                    },
                    secs_engine::MimoKey::from_settings(&settings).max_supports,
                )
            }
        };
        MimoDescription {
            kind,
            fsiso: paths.fsiso,
            supports: paths.paths.len(),
            cap_text: secs_engine::mimo::cap_text(max_supports),
            note: MimoNote::for_paths(paths, description.max_supports)
                .expect("an exported MIMO has support paths"),
        }
    });
    description.mode_name = note::mode_name(description.mode, description.ultra_ms);
    let mimo_note = description.mimo.as_ref().map(|mimo| &mimo.note);
    description.camilla_note = note::camilla_note_with_bands(
        description.mode,
        description.latency_ms,
        description.rc_hi_hz,
        &description.rc_bands,
        mimo_note,
    );
    description.filter_note = note::eqapo_note_with_bands(
        &description.mode_name,
        description.latency_ms,
        description.rc_hi_hz,
        &description.rc_bands,
        mimo_note,
    );
    Ok(description)
}

impl ExportDescription {
    pub(crate) fn mode(&self) -> &'static str {
        mode_tag(self.mode)
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn latency_ms(&self) -> f64 {
        self.latency_ms
    }

    pub(crate) fn camilla(&self) -> RunLabels {
        RunLabels {
            mode: self.mode_name.clone(),
            note: self.camilla_note.clone(),
            latency_ms: self.latency_ms,
        }
    }

    pub(crate) fn filters<'a>(
        &'a self,
        chain: &'a SignalChain,
        mimo: Option<&'a MimoPaths>,
    ) -> FilterRun<'a> {
        FilterRun {
            chain,
            mimo,
            mode: &self.mode_name,
            note: &self.filter_note,
            label: &self.label,
            latency_ms: self.latency_ms,
            rc_pieces: &self.rc_pieces,
            stamp: &self.stamp,
        }
    }

    pub(crate) fn mimo_responses(
        &self,
        paths: MimoPaths,
        applied: Vec<(String, Vec<secs_engine::C>)>,
    ) -> MimoResponses {
        let mimo = self.mimo.as_ref().expect("exported MIMO has a description");
        MimoResponses {
            kind: mimo.kind,
            fsiso: mimo.fsiso,
            cap_text: mimo.cap_text.clone(),
            paths,
            applied,
        }
    }
}

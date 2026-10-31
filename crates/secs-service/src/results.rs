use std::sync::Arc;

use secs_engine::Design;
use secs_project::{Project, calibration_from_project, measured_positions};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrationSummary, MimoExportDecision, MimoSummary, Notice, ResultsSummary,
};

use crate::cache::CachedResults;
use crate::calibration::{config_key, signature_key, target_key_string};
use crate::compute::ComputeIdentity;

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;

#[derive(Debug)]
pub(crate) struct CalibrationRun {
    pub summary: CalibrationSummary,
    pub settings: CalibrationSettings,
    pub signature: String,
    pub target_key: String,
    pub config_key: String,
    pub run_key: String,
    /// Positions the engine used; MIMO needs three (upstream `mimo_available`
    /// counts the run's positions, not the recorded ones).
    positions: usize,
    design: CalibrationDesign,
}

#[derive(Debug)]
enum CalibrationDesign {
    Computed(Arc<Design>),
    #[cfg(test)]
    MetadataOnly,
}

impl CalibrationRun {
    pub(crate) fn new(
        summary: CalibrationSummary,
        settings: CalibrationSettings,
        signature: String,
        target_key: String,
        config_key: String,
        run_key: String,
        design: Arc<Design>,
    ) -> Self {
        Self {
            summary,
            settings,
            signature,
            target_key,
            config_key,
            run_key,
            positions: design.spectra.positions.len(),
            design: CalibrationDesign::Computed(design),
        }
    }

    pub(crate) fn design(&self) -> &Design {
        match &self.design {
            CalibrationDesign::Computed(design) => design,
            #[cfg(test)]
            CalibrationDesign::MetadataOnly => panic!("metadata-only calibration has no design"),
        }
    }

    pub(crate) fn shared_design(&self) -> Arc<Design> {
        match &self.design {
            CalibrationDesign::Computed(design) => design.clone(),
            #[cfg(test)]
            CalibrationDesign::MetadataOnly => panic!("metadata-only calibration has no design"),
        }
    }

    #[cfg(test)]
    fn metadata_only(
        summary: CalibrationSummary,
        settings: CalibrationSettings,
        signature: String,
        target_key: String,
        config_key: String,
        run_key: String,
        positions: usize,
    ) -> Self {
        Self {
            summary,
            settings,
            signature,
            target_key,
            config_key,
            run_key,
            positions,
            design: CalibrationDesign::MetadataOnly,
        }
    }
}

#[derive(Debug)]
pub(crate) struct MimoResult {
    pub summary: MimoSummary,
    pub settings_key: String,
    pub compute_key: ComputeIdentity,
    pub calibration_key: String,
    pub run: secs_engine::MimoRun,
}

/// The results of the open project: runs computed in this session and cached
/// results carried by the project, with one rule set for staleness and MIMO
/// inclusion (ADR 0009, 0012).
#[derive(Debug, Default)]
pub(crate) struct ResultBook {
    calibration: Option<CalibrationRun>,
    /// The last subs-only run; a full run clears it.
    subs: Option<CalibrationRun>,
    mimo: Option<MimoResult>,
    cached: Option<CachedResults>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RunView<'a> {
    Live {
        run: &'a CalibrationRun,
        stale_reason: Option<&'static str>,
    },
    Cached {
        cache: &'a CachedResults,
        stale_reason: Option<&'static str>,
    },
}

impl RunView<'_> {
    pub(crate) fn stale(self) -> bool {
        match self {
            Self::Live { stale_reason, .. } | Self::Cached { stale_reason, .. } => {
                stale_reason.is_some()
            }
        }
    }

    fn summary(self) -> CalibrationSummary {
        match self {
            Self::Live { run, stale_reason } => {
                let mut summary = run.summary.clone();
                summary.stale_reason = stale_reason.map(str::to_owned);
                summary.stale = stale_reason.is_some();
                summary
            }
            Self::Cached {
                cache,
                stale_reason,
            } => {
                let mut summary = cache.calibration.clone();
                summary.cached = true;
                summary.stale_reason = stale_reason.map(str::to_owned);
                summary.stale = stale_reason.is_some();
                summary
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MimoView<'a> {
    Live(&'a MimoResult),
    Cached(&'a CachedResults),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MimoVerdict<'a> {
    pub decision: MimoExportDecision,
    pub result: Option<MimoView<'a>>,
    stale: bool,
}

impl<'a> MimoVerdict<'a> {
    /// The result the graphs and result lines draw: upstream `show_mimo`
    /// (MIMO on, computed on the current calibration, three positions),
    /// which does not need support paths.
    pub(crate) fn shown(self) -> Option<MimoView<'a>> {
        matches!(
            self.decision,
            MimoExportDecision::Included | MimoExportDecision::NoSupports
        )
        .then_some(self.result)
        .flatten()
    }

    /// The result an export writes: shown, with support paths.
    pub(crate) fn exported(self) -> Option<MimoView<'a>> {
        (self.decision == MimoExportDecision::Included)
            .then_some(self.result)
            .flatten()
    }
}

impl ResultBook {
    pub(crate) fn from_cache(cached: Option<CachedResults>) -> Self {
        Self {
            cached,
            ..Self::default()
        }
    }

    /// What `results`, the curves, the export and the CLI read.
    pub(crate) fn summary(&self, project: &Project, compute: &ComputeIdentity) -> ResultsSummary {
        let calibration = self.calibration(project).map(RunView::summary);
        let verdict = self.mimo(project, compute);
        let mimo = verdict.result.map(|view| {
            let mut summary = match view {
                MimoView::Live(result) => result.summary.clone(),
                MimoView::Cached(cache) => {
                    cache.mimo.clone().expect("cached MIMO view has summary")
                }
            };
            summary.stale = verdict.stale;
            summary
        });
        ResultsSummary {
            calibration,
            mimo,
            mimo_export: verdict.decision,
        }
    }

    /// The run an export or graph uses: live when there is one, otherwise the
    /// cached one, with its staleness reason.
    pub(crate) fn calibration(&self, project: &Project) -> Option<RunView<'_>> {
        if let Some(run) = self.calibration.as_ref() {
            return Some(RunView::Live {
                run,
                stale_reason: calibration_stale_reason(run, project),
            });
        }
        self.cached.as_ref().map(|cache| RunView::Cached {
            cache,
            stale_reason: cached_stale_reason(cache, project),
        })
    }

    /// Whether MIMO is shown and exported, and if not, why.
    pub(crate) fn mimo(&self, project: &Project, compute: &ComputeIdentity) -> MimoVerdict<'_> {
        let settings = calibration_from_project(project);
        let result = self.mimo_view();
        let calibration = self.calibration(project);
        let stale = result.is_some_and(|result| match result {
            MimoView::Live(mimo) => {
                calibration.is_none_or(RunView::stale)
                    || self
                        .calibration
                        .as_ref()
                        .is_none_or(|run| run.run_key != mimo.calibration_key)
                    || settings.mimo_key() != mimo.settings_key
                    || &mimo.compute_key != compute
            }
            MimoView::Cached(cache) => {
                calibration.is_none_or(RunView::stale)
                    || cache.mimo_key.as_deref() != Some(settings.mimo_key().as_str())
                    || cache.run_key.is_none()
                    || cache.mimo_calibration_key.as_deref() != cache.run_key.as_deref()
                    || cache.engine_key != settings.engine_key()
                    || &cache.mimo_compute_key != compute
            }
        });
        let supports = result.map_or(0, |result| match result {
            MimoView::Live(mimo) => mimo.run.paths.paths.len(),
            MimoView::Cached(cache) => cache
                .mimo_paths
                .as_ref()
                .map_or(0, |paths| paths.paths.len()),
        });
        let decision = if !settings.mimo_on {
            MimoExportDecision::Off
        } else if self
            .calibration
            .as_ref()
            .map_or_else(|| measured_positions(project).len(), |run| run.positions)
            < 3
        {
            MimoExportDecision::TooFewPositions
        } else if result.is_none() {
            MimoExportDecision::None
        } else if stale {
            MimoExportDecision::Stale
        } else if supports == 0 {
            MimoExportDecision::NoSupports
        } else {
            MimoExportDecision::Included
        };
        MimoVerdict {
            decision,
            result,
            stale,
        }
    }

    fn mimo_view(&self) -> Option<MimoView<'_>> {
        if self.calibration.is_some() {
            self.mimo.as_ref().map(MimoView::Live)
        } else {
            self.cached
                .as_ref()
                .filter(|cache| cache.mimo.is_some())
                .map(MimoView::Cached)
        }
    }

    pub(crate) fn commit_calibration(&mut self, run: CalibrationRun, subs_only: bool) {
        if subs_only {
            self.subs = Some(run);
        } else {
            self.calibration = Some(run);
            self.subs = None;
        }
    }

    pub(crate) fn commit_mimo(&mut self, result: MimoResult) {
        self.mimo = Some(result);
    }

    /// The cache to write, captured while the session is locked and serialized
    /// after that lock is released.
    pub(crate) fn cache_snapshot(&self) -> Option<CachedResults> {
        let calibration = self.calibration.as_ref()?;
        Some(CachedResults {
            calibration: CalibrationSummary {
                notes: cached_notes(&calibration.summary.notes),
                ..calibration.summary.clone()
            },
            calibration_chain: calibration.design().chain.clone(),
            mimo: self.mimo.as_ref().map(|result| MimoSummary {
                notes: cached_notes(&result.summary.notes),
                ..result.summary.clone()
            }),
            measurement_signature: calibration.signature.clone(),
            target_key: calibration.target_key.clone(),
            engine_key: calibration.settings.engine_key(),
            run_key: Some(calibration.run_key.clone()),
            config_key: Some(calibration.config_key.clone()),
            mimo_key: self.mimo.as_ref().map(|result| result.settings_key.clone()),
            mimo_calibration_key: self
                .mimo
                .as_ref()
                .map(|result| result.calibration_key.clone()),
            labels: crate::cache::CachedRunLabels {
                sample_rate_policy: calibration.settings.sample_rate_policy,
                mode: match calibration.settings.mode {
                    secs_types::calibration::Mode::Zero => "zero",
                    secs_types::calibration::Mode::Mixed => "mixed",
                    secs_types::calibration::Mode::Linear => "linear",
                    secs_types::calibration::Mode::Ultra => "ultra",
                }
                .into(),
                ultra_ms: calibration.settings.ultra_ms,
                latency_ms: calibration.design().chain.latency_ms,
                rc_hi_hz: calibration.design().rc_hi,
                rc_bands: calibration.settings.rc_bands.clone(),
                mimo_max_supports: calibration.settings.mimo_max_supports.max(0) as usize,
            },
            mimo_paths: self.mimo.as_ref().map(|result| result.run.paths.clone()),
            mimo_compute_key: self
                .mimo
                .as_ref()
                .map(|result| result.compute_key.clone())
                .unwrap_or_default(),
        })
    }

    pub(crate) fn live_calibration(&self) -> Option<&CalibrationRun> {
        self.calibration.as_ref()
    }

    pub(crate) fn subs(&self) -> Option<&CalibrationRun> {
        self.subs.as_ref()
    }

    pub(crate) fn cached(&self) -> Option<&CachedResults> {
        self.cached.as_ref()
    }

    pub(crate) fn replace_cache(&mut self, cached: CachedResults) {
        self.cached = Some(cached);
    }

    pub(crate) fn all_notes_mut(&mut self) -> impl Iterator<Item = &mut Vec<Notice>> {
        [
            self.calibration.as_mut().map(|run| &mut run.summary.notes),
            self.subs.as_mut().map(|run| &mut run.summary.notes),
            self.mimo.as_mut().map(|result| &mut result.summary.notes),
        ]
        .into_iter()
        .flatten()
    }

    pub(crate) fn owner_notes_mut(&mut self, owner: CacheOwner) -> Option<&mut Vec<Notice>> {
        match owner {
            CacheOwner::Calibration => self.calibration.as_mut().map(|run| &mut run.summary.notes),
            CacheOwner::Subs => self.subs.as_mut().map(|run| &mut run.summary.notes),
            CacheOwner::Mimo => self.mimo.as_mut().map(|result| &mut result.summary.notes),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CacheOwner {
    Calibration,
    Subs,
    Mimo,
}

pub(crate) fn calibration_stale(run: &CalibrationRun, project: &Project) -> bool {
    calibration_stale_reason(run, project).is_some()
}

fn calibration_stale_reason(run: &CalibrationRun, project: &Project) -> Option<&'static str> {
    stale_reason(
        &run.signature,
        &run.settings.engine_key(),
        &run.target_key,
        Some(&run.config_key),
        project,
    )
}

pub(crate) fn cached_stale_reason(
    cached: &CachedResults,
    project: &Project,
) -> Option<&'static str> {
    stale_reason(
        &cached.measurement_signature,
        &cached.engine_key,
        &cached.target_key,
        cached.config_key.as_deref(),
        project,
    )
}

fn stale_reason(
    signature: &str,
    engine_key: &str,
    target_key: &str,
    config_key_at_run: Option<&str>,
    project: &Project,
) -> Option<&'static str> {
    if signature != signature_key(project) {
        Some("measurements")
    } else if config_key_at_run.is_some_and(|key| key != config_key(project)) {
        Some("config")
    } else if engine_key != calibration_from_project(project).engine_key() {
        Some("settings")
    } else if target_key != target_key_string(project) {
        Some("target")
    } else {
        None
    }
}

fn cached_notes(notes: &[Notice]) -> Vec<Notice> {
    notes
        .iter()
        .filter(|notice| notice.code != "cache_save_failed")
        .cloned()
        .collect()
}

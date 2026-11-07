use std::sync::Arc;

use secs_engine::Design;
use secs_project::{
    Project, calibration_from_project, measured_positions, measurement_signature, target_key,
};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrationSummary, MimoExportDecision, MimoSummary, Notice, ResultsSummary,
};

use crate::cache::CachedResults;
use crate::calibration::xo_rec_key;
use crate::compute::ComputeIdentity;
use crate::measure::json::JsonExt;

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectKeys {
    pub config: String,
    pub signature: String,
    pub target: String,
}

impl ProjectKeys {
    pub(crate) fn of(project: &Project) -> Self {
        let identities = match &project.measurements {
            secs_project::PyJson::Array(entries) => entries
                .iter()
                .map(|entry| match entry {
                    secs_project::PyJson::Object(fields) => fields
                        .iter()
                        .find(|(key, _)| key == "uuid")
                        .map(|(_, value)| value),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        };
        let base = format!("{:?}:{identities:?}", measurement_signature(project));
        let has_ir = |position: i64, role: &str| {
            match &project.measurements {
                secs_project::PyJson::Array(entries) => entries.as_slice(),
                _ => &[],
            }
            .iter()
            .any(|entry| {
                let matches_position = match entry.get("pos") {
                    Some(secs_project::PyJson::Int(value) | secs_project::PyJson::Str(value)) => {
                        value.parse() == Ok(position)
                    }
                    Some(secs_project::PyJson::Float(value)) => {
                        value.is_finite() && value.trunc() == position as f64
                    }
                    Some(secs_project::PyJson::Bool(value)) => i64::from(*value) == position,
                    _ => false,
                };
                matches_position
                    && entry
                        .get("ir")
                        .and_then(secs_project::PyJson::object)
                        .and_then(|values| values.iter().find(|(name, _)| name == role))
                        .is_some_and(|(_, value)| {
                            matches!(value, secs_project::PyJson::Str(path) if !path.is_empty())
                        })
            })
        };
        let calibration = project
            .measurement_calibration
            .iter()
            .filter_map(|((position, role), fingerprint)| {
                has_ir(*position, role).then_some((*position, role.as_str(), *fingerprint))
            })
            .collect::<Vec<_>>();
        let signature = if calibration.is_empty() {
            base
        } else {
            format!("{base}:cal{calibration:?}")
        };
        Self {
            config: format!("{:?}:{:?}", project.config, project.xo),
            signature,
            target: format!("{:?}", target_key(project)),
        }
    }

    pub(crate) fn run(&self, settings: &CalibrationSettings) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.signature,
            self.target,
            settings.engine_key(),
            self.config
        )
    }
}

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
            positions: design.positions().count(),
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
    pub(crate) fn inputs_unchanged_since(
        &self,
        keys: &ProjectKeys,
        settings: &CalibrationSettings,
    ) -> bool {
        let expected = keys.run(settings);
        self.calibration
            .as_ref()
            .is_some_and(|run| run.run_key == expected)
            || (self.calibration.is_none()
                && self
                    .cached
                    .as_ref()
                    .and_then(|cache| cache.run_key.as_ref())
                    == Some(&expected))
    }

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
        let settings = calibration_from_project(project);
        let xo_rec_stale = !settings.xo_rec.is_empty()
            && !settings.xo_rec_key.is_empty()
            && settings.xo_rec_key != xo_rec_key(project, &settings);
        ResultsSummary {
            calibration,
            mimo,
            mimo_export: verdict.decision,
            xo_rec_stale,
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
                notices: cached_notices(&calibration.summary.notices),
                ..calibration.summary.clone()
            },
            calibration_chain: calibration.design().signal_chain().clone(),
            mimo: self.mimo.as_ref().map(|result| MimoSummary {
                notices: cached_notices(&result.summary.notices),
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
                latency_ms: calibration.design().latency_ms(),
                rc_hi_hz: calibration.design().rc_hi(),
                rc_bands: calibration.settings.rc_bands.clone(),
                lf_align: calibration.settings.lf_align,
                mode_decay: calibration.settings.mode_decay,
                hf_phase: calibration.settings.hf_phase,
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
            self.calibration
                .as_mut()
                .map(|run| &mut run.summary.notices),
            self.subs.as_mut().map(|run| &mut run.summary.notices),
            self.mimo.as_mut().map(|result| &mut result.summary.notices),
        ]
        .into_iter()
        .flatten()
    }

    pub(crate) fn owner_notes_mut(&mut self, owner: CacheOwner) -> Option<&mut Vec<Notice>> {
        match owner {
            CacheOwner::Calibration => self
                .calibration
                .as_mut()
                .map(|run| &mut run.summary.notices),
            CacheOwner::Subs => self.subs.as_mut().map(|run| &mut run.summary.notices),
            CacheOwner::Mimo => self.mimo.as_mut().map(|result| &mut result.summary.notices),
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
    let keys = ProjectKeys::of(project);
    if signature != keys.signature {
        Some("measurements")
    } else if config_key_at_run.is_none_or(|key| key != keys.config) {
        Some("config")
    } else if engine_key != calibration_from_project(project).engine_key() {
        Some("settings")
    } else if target_key != keys.target {
        Some("target")
    } else {
        None
    }
}

fn cached_notices(notices: &[Notice]) -> Vec<Notice> {
    notices
        .iter()
        .filter(|notice| notice.code != "cache_save_failed")
        .cloned()
        .collect()
}

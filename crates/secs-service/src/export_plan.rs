use std::path::PathBuf;
use std::sync::Arc;

use secs_types::calibration::{CalibrationSettings, SampleRatePolicy};
use secs_types::chain::{MimoPaths, SignalChain};
use secs_types::compute::ComputeReport;
use secs_types::ipc::{CamillaDevices, ExportKind, Notice};

use crate::export_run::ExportDescription;
use crate::results::ProjectKeys;
use crate::session::{ProjectSnapshot, ProjectTicket};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportPurpose {
    Export,
    Apply,
}

impl ExportPurpose {
    pub(crate) fn job_kind(self) -> secs_types::ipc::JobKind {
        match self {
            Self::Export => secs_types::ipc::JobKind::Export,
            Self::Apply => secs_types::ipc::JobKind::Apply,
        }
    }
}

#[derive(Debug)]
pub(crate) enum DesignSource {
    Live {
        design: Arc<secs_engine::Design>,
        settings: Arc<CalibrationSettings>,
    },
    Cached {
        chain: Arc<SignalChain>,
    },
    Replay {
        snapshot: Arc<ProjectSnapshot>,
        chain: Arc<SignalChain>,
        sample_rate_policy: SampleRatePolicy,
    },
    Measurements {
        snapshot: Arc<ProjectSnapshot>,
    },
}

impl DesignSource {
    pub(crate) fn chain(&self) -> Option<&SignalChain> {
        match self {
            Self::Live { design, .. } => Some(design.signal_chain()),
            Self::Cached { chain } | Self::Replay { chain, .. } => Some(chain),
            Self::Measurements { .. } => None,
        }
    }
}

type AppliedMimo = Vec<(String, Vec<secs_engine::C>)>;

#[derive(Debug)]
pub(crate) struct MimoSelection {
    pub paths: Arc<MimoPaths>,
    pub applied: Option<Arc<AppliedMimo>>,
    pub compute: ComputeReport,
}

#[derive(Debug)]
pub(crate) struct Destination {
    pub staging: PathBuf,
    pub final_folder: PathBuf,
    pub index_root: Option<PathBuf>,
    pub explicit_unnamed: bool,
}

#[derive(Debug)]
pub(crate) struct ManifestMeta {
    pub project_name: Option<String>,
    pub app_version: String,
    pub created_utc: Option<String>,
    pub display_design_name: String,
    pub output_rate_auto: bool,
    pub design_id: Option<String>,
}

#[derive(Debug)]
pub(crate) struct LastExportRecord {
    pub mode: String,
    pub includes_mimo: bool,
    pub design_name: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ExportPlan {
    pub kind: ExportKind,
    pub purpose: ExportPurpose,
    pub source: DesignSource,
    pub mimo: Option<MimoSelection>,
    pub description: ExportDescription,
    pub destination: Destination,
    pub manifest: ManifestMeta,
    pub record: Option<LastExportRecord>,
    pub output_rate: u32,
    pub output_rate_notes: Vec<Notice>,
    pub extra_rates: Vec<u32>,
    pub camilla_devices: Option<CamillaDevices>,
    pub mdat_scope: secs_export::MdatScope,
    pub originals: Option<ProjectSnapshot>,
    pub source_sidecars: Option<ProjectSnapshot>,
    pub project_keys: ProjectKeys,
    pub ticket: ProjectTicket,
    pub stale: bool,
    pub spl_offset: Option<f64>,
    pub now_ms: i64,
}

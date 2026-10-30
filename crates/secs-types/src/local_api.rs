//! Shared wire contract for the opt-in local HTTP and WebSocket API.
//!
//! The server and clients use these paths and payloads. The security and
//! lifecycle decisions are recorded in ADR 0016.

use serde::{Deserialize, Serialize};

use crate::ipc::{ExportKind, JobEvent, JobSnapshot, ProjectInfo, ResultsSummary};

/// Local API protocol version.
pub const API_VERSION: u32 = 1;
/// Default loopback TCP port.
pub const DEFAULT_PORT: u16 = 4712;
/// Token file name inside the application configuration directory.
pub const TOKEN_FILE: &str = "local-api-token";
/// Request dispatch endpoint.
pub const CALL_PATH: &str = "/v1/call";
/// Aggregate status endpoint.
pub const STATUS_PATH: &str = "/v1/status";
/// WebSocket event endpoint.
pub const EVENTS_PATH: &str = "/v1/events";

/// Aggregate state returned by [`STATUS_PATH`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// [`API_VERSION`].
    pub api: u32,
    /// Product name.
    pub app_name: String,
    /// Version of the running application.
    pub app_version: String,
    /// Version of the linked `secs-service` crate.
    pub library_version: String,
    /// Open project, when present.
    pub project: Option<ProjectInfo>,
    /// Current and retained job snapshots.
    pub jobs: Vec<JobSnapshot>,
    /// Latest calibration and MIMO summary, when a project has results.
    pub results: Option<ResultsSummary>,
    /// Most recent successful export in this service session.
    pub last_export: Option<LastExport>,
}

/// One successful export remembered by the service session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastExport {
    /// Export bundle kind.
    pub kind: ExportKind,
    /// Actual folder written by the export.
    pub folder: String,
    /// Whether the export included MIMO support paths.
    pub mimo: bool,
    /// Stable correction-mode name.
    pub mode: String,
    /// The design name a filter export carried (`ExportParams::design_name`),
    /// `None` for an unnamed or REW mdat export.
    #[serde(default)]
    pub design_name: Option<String>,
    /// The design identity in `secs-design.json`, for a filter export.
    #[serde(default)]
    pub design_id: Option<String>,
    /// Completion time in UTC, formatted as RFC 3339.
    pub time: String,
}

/// Event sent as one text frame on [`EVENTS_PATH`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerEvent {
    /// A service job event, regardless of which caller started the job.
    Job { event: JobEvent },
    /// A successful local-API request changed observable state.
    Changed { method: String },
}

/// Local-API policy for one IPC method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodAccess {
    /// The method opens a window or program and cannot be called remotely.
    pub ui_only: bool,
    /// A successful call changes observable state or starts work.
    pub mutates: bool,
}

const fn entry(method: &'static str, ui_only: bool, mutates: bool) -> (&'static str, MethodAccess) {
    (method, MethodAccess { ui_only, mutates })
}

const ACCESS: &[(&str, MethodAccess)] = &[
    entry("bootstrap", false, false),
    entry("project_new", false, true),
    entry("project_open", false, true),
    entry("project_save", false, true),
    entry("project_info", false, false),
    entry("sample_load", false, true),
    entry("calibrate", false, true),
    entry("recommend_xo", false, true),
    entry("mimo", false, true),
    entry("results", false, false),
    entry("export", false, true),
    entry("job_cancel", false, true),
    entry("jobs", false, false),
    entry("measure_methods", false, false),
    entry("measure_configure", false, true),
    entry("measure_config_get", false, false),
    entry("measurement_records", false, false),
    entry("mic_calibration_get", false, false),
    entry("mic_calibration_set", false, true),
    entry("mic_calibration_clear", false, true),
    entry("mic_calibration_attach", false, true),
    entry("levels_get", false, false),
    entry("levels_fill_defaults", false, true),
    entry("level_stop", false, true),
    entry("measure_position", false, true),
    entry("position_clear", false, true),
    entry("undo", false, true),
    entry("rew_launch", true, true),
    entry("mdat_scan", false, false),
    entry("mdat_import", false, true),
    entry("config_get", false, false),
    entry("config_apply_preset", false, true),
    entry("config_set_sub_count", false, true),
    entry("config_set_speaker", false, true),
    entry("config_add_speaker", false, true),
    entry("config_remove_speaker", false, true),
    entry("config_renumber", false, true),
    entry("config_validate", false, false),
    entry("patch_sheet_csv", false, true),
    entry("dsp_get", false, false),
    entry("dsp_set", false, true),
    entry("layout_get", false, false),
    entry("layout_set", false, true),
    entry("position_estimate", false, false),
    entry("target_get", false, false),
    entry("target_set_points", false, true),
    entry("target_apply_preset", false, true),
    entry("target_save", false, true),
    entry("target_load", false, true),
    entry("calibration_settings_get", false, false),
    entry("calibration_settings_set", false, true),
    entry("calibration_settings_edit", false, true),
    entry("sub_warnings", false, false),
    entry("rew_info", false, false),
    entry("rew_driver_set", false, true),
    entry("mode_spec", false, false),
    entry("app_settings_get", false, false),
    entry("app_settings_set", false, true),
    entry("apply_status", false, false),
    entry("apply_camilla", false, true),
    entry("apply_eqapo", false, true),
    entry("revert_eqapo", false, true),
    entry("apply_settings_set", false, true),
    entry("host_install", true, true),
    entry("host_uninstall", true, true),
    entry("host_stop_managing", false, true),
    // Installing a release replaces the app; only its own screen asks for it.
    entry("update_status", false, false),
    entry("update_check", true, false),
    entry("update_download", true, true),
    entry("update_restart", true, true),
    entry("update_notice_dismiss", true, true),
    entry("update_open_release", true, false),
    entry("compute_status", false, false),
    entry("open_gpu_help", true, false),
    entry("measurement_curves", false, false),
    entry("result_curves", false, false),
    entry("result_lines", false, false),
    entry("sub_take_values", false, true),
    entry("dialog_open_project", true, false),
    entry("dialog_save_project", true, false),
    entry("dialog_pick_mdat", true, false),
    entry("dialog_pick_folder", true, false),
    entry("dialog_save_file", true, false),
    entry("dialog_open_file", true, false),
    entry("open_folder", true, true),
    entry("level_monitor_start", false, true),
    entry("level_monitor_stop", false, true),
    entry("level_play", false, true),
    entry("status_watch_start", false, true),
    entry("status_watch_stop", false, true),
    entry("export_folder_get", false, false),
    entry("export_folder_set", false, true),
    entry("local_api_status", true, false),
    entry("local_api_token_regenerate", true, true),
];

/// Returns the local-API policy for a wire method name.
pub fn access(method: &str) -> Option<MethodAccess> {
    ACCESS
        .iter()
        .find_map(|(name, value)| (*name == method).then_some(*value))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::ipc::Request;

    #[test]
    fn access_table_covers_every_request_method_exactly() {
        let known = Request::METHODS.iter().copied().collect::<BTreeSet<_>>();
        let listed = ACCESS.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let unique = listed.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(listed.len(), unique.len(), "duplicate access-table method");
        assert_eq!(unique, known);
        for method in Request::METHODS {
            assert!(access(method).is_some(), "missing {method}");
        }
    }
}

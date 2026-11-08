//! Shared wire contract for the opt-in local HTTP and WebSocket API.
//!
//! The server and clients use these paths and payloads. The security and
//! lifecycle decisions are recorded in ADR 0016.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::ipc::{ExportKind, JobEvent, JobSnapshot, ProjectInfo, Request, ResultsSummary};

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
#[derive(ts_rs::TS)]
#[ts(export)]
pub enum ServerEvent {
    /// A service job event, regardless of which caller started the job.
    Job { event: JobEvent },
    /// A successful local-API request changed observable state.
    Changed { method: String },
    /// Events lost by a slow subscriber; the server closes with code 1013 next.
    Lagged {
        /// Number of events no longer available in the server queue.
        #[ts(type = "number")]
        missed: u64,
    },
}

/// Local-API policy for one IPC method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodAccess {
    /// The method opens a window or program and cannot be called remotely.
    pub ui_only: bool,
    /// A successful call changes observable state or starts work.
    pub mutates: bool,
}

static METHOD_ACCESS: OnceLock<HashMap<&'static str, MethodAccess>> = OnceLock::new();

/// Returns the local-API policy for a wire method name.
pub fn access(method: &str) -> Option<MethodAccess> {
    METHOD_ACCESS
        .get_or_init(|| {
            let examples = Request::examples();
            Request::METHODS
                .iter()
                .copied()
                .zip(examples.iter().map(Request::access))
                .collect()
        })
        .get(method)
        .copied()
}

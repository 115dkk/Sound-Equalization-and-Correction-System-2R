#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The application service of SECS 2R.
//!
//! Its interface is small on purpose: [`Service::call`] takes a request and
//! always returns an envelope, and long jobs (added from M4) stream their
//! progress through an event sink. Behind it sit the project session, the job
//! registry (one job per kind, real cancellation), the stage cache and the
//! staleness of results, and the assembly of exports. The Tauri app and the
//! CLI are thin adapters over this crate; tests drive it directly.

use std::sync::OnceLock;

use secs_types::ipc::{
    BootstrapInfo, Envelope, ErrorCode, IpcError, Platform, Request, Response, UpstreamBase,
};

/// `UPSTREAM.toml`, compiled in so a build always reports the release it was
/// ported from.
const UPSTREAM_TOML: &str = include_str!("../../../UPSTREAM.toml");

/// Product name shown to users.
pub const APP_NAME: &str = "SECS 2R";

/// The SECS 2R version (the workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Deserialize)]
struct UpstreamFile {
    upstream: UpstreamBase,
}

/// The upstream release this build follows, parsed once from `UPSTREAM.toml`.
pub fn upstream() -> &'static UpstreamBase {
    static UPSTREAM: OnceLock<UpstreamBase> = OnceLock::new();
    UPSTREAM.get_or_init(|| {
        let file: UpstreamFile =
            toml::from_str(UPSTREAM_TOML).expect("UPSTREAM.toml is checked by the policy gates");
        file.upstream
    })
}

/// The application service. One instance lives as long as the app.
#[derive(Debug, Default)]
pub struct Service {}

impl Service {
    pub fn new() -> Self {
        Self {}
    }

    /// Handles one request. Never panics across the seam and never returns
    /// anything but an envelope.
    pub fn call(&self, request: Request) -> Envelope {
        match request {
            Request::Bootstrap => Envelope::Ok(Response::Bootstrap(self.bootstrap())),
        }
    }

    /// Handles a request that has not been parsed yet (what the app receives).
    /// A malformed request becomes an `invalid_request` envelope.
    pub fn call_value(&self, request: serde_json::Value) -> Envelope {
        match serde_json::from_value::<Request>(request) {
            Ok(request) => self.call(request),
            Err(error) => {
                Envelope::Err(IpcError::new(ErrorCode::InvalidRequest, error.to_string()))
            }
        }
    }

    fn bootstrap(&self) -> BootstrapInfo {
        BootstrapInfo {
            app_name: APP_NAME.to_owned(),
            version: VERSION.to_owned(),
            prerelease: VERSION.starts_with("0."),
            upstream: upstream().clone(),
            platform: Platform {
                os: std::env::consts::OS.to_owned(),
                arch: std::env::consts::ARCH.to_owned(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_reports_version_and_upstream_base() {
        let service = Service::new();
        let Envelope::Ok(Response::Bootstrap(info)) = service.call(Request::Bootstrap) else {
            panic!("bootstrap must succeed");
        };
        assert_eq!(info.app_name, "SECS 2R");
        assert_eq!(info.version, VERSION);
        assert_eq!(info.prerelease, VERSION.starts_with("0."));
        assert_eq!(info.upstream.commit.len(), 40);
        assert_eq!(info.platform.os, std::env::consts::OS);
    }

    #[test]
    fn malformed_requests_become_invalid_request_envelopes() {
        let service = Service::new();
        for request in [
            serde_json::json!({"method": "no_such_method"}),
            serde_json::json!({"params": {}}),
            serde_json::json!("bootstrap"),
        ] {
            let Envelope::Err(error) = service.call_value(request.clone()) else {
                panic!("{request} must fail");
            };
            assert_eq!(error.code, ErrorCode::InvalidRequest);
            assert!(!error.detail.is_empty());
        }
        assert!(matches!(
            service.call_value(serde_json::json!({"method": "bootstrap"})),
            Envelope::Ok(Response::Bootstrap(_))
        ));
    }
}

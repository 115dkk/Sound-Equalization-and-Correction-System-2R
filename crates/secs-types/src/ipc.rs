//! IPC between the screen and the service.
//!
//! The screen sends a [`Request`] as `{"method": "...", "params": {...}}` and
//! always receives an envelope: `{"ok": true, "data": Response}` or
//! `{"ok": false, "error": IpcError}`. The service never throws across this
//! seam. Long jobs report progress through a separate event channel.
//!
//! Errors carry a code and parameters, not prose: the screen turns them into
//! a sentence in the current language and keeps `detail` (the raw text) for
//! the details view and the log.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A call from the screen.
///
/// The method names come from the variant names in one place: serde and
/// strum both rename them to snake_case, and `method_names_match_the_wire`
/// checks that the two agree.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    strum::VariantNames,
    strum::IntoStaticStr,
)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum Request {
    /// Version, the upstream release this build is ported from, and the
    /// platform. The first call the screen makes.
    Bootstrap,
}

impl Request {
    /// Every method name, as it appears on the wire, derived from the
    /// variants. `secs-policy` checks that each one is registered in
    /// `features.toml` as `ipc.<method>`.
    pub const METHODS: &'static [&'static str] = <Self as strum::VariantNames>::VARIANTS;

    /// The wire name of this request's method.
    pub fn method(&self) -> &'static str {
        self.into()
    }
}

/// The successful result of a [`Request`], tagged with the same method name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "data", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    Bootstrap(BootstrapInfo),
}

/// What the screen needs before it draws anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapInfo {
    /// Product name shown in the title bar and the about view.
    pub app_name: String,
    /// The SECS 2R version (the workspace version).
    pub version: String,
    /// True while the major version is 0: every 0.x build is a prerelease.
    pub prerelease: bool,
    /// The SECS 2 release this build follows.
    pub upstream: UpstreamBase,
    pub platform: Platform,
}

/// The upstream SECS 2 release a build is ported from (`UPSTREAM.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpstreamBase {
    pub name: String,
    pub version: String,
    pub commit: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Platform {
    /// `windows`, `macos` or `linux` (`std::env::consts::OS`).
    pub os: String,
    /// `x86_64` or `aarch64` (`std::env::consts::ARCH`).
    pub arch: String,
}

/// Why a call failed. The screen maps each code to a written message with the
/// cause and the next action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    /// The request did not parse or named an unknown method.
    InvalidRequest,
    /// A bug: the service failed in a way it does not expect.
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IpcError {
    pub code: ErrorCode,
    /// Values for the message template of `code` (file names, counts, ...).
    pub params: BTreeMap<String, String>,
    /// Whether trying the same call again can succeed.
    pub retryable: bool,
    /// The raw error text, for the details view and the log only.
    pub detail: String,
}

impl IpcError {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
            retryable: false,
            detail: detail.into(),
        }
    }
}

/// The reply to every call: `{"ok": true, "data": ...}` or
/// `{"ok": false, "error": ...}`.
///
/// The TypeScript side declares the same union in `src/ipc/client.ts`
/// (`Envelope`); `envelope_wire_shape` below pins the JSON both sides rely on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Envelope {
    Ok(Response),
    Err(IpcError),
}

impl Serialize for Envelope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Envelope", 2)?;
        match self {
            Self::Ok(data) => {
                state.serialize_field("ok", &true)?;
                state.serialize_field("data", data)?;
            }
            Self::Err(error) => {
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", error)?;
            }
        }
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value of every variant. A new variant fails to compile here until
    /// it gets an example, which `method_names_match_the_wire` then checks.
    fn examples() -> Vec<Request> {
        let all = vec![Request::Bootstrap];
        for request in &all {
            match request {
                Request::Bootstrap => {}
            }
        }
        all
    }

    #[test]
    fn method_names_match_the_wire() {
        let examples = examples();
        assert_eq!(examples.len(), Request::METHODS.len());
        for request in examples {
            let wire = serde_json::to_value(&request).expect("serialize");
            assert_eq!(wire["method"], request.method(), "{request:?}");
            assert!(Request::METHODS.contains(&request.method()));
        }
    }

    #[test]
    fn every_method_name_is_listed_and_parses() {
        for method in Request::METHODS {
            let request: Request = serde_json::from_value(serde_json::json!({ "method": method }))
                .unwrap_or_else(|error| panic!("{method}: {error}"));
            assert_eq!(request.method(), *method);
        }
        let mut sorted = Request::METHODS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            Request::METHODS.len(),
            "duplicate method name"
        );
    }

    #[test]
    fn envelope_wire_shape() {
        let ok = Envelope::Ok(Response::Bootstrap(BootstrapInfo {
            app_name: "SECS 2R".into(),
            version: "0.0.1".into(),
            prerelease: true,
            upstream: UpstreamBase {
                name: "Sound Equalization and Correction System 2".into(),
                version: "Beta 1.2.0".into(),
                commit: "60ac53ad3a3d9304da115e5dda3183a475d2d7dd".into(),
                date: "2026-10-12".into(),
            },
            platform: Platform {
                os: "linux".into(),
                arch: "x86_64".into(),
            },
        }));
        let value = serde_json::to_value(&ok).expect("serialize");
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["method"], "bootstrap");
        assert_eq!(value["data"]["data"]["upstream"]["version"], "Beta 1.2.0");

        let err = Envelope::Err(IpcError::new(ErrorCode::InvalidRequest, "unknown method"));
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(
            value,
            serde_json::json!({
                "ok": false,
                "error": {"code": "invalid_request", "params": {}, "retryable": false, "detail": "unknown method"}
            })
        );
    }
}

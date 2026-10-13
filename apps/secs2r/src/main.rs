#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
// Release builds are GUI-subsystem executables: no console window behind the
// app on Windows. release.yml checks the PE subsystem of the packed exe.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! SECS 2R desktop app.
//!
//! A thin shell: one command, `secs_call`, hands the screen's request to
//! [`secs_service::Service`] and returns its envelope. The command never
//! rejects; failures travel inside the envelope (docs/adr/0003).

use secs_service::Service;
use secs_types::ipc::{Envelope, ErrorCode, IpcError};
use tauri::State;

/// The one IPC command. `request` stays unparsed (and optional) JSON here so
/// a malformed or missing request still yields an envelope instead of a
/// rejected promise.
#[tauri::command]
fn secs_call(service: State<'_, Service>, request: Option<serde_json::Value>) -> Envelope {
    handle(&service, request)
}

fn handle(service: &Service, request: Option<serde_json::Value>) -> Envelope {
    match request {
        Some(request) => service.call_value(request),
        None => Envelope::Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "secs_call needs a `request` argument",
        )),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(Service::new())
        .invoke_handler(tauri::generate_handler![secs_call])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            eprintln!("SECS 2R could not start: {error}");
            std::process::exit(1);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_request_argument_is_an_envelope() {
        let service = Service::new();
        let Envelope::Err(error) = handle(&service, None) else {
            panic!("a missing request must be an error envelope");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(matches!(
            handle(&service, Some(serde_json::json!({"method": "bootstrap"}))),
            Envelope::Ok(_)
        ));
    }
}

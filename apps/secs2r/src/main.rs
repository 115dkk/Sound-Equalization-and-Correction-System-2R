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

use std::fmt;
use std::sync::Arc;

use secs_service::{Deps, EventSink, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, IpcError, JobEvent};
use tauri::{State, Webview, ipc::JavaScriptChannelId};

#[derive(Clone)]
struct EventChannel(tauri::ipc::Channel<JobEvent>);

impl fmt::Debug for EventChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EventChannel").field(&self.0.id()).finish()
    }
}

impl EventSink for EventChannel {
    fn send(&self, event: JobEvent) {
        let _ = self.0.send(event);
    }
}

/// The one IPC command. `request` stays unparsed (and optional) JSON here so
/// a malformed or missing request still yields an envelope instead of a
/// rejected promise.
#[tauri::command]
fn secs_call(
    service: State<'_, Service>,
    webview: Webview,
    request: Option<serde_json::Value>,
    events: Option<JavaScriptChannelId>,
) -> Envelope {
    let events: Arc<dyn EventSink> = events
        .map(|id| Arc::new(EventChannel(id.channel_on(webview))) as Arc<dyn EventSink>)
        .unwrap_or_else(|| Arc::new(NoEvents));
    handle(&service, request, events)
}

fn handle(
    service: &Service,
    request: Option<serde_json::Value>,
    events: Arc<dyn EventSink>,
) -> Envelope {
    match request {
        Some(request) => service.call_value(request, events),
        None => Envelope::Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "secs_call needs a `request` argument",
        )),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(Service::new(Deps::default()))
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
        let service = Service::new(Deps::default());
        let Envelope::Err(error) = handle(&service, None, Arc::new(NoEvents)) else {
            panic!("a missing request must be an error envelope");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(matches!(
            handle(
                &service,
                Some(serde_json::json!({"method": "bootstrap"})),
                Arc::new(NoEvents),
            ),
            Envelope::Ok(_)
        ));
    }
}

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
use std::time::{Duration, SystemTime};

mod host;
mod icon;

use secs_service::{Deps, EventSink, LocalTime, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, IpcError, JobEvent, Request, Response};
use tauri::{AppHandle, Manager, Webview, ipc::JavaScriptChannelId};

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
///
/// A command without `async` runs on the main thread, where a blocking file
/// dialog (the host methods) would freeze the event loop; the call runs on
/// the blocking pool instead.
#[tauri::command]
async fn secs_call(
    app: AppHandle,
    webview: Webview,
    request: Option<serde_json::Value>,
    events: Option<JavaScriptChannelId>,
) -> Envelope {
    let events: Arc<dyn EventSink> = events
        .map(|id| Arc::new(EventChannel(id.channel_on(webview))) as Arc<dyn EventSink>)
        .unwrap_or_else(|| Arc::new(NoEvents));
    tauri::async_runtime::spawn_blocking(move || {
        let envelope = handle(&app.state::<Service>(), request, events);
        if let Envelope::Ok(response) = &envelope
            && let Response::AppSettingsSet(settings) = response.as_ref()
        {
            icon::show(&app, settings.icon);
        }
        envelope
    })
    .await
    .unwrap_or_else(|error| Envelope::Err(IpcError::new(ErrorCode::Internal, error.to_string())))
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            app.manage(Service::new(Deps {
                acceleration: Arc::new(secs_gpu::select),
                audio_backend: Arc::new(secs_sys::audio::default_backend),
                rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
                process: Arc::new(secs_sys::process::OsProcessProbe),
                memory: Arc::new(secs_sys::memory::OsMemoryProbe),
                host: Arc::new(host::TauriHost::new(handle)),
                clock: Arc::new(|| {
                    let now = SystemTime::now();
                    LocalTime::from_system(now, secs_sys::clock::utc_offset_seconds(now))
                }),
                app_settings_path: Some(config_dir.join("settings.json")),
                ..Deps::default()
            }));
            app.manage(icon::Shown::default());
            let service = app.state::<Service>();
            if let Envelope::Ok(response) =
                service.call(Request::AppSettingsGet, Arc::new(NoEvents))
                && let Response::AppSettingsGet(settings) = *response
            {
                icon::show(app.handle(), settings.icon);
            }
            Ok(())
        })
        // The screen may answer a close request with "save first?" and keep
        // the window, so jobs stop only once the window is really gone (as
        // upstream stops monitors, measurement and computation on close,
        // waiting up to 3 s for the device).
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = window.state::<Service>().shutdown(Duration::from_secs(3));
            }
        })
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

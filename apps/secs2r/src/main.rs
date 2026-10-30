#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
// Release builds are GUI-subsystem executables: no console window behind the
// app on Windows. release.yml checks the PE subsystem of the packed exe.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! SECS 2R desktop app.
//!
//! A thin shell: one command, `secs_call`, hands the screen's request to
//! [`secs_service::Service`] and returns its envelope. The command never
//! rejects; failures travel inside the envelope (docs/adr/0003). The only
//! other command reports the bundle self-test (`self_test`) and does nothing
//! unless `SECS2R_SELF_TEST` armed one.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

mod apply;
mod host;
mod icon;
mod local_api;
mod self_test;

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
        let service = app.state::<Arc<Service>>();
        let changes_settings = request
            .as_ref()
            .and_then(|value| value.get("method"))
            .and_then(serde_json::Value::as_str)
            == Some("app_settings_set");
        let old_local_api = changes_settings.then(|| {
            match service.call(Request::AppSettingsGet, Arc::new(NoEvents)) {
                Envelope::Ok(response) => match *response {
                    Response::AppSettingsGet(settings) => settings.local_api,
                    _ => unreachable!("app_settings_get response matches its request"),
                },
                Envelope::Err(_) => unreachable!("app_settings_get cannot fail"),
            }
        });
        let envelope = handle(service.as_ref(), request, events);
        if let Envelope::Ok(response) = &envelope
            && let Response::AppSettingsSet(settings) = response.as_ref()
        {
            icon::show(&app, settings.icon);
            app.state::<local_api::Manager>().apply_settings(
                old_local_api.expect("settings request captured its old value"),
                settings.local_api,
            );
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
            let hub = secs_local_api::EventHub::default();
            let service = Arc::new(Service::new(Deps {
                app_version: env!("CARGO_PKG_VERSION").into(),
                acceleration: Arc::new(secs_gpu::select),
                audio_backend: Arc::new(secs_sys::audio::default_backend),
                rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
                process: Arc::new(secs_sys::process::OsProcessProbe),
                memory: Arc::new(secs_sys::memory::OsMemoryProbe),
                host: Arc::new(host::TauriHost::new(handle.clone())),
                observer: Arc::new(hub.clone()),
                clock: Arc::new(|| {
                    let now = SystemTime::now();
                    LocalTime::from_system(now, secs_sys::clock::utc_offset_seconds(now))
                }),
                clock_offset_seconds: Arc::new(|| {
                    secs_sys::clock::utc_offset_seconds(SystemTime::now())
                }),
                app_settings_path: Some(config_dir.join("settings.json")),
                app_data_path: config_dir.clone(),
                eqapo_install: Arc::new(secs_sys::eqapo_install),
                apply: Arc::new(apply::NativeApply),
                managed_host: Arc::new(apply::NativeManagedHost),
                ..Deps::default()
            }));
            let local_api = local_api::Manager::new(config_dir, service.clone(), hub, handle);
            app.manage(service.clone());
            app.manage(local_api);
            app.manage(icon::Shown::default());
            if let Envelope::Ok(response) =
                service.call(Request::AppSettingsGet, Arc::new(NoEvents))
                && let Response::AppSettingsGet(settings) = *response
            {
                icon::show(app.handle(), settings.icon);
                app.state::<local_api::Manager>()
                    .startup(settings.local_api);
            }
            self_test::arm(app);
            Ok(())
        })
        .on_page_load(self_test::on_page_load)
        // The screen may answer a close request with "save first?" and keep
        // the window, so jobs stop only once the window is really gone (as
        // upstream stops monitors, measurement and computation on close,
        // waiting up to 3 s for the device).
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = window
                    .state::<Arc<Service>>()
                    .shutdown(Duration::from_secs(3));
            }
        })
        .invoke_handler(tauri::generate_handler![
            secs_call,
            self_test::secs_self_test_report
        ])
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

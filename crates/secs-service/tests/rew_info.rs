#![forbid(unsafe_code)]
use secs_rew::{RecordedExchange, RecordedTransport, RewError};
use secs_service::{Deps, NoEvents, Service};
use secs_types::ipc::{Envelope, Request, Response};
use serde_json::json;
use std::{sync::Arc, time::Duration};

#[test]
fn rew_info_uses_injected_transport_and_preserves_unknown_facts() {
    let paths = [
        ("/version", 2),
        ("/application/blocking", 5),
        ("/audio/input-cal", 3),
        ("/audio/driver", 5),
        ("/audio/java/inputs", 5),
        ("/audio/java/input-devices", 5),
        ("/audio/java/output-devices", 5),
        ("/audio/samplerate", 5),
        ("/measure/sweep/configuration", 5),
        ("/measure/level", 3),
    ];
    let transport =
        RecordedTransport::new(paths.into_iter().map(|(path, seconds)| RecordedExchange {
            request: secs_rew::Request::new("GET", path, None, Duration::from_secs(seconds)),
            response: if path == "/version" {
                Ok(secs_rew::Response::ok(
                    json!({"message":"5.40 Beta 124 API 0.9.4"}),
                ))
            } else if path == "/audio/driver" {
                Ok(secs_rew::Response::ok(json!({"driver":"Java"})))
            } else {
                Err(RewError::Http {
                    status: 404,
                    path: path.into(),
                    message: "not available".into(),
                })
            },
        }));
    let cloned = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(cloned.clone())),
        ..Deps::default()
    });
    let Envelope::Ok(response) = service.call(Request::RewInfo, Arc::new(NoEvents)) else {
        panic!()
    };
    let Response::RewInfo(info) = *response else {
        panic!()
    };
    assert!(info.running && info.reachable);
    assert_eq!(info.version.as_deref(), Some("5.40 Beta 124 API 0.9.4"));
    assert!(info.pro.is_none() && info.blocking.is_none() && info.input_calibration_file.is_none());
    assert!(
        info.input_devices.is_none()
            && info.output_devices.is_none()
            && info.input_channel_names.is_none()
    );
    assert!(info.sample_rate.is_none() && info.sweep.is_none() && info.level_dbfs.is_none());
    transport.assert_finished();
}

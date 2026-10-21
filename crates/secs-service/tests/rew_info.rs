#![forbid(unsafe_code)]
use secs_rew::{RecordedExchange, RecordedTransport, RewError};
use secs_service::{Deps, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, Request, Response, RewDriverSetParams};
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
        ("/audio/java/input-device", 5),
        ("/audio/java/output-device", 5),
        ("/audio/java/input", 5),
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
    assert_eq!(info.driver.as_deref(), Some("Java"));
    assert!(info.pro.is_none() && info.blocking.is_none() && info.input_calibration_file.is_none());
    assert!(info.input_device.is_none() && info.output_device.is_none() && info.input.is_none());
    assert!(
        info.input_devices.is_none()
            && info.output_devices.is_none()
            && info.input_channel_names.is_none()
    );
    assert!(info.sample_rate.is_none() && info.sweep.is_none() && info.level_dbfs.is_none());
    transport.assert_finished();
}

fn post_driver(status: u16) -> RecordedExchange {
    RecordedExchange {
        request: secs_rew::Request::new(
            "POST",
            "/audio/driver",
            Some(json!({"driver":"ASIO"})),
            Duration::from_secs(30),
        ),
        response: Ok(secs_rew::Response {
            status,
            body: json!({"message":"result"}),
        }),
    }
}

#[test]
fn rew_driver_set_validates_posts_and_refreshes_info() {
    let service = Service::new(Deps::default());
    let Envelope::Err(error) = service.call(
        Request::RewDriverSet(RewDriverSetParams { driver: " ".into() }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(error.code, ErrorCode::MeasurementInput);
    assert_eq!(error.params["field"], "driver");

    let script = [
        post_driver(200),
        RecordedExchange::new(
            secs_rew::Request::new("GET", "/version", None, Duration::from_secs(2)),
            json!({"message":"5.40 Beta 124 API 0.9.4"}),
        ),
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/application/blocking",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/application/blocking".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/audio/input-cal",
                None,
                Duration::from_secs(3),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/audio/input-cal".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange::new(
            secs_rew::Request::new("GET", "/audio/driver", None, Duration::from_secs(5)),
            json!({"driver":"ASIO"}),
        ),
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/audio/asio/inputs",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/audio/asio/inputs".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/audio/asio/devices",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/audio/asio/devices".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/audio/asio/devices",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/audio/asio/devices".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/audio/samplerate",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/audio/samplerate".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new(
                "GET",
                "/measure/sweep/configuration",
                None,
                Duration::from_secs(5),
            ),
            response: Err(RewError::Http {
                status: 404,
                path: "/measure/sweep/configuration".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange {
            request: secs_rew::Request::new("GET", "/measure/level", None, Duration::from_secs(3)),
            response: Err(RewError::Http {
                status: 404,
                path: "/measure/level".into(),
                message: "not available".into(),
            }),
        },
        RecordedExchange::new(
            secs_rew::Request::new("GET", "/audio/asio/device", None, Duration::from_secs(5)),
            json!("MOTU Gen 5"),
        ),
        RecordedExchange::new(
            secs_rew::Request::new("GET", "/audio/asio/input", None, Duration::from_secs(5)),
            json!("1: Mic"),
        ),
    ];
    let transport = RecordedTransport::new(script);
    let cloned = transport.clone();
    let service = Service::new(Deps {
        rew_transport: Arc::new(move || Box::new(cloned.clone())),
        ..Deps::default()
    });
    let Envelope::Ok(response) = service.call(
        Request::RewDriverSet(RewDriverSetParams {
            driver: "ASIO".into(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    let Response::RewDriverSet(info) = *response else {
        panic!()
    };
    assert_eq!(info.driver.as_deref(), Some("ASIO"));
    transport.assert_finished();
}

#[test]
fn rew_driver_set_maps_transport_license_and_http_errors() {
    for (exchange, code) in [
        (
            RecordedExchange {
                request: post_driver(200).request,
                response: Err(RewError::Connection {
                    path: "/audio/driver".into(),
                    message: "offline".into(),
                }),
            },
            ErrorCode::RewUnreachable,
        ),
        (post_driver(401), ErrorCode::RewProRequired),
        (post_driver(409), ErrorCode::RewFailed),
    ] {
        let transport = RecordedTransport::new([exchange]);
        let cloned = transport.clone();
        let service = Service::new(Deps {
            rew_transport: Arc::new(move || Box::new(cloned.clone())),
            ..Deps::default()
        });
        let Envelope::Err(error) = service.call(
            Request::RewDriverSet(RewDriverSetParams {
                driver: "ASIO".into(),
            }),
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        assert_eq!(error.code, code);
        transport.assert_finished();
    }
}

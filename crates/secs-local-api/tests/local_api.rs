#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use secs_local_api::{EventHub, LocalApiError, ServerConfig, ServerHandle};
use secs_service::{Deps, Host, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, JobEvent, Request};
use secs_types::local_api::{ServerEvent, Status};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
static NEXT: AtomicU64 = AtomicU64::new(0);

fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "secs-local-api-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn service_with(host: Arc<dyn Host>) -> (Arc<Service>, EventHub) {
    let hub = EventHub::default();
    let service = Arc::new(Service::new(Deps {
        host,
        observer: Arc::new(hub.clone()),
        ..Deps::default()
    }));
    (service, hub)
}

fn running(service: Arc<Service>, hub: EventHub) -> ServerHandle {
    secs_local_api::start(ServerConfig {
        port: 0,
        token: TOKEN.into(),
        service,
        hub,
        on_change: Arc::new(|_| {}),
    })
    .unwrap()
}

fn exchange(port: u16, request: Vec<u8>) -> (u16, Vec<u8>) {
    let mut stream = (0..100)
        .find_map(|_| match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => Some(stream),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(5));
                None
            }
        })
        .expect("server accepts connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(&request).unwrap();
    let mut response = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(size) => response.extend_from_slice(&chunk[..size]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                break;
            }
            Err(error) => panic!("read response: {error}"),
        }
    }
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response header");
    let header = std::str::from_utf8(&response[..split]).unwrap();
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, response[split + 4..].to_vec())
}

fn request(
    _port: u16,
    method: &str,
    path: &str,
    host: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(origin) = origin {
        head.push_str(&format!("Origin: {origin}\r\n"));
    }
    head.push_str("\r\n");
    let mut request = head.into_bytes();
    request.extend_from_slice(body);
    request
}

fn call(port: u16, body: serde_json::Value) -> (u16, Vec<u8>) {
    let bytes = serde_json::to_vec(&body).unwrap();
    exchange(
        port,
        request(
            port,
            "POST",
            "/v1/call",
            &format!("127.0.0.1:{port}"),
            Some(TOKEN),
            None,
            &bytes,
        ),
    )
}

#[derive(Debug, Default)]
struct CountingHost(AtomicUsize);

impl Host for CountingHost {
    fn dialog_open_project(&self) -> Result<Option<String>, secs_types::ipc::IpcError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Some("must-not-open.secs".into()))
    }
}

#[test]
fn authentication_host_origin_and_body_limit_are_enforced() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let valid_host = format!("127.0.0.1:{port}");

    for token in [None, Some("wrong")] {
        let (status, body) = exchange(
            port,
            request(port, "GET", "/v1/status", &valid_host, token, None, &[]),
        );
        assert_eq!(status, 401);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["ok"],
            false
        );
    }
    let (status, _) = exchange(
        port,
        request(
            port,
            "GET",
            "/v1/status",
            &format!("example.com:{port}"),
            Some(TOKEN),
            None,
            &[],
        ),
    );
    assert_eq!(status, 403);
    let (status, _) = exchange(
        port,
        request(
            port,
            "GET",
            "/v1/status",
            &valid_host,
            Some(TOKEN),
            Some("https://example.com"),
            &[],
        ),
    );
    assert_eq!(status, 403);

    let oversized = vec![b'x'; 2 * 1024 * 1024];
    let (status, body) = exchange(
        port,
        request(
            port,
            "POST",
            "/v1/call",
            &valid_host,
            Some(TOKEN),
            None,
            &oversized,
        ),
    );
    assert_eq!(status, 413);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["ok"],
        false
    );
}

#[test]
fn call_matches_service_and_ui_only_never_reaches_the_host() {
    let host = Arc::new(CountingHost::default());
    let (service, hub) = service_with(host.clone());
    let server = running(service.clone(), hub);
    let (status, body) = call(server.port(), serde_json::json!({"method": "bootstrap"}));
    assert_eq!(status, 200);
    let direct = service.call(Request::Bootstrap, Arc::new(NoEvents));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::to_value(direct).unwrap()
    );

    let (status, body) = call(
        server.port(),
        serde_json::json!({"method": "dialog_open_project"}),
    );
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "ui_only");
    assert_eq!(host.0.load(Ordering::SeqCst), 0);
}

#[test]
fn status_has_protocol_version_and_empty_session() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let (code, body) = exchange(
        port,
        request(
            port,
            "GET",
            "/v1/status",
            &format!("localhost:{port}"),
            Some(TOKEN),
            None,
            &[],
        ),
    );
    assert_eq!(code, 200);
    let status: Status = serde_json::from_slice(&body).unwrap();
    assert_eq!(status.api, 1);
    assert!(status.project.is_none());
    assert!(status.last_export.is_none());
}

#[test]
fn websocket_receives_changed_and_ordered_job_events() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        websocket_receives_changed_and_ordered_job_events_async().await;
    });
}

async fn websocket_receives_changed_and_ordered_job_events_async() {
    let root = root("events");
    std::fs::create_dir_all(&root).unwrap();
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let mut request = format!("ws://127.0.0.1:{port}/v1/events")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
    let (mut websocket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    let project = root.join("sample.secs");
    let (status, body) = call(
        port,
        serde_json::json!({
            "method": "project_new",
            "params": {
                "path": project.to_string_lossy(),
                "preset": "2.0",
                "sub_count": 0,
                "sample": true,
                "tutorial": false
            }
        }),
    );
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let changed = next_event(&mut websocket).await;
    assert_eq!(
        changed,
        ServerEvent::Changed {
            method: "project_new".into()
        }
    );

    let (status, body) = call(
        port,
        serde_json::json!({"method": "sample_load", "params": {}}),
    );
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let mut jobs = Vec::new();
    while !matches!(jobs.last(), Some(JobEvent::Finished { .. })) {
        if let ServerEvent::Job { event } = next_event(&mut websocket).await {
            jobs.push(event);
        }
    }
    assert!(matches!(jobs.first(), Some(JobEvent::Started { .. })));
    assert!(matches!(jobs.last(), Some(JobEvent::Finished { .. })));
    std::fs::remove_dir_all(root).unwrap();
}

async fn next_event(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> ServerEvent {
    let message = tokio::time::timeout(Duration::from_secs(10), websocket.next())
        .await
        .expect("event timeout")
        .expect("websocket remains open")
        .expect("valid websocket frame");
    let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
        panic!("expected text frame")
    };
    serde_json::from_str(&text).unwrap()
}

#[test]
fn stop_frees_the_port_and_busy_port_is_typed() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let first = running(service.clone(), hub.clone());
    let port = first.port();
    let busy = secs_local_api::start(ServerConfig {
        port,
        token: TOKEN.into(),
        service: service.clone(),
        hub: hub.clone(),
        on_change: Arc::new(|_| {}),
    })
    .unwrap_err();
    assert!(matches!(busy, LocalApiError::PortBusy { port: value } if value == port));
    first.stop();
    let second = secs_local_api::start(ServerConfig {
        port,
        token: TOKEN.into(),
        service,
        hub,
        on_change: Arc::new(|_| {}),
    })
    .unwrap();
    assert_eq!(second.port(), port);
    second.stop();
}

#[test]
fn ui_only_error_has_the_public_code() {
    assert_eq!(
        serde_json::to_value(Envelope::Err(secs_types::ipc::IpcError::new(
            ErrorCode::UiOnly,
            "ui only"
        )))
        .unwrap()["error"]["code"],
        "ui_only"
    );
}

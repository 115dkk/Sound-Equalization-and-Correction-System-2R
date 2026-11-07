#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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

fn read_status(stream: &mut TcpStream) -> u16 {
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response header");
    std::str::from_utf8(&response[..split])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn progressing_near_limit_upload_is_not_timed_out_td126() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let padding = "x".repeat(1024 * 1024 - 128);
    let body = format!(r#"{{"method":"bootstrap","padding":"{padding}"}}"#).into_bytes();
    assert!(body.len() < 1024 * 1024);
    assert!(body.len() > 1024 * 1024 - 256);

    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        client,
        "POST /v1/call HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    for (index, chunk) in body.chunks(body.len().div_ceil(5)).enumerate() {
        client.write_all(chunk).unwrap();
        client.flush().unwrap();
        if index < 4 {
            std::thread::sleep(Duration::from_millis(600));
        }
    }
    assert_eq!(read_status(&mut client), 200);
    server.stop();
}

#[test]
fn idle_request_body_returns_request_timeout_td126() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        client,
        "POST /v1/call HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
    )
    .unwrap();
    client.flush().unwrap();

    assert_eq!(read_status(&mut client), 408);
    server.stop();
}

#[test]
fn stop_finishes_while_authenticated_chunked_body_keeps_trickling_td126() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        client,
        "POST /v1/call HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\n{{\r\n"
    )
    .unwrap();
    client.flush().unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..5 {
            std::thread::sleep(Duration::from_secs(1));
            if client.write_all(b"1\r\n \r\n").is_err() || client.flush().is_err() {
                break;
            }
        }
    });
    std::thread::sleep(Duration::from_millis(100));

    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        server.stop();
        let _ = finished_tx.send(());
    });
    assert!(
        finished_rx.recv_timeout(Duration::from_secs(3)).is_ok(),
        "server stop waited for a trickling authenticated request body"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    writer.join().unwrap();
}

#[test]
fn stop_finishes_while_a_client_stalls_mid_body_td126() {
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let server = running(service, hub);
    let port = server.port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        client,
        "POST /v1/call HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{{"
    )
    .unwrap();
    client.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        server.stop();
        let _ = finished_tx.send(());
    });
    assert!(
        finished_rx.recv_timeout(Duration::from_secs(4)).is_ok(),
        "server stop waited for an incomplete request body"
    );
    assert!(started.elapsed() < Duration::from_secs(4));
    drop(client);
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

#[test]
fn slow_reader_receives_lagged_before_the_1013_close_td161() {
    use secs_service::EventSink;
    use secs_types::ipc::JobId;
    use tokio_tungstenite::tungstenite::Message;

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (service, hub) = service_with(Arc::new(secs_service::NullHost));
        let server = running(service, hub.clone());
        let mut request = format!("ws://127.0.0.1:{}/v1/events", server.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        // Large frames exhaust the socket buffer while the client does not read.
        // Use several ring capacities because the server can drain some concurrently.
        for _ in 0..1024 {
            hub.send(JobEvent::Progress {
                job: JobId(1),
                stage: "x".repeat(128 * 1024),
                done: 0,
                total: 1,
            });
        }
        let mut saw_lagged = false;
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match frame {
                Message::Text(text) => {
                    assert!(!saw_lagged, "lagged must be the final text frame");
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if value["kind"] == "lagged" {
                        assert!(value["missed"].as_u64().unwrap() >= 5);
                        saw_lagged = true;
                    }
                }
                Message::Close(frame) => {
                    assert!(saw_lagged, "missing lagged text before the close frame");
                    assert_eq!(u16::from(frame.unwrap().code), 1013);
                    break;
                }
                _ => {}
            }
        }
        server.stop();
    });
}

#[test]
fn stop_returns_within_the_shutdown_bound_while_a_request_runs_td214() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let service = Arc::new(Service::new(Deps {
        eqapo_install: Arc::new(move || {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(10));
            let _ = finished_tx.send(());
            None
        }),
        ..Deps::default()
    }));
    let server = running(service, EventHub::default());
    let port = server.port();
    let caller = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        stream
            .write_all(&request(
                port,
                "POST",
                "/v1/call",
                &format!("127.0.0.1:{port}"),
                Some(TOKEN),
                None,
                br#"{"method":"apply_status"}"#,
            ))
            .unwrap();
        let _ = stream.read_to_end(&mut Vec::new());
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    server.stop();
    let elapsed = started.elapsed();
    // Let the injected blocking operation finish before the test exits.
    finished_rx.recv_timeout(Duration::from_secs(12)).unwrap();
    caller.join().unwrap();
    assert!(
        elapsed < Duration::from_secs(3),
        "stop waited {elapsed:?} for a blocking request"
    );
}

fn managed_runtime(name: &str) -> (secs_local_api::Runtime, PathBuf) {
    let root = root(name);
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let runtime = secs_local_api::Runtime::new(
        root.clone(),
        service,
        hub,
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
    );
    (runtime, root)
}

fn local_settings(enabled: bool, port: u16) -> secs_types::ipc::LocalApiSettings {
    secs_types::ipc::LocalApiSettings { enabled, port }
}

#[test]
fn runtime_startup_enabled_serves_and_disabled_only_makes_a_token() {
    let (enabled, enabled_root) = managed_runtime("runtime-enabled");
    enabled.startup(local_settings(true, 0));
    let status = enabled.status().unwrap();
    assert!(status.running);
    assert_ne!(status.port, 0);
    assert_eq!(
        exchange(
            status.port,
            request(
                status.port,
                "GET",
                "/v1/status",
                &format!("127.0.0.1:{}", status.port),
                Some(&status.token),
                None,
                &[],
            ),
        )
        .0,
        200
    );
    enabled.stop();

    let (disabled, disabled_root) = managed_runtime("runtime-disabled");
    disabled.startup(local_settings(false, 0));
    let status = disabled.status().unwrap();
    assert!(!status.running);
    assert_eq!(status.token.len(), 64);
    assert!(std::path::Path::new(&status.token_path).is_file());
    let _ = std::fs::remove_dir_all(enabled_root);
    let _ = std::fs::remove_dir_all(disabled_root);
}

#[test]
fn runtime_apply_settings_moves_port_stops_and_keeps_an_unchanged_server() {
    let (runtime, root) = managed_runtime("runtime-apply");
    runtime.startup(local_settings(true, 0));
    let first = runtime.status().unwrap();
    runtime.apply_settings(local_settings(true, 0), local_settings(true, 0));
    let unchanged = runtime.status().unwrap();
    assert_eq!(unchanged.port, first.port);
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let moved_port = probe.local_addr().unwrap().port();
    drop(probe);
    runtime.apply_settings(local_settings(true, 0), local_settings(true, moved_port));
    let moved = runtime.status().unwrap();
    assert!(moved.running);
    assert_eq!(moved.port, moved_port);
    assert!(TcpStream::connect(("127.0.0.1", first.port)).is_err());
    runtime.apply_settings(
        local_settings(true, moved_port),
        local_settings(false, moved_port),
    );
    assert!(!runtime.status().unwrap().running);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runtime_regenerate_token_invalidates_the_old_token() {
    let (runtime, root) = managed_runtime("runtime-token");
    runtime.startup(local_settings(true, 0));
    let old = runtime.status().unwrap();
    let new = runtime.regenerate_token().unwrap();
    assert_ne!(new.token, old.token);
    assert_eq!(
        exchange(
            new.port,
            request(
                new.port,
                "GET",
                "/v1/status",
                &format!("127.0.0.1:{}", new.port),
                Some(&old.token),
                None,
                &[],
            ),
        )
        .0,
        401
    );
    assert_eq!(
        exchange(
            new.port,
            request(
                new.port,
                "GET",
                "/v1/status",
                &format!("127.0.0.1:{}", new.port),
                Some(&new.token),
                None,
                &[],
            ),
        )
        .0,
        200
    );
    runtime.stop();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runtime_stop_returns_within_three_seconds_while_a_request_runs() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let service = Arc::new(Service::new(Deps {
        eqapo_install: Arc::new(move || {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(10));
            None
        }),
        ..Deps::default()
    }));
    let root = root("runtime-stop");
    let runtime = Arc::new(secs_local_api::Runtime::new(
        root.clone(),
        service,
        EventHub::default(),
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
    ));
    runtime.startup(local_settings(true, 0));
    let status = runtime.status().unwrap();
    let caller = std::thread::spawn(move || {
        let body = br#"{"method":"apply_status"}"#;
        let mut stream = TcpStream::connect(("127.0.0.1", status.port)).unwrap();
        stream
            .write_all(&request(
                status.port,
                "POST",
                "/v1/call",
                &format!("127.0.0.1:{}", status.port),
                Some(&status.token),
                None,
                body,
            ))
            .unwrap();
        let _ = stream.read_to_end(&mut Vec::new());
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    runtime.stop();
    assert!(started.elapsed() < Duration::from_secs(3));
    caller.join().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn read_only_token_folder_is_a_token_error() {
    let root = root("runtime-token-error");
    std::fs::write(&root, b"not a directory").unwrap();
    let (service, hub) = service_with(Arc::new(secs_service::NullHost));
    let runtime = secs_local_api::Runtime::new(
        root.clone(),
        service,
        hub,
        Arc::new(|_| {}),
        Arc::new(|_, _| {}),
    );
    assert!(matches!(runtime.status(), Err(LocalApiError::Token { .. })));
    let _ = std::fs::remove_file(root);
}

#[test]
fn calibration_summary_notes_are_typed_on_the_wire() {
    let note = secs_types::notes::Note::DipLimit {
        role: "L".into(),
        dips: vec![secs_types::notes::Dip {
            hz: 80.0,
            depth_db: 6.1,
            fill_db: 3.0,
        }],
    };
    let value = serde_json::to_value(note).unwrap();
    assert_eq!(value["code"], "dip_limit");
    assert_eq!(value["role"], "L");
    assert_eq!(value["dips"][0]["hz"], 80.0);
}

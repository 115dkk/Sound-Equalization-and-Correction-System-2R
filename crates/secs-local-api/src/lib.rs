#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Opt-in loopback HTTP and WebSocket API for SECS 2R.
//!
//! The protocol, authentication rules, and app-owned lifecycle are fixed by
//! [ADR 0016](https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/blob/main/docs/adr/0016-public-rust-api-and-local-api.md).

pub mod token;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::body::{Body, Bytes, HttpBody};
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::http::header::{AUTHORIZATION, CONNECTION, HOST, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, Request as HttpRequest, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response as HttpResponse};
use axum::routing::{get, post};
use secs_service::{EventSink, NoEvents, Service};
use secs_types::ipc::{Envelope, ErrorCode, IpcError};
use secs_types::local_api::{self, ServerEvent};
use tokio::sync::{broadcast, oneshot};

const MAX_BODY: usize = 1024 * 1024;
const BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(2);
const BODY_SHUTDOWN_POLL: Duration = Duration::from_millis(250);
const OVERSIZE_CLOSE_GRACE: Duration = Duration::from_millis(50);
const EVENT_CAPACITY: usize = 256;

/// Broadcast event sink shared by the app service and the local API server.
#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<ServerEvent>,
}

impl std::fmt::Debug for EventHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHub")
            .field("receiver_count", &self.sender.receiver_count())
            .finish()
    }
}

impl Default for EventHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CAPACITY);
        Self { sender }
    }
}

impl EventHub {
    fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.sender.subscribe()
    }

    fn changed(&self, method: &str) {
        let _ = self.sender.send(ServerEvent::Changed {
            method: method.to_owned(),
        });
    }
}

impl EventSink for EventHub {
    fn send(&self, event: secs_types::ipc::JobEvent) {
        let _ = self.sender.send(ServerEvent::Job { event });
    }
}

/// Inputs required to start one server.
#[derive(Clone)]
pub struct ServerConfig {
    /// Loopback port. Zero asks the operating system to choose a free port.
    pub port: u16,
    /// 64-character bearer token.
    pub token: String,
    /// Shared application service.
    pub service: Arc<Service>,
    /// Event hub also installed as the service observer.
    pub hub: EventHub,
    /// Called after a successful mutating request.
    pub on_change: Arc<dyn Fn(&str) + Send + Sync>,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("service", &self.service)
            .field("hub", &self.hub)
            .field("on_change", &"callback")
            .finish()
    }
}

/// Failure to bind or own the server runtime.
#[derive(Debug, thiserror::Error)]
pub enum LocalApiError {
    /// The requested loopback port is already occupied.
    #[error("local API port {port} is busy")]
    PortBusy { port: u16 },
    /// A listener operation failed.
    #[error("local API listener on port {port}: {source}")]
    Listener {
        port: u16,
        #[source]
        source: std::io::Error,
    },
    /// The dedicated Tokio runtime could not be created.
    #[error("local API runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// The owning server thread could not be created.
    #[error("local API thread: {0}")]
    Thread(#[source] std::io::Error),
}

/// Running server. Dropping it performs a blocking graceful stop.
pub struct ServerHandle {
    port: u16,
    shutdown_requested: Arc<AtomicBool>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl ServerHandle {
    /// Actual bound port, including the port selected for a zero request.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stops accepting connections and waits for the listener thread to exit.
    pub fn stop(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
        if let Some(shutdown) = lock(&self.shutdown).take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct AppState {
    port: u16,
    token: Arc<str>,
    shutdown_requested: Arc<AtomicBool>,
    service: Arc<Service>,
    hub: EventHub,
    on_change: Arc<dyn Fn(&str) + Send + Sync>,
}

/// Starts the loopback server on its own two-worker Tokio runtime.
pub fn start(config: ServerConfig) -> Result<ServerHandle, LocalApiError> {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);
    let listener = TcpListener::bind(address).map_err(|source| {
        if source.kind() == std::io::ErrorKind::AddrInUse {
            LocalApiError::PortBusy { port: config.port }
        } else {
            LocalApiError::Listener {
                port: config.port,
                source,
            }
        }
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|source| LocalApiError::Listener {
            port: config.port,
            source,
        })?;
    let port = listener
        .local_addr()
        .map_err(|source| LocalApiError::Listener {
            port: config.port,
            source,
        })?
        .port();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_io()
        .enable_time()
        .build()
        .map_err(LocalApiError::Runtime)?;
    let listener = {
        let _entered = runtime.enter();
        tokio::net::TcpListener::from_std(listener).map_err(|source| LocalApiError::Listener {
            port: config.port,
            source,
        })?
    };
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let state = AppState {
        port,
        token: config.token.into(),
        shutdown_requested: shutdown_requested.clone(),
        service: config.service,
        hub: config.hub,
        on_change: config.on_change,
    };
    let auth_state = state.clone();
    let router = Router::new()
        .route(local_api::CALL_PATH, post(call))
        .route(local_api::STATUS_PATH, get(status))
        .route(local_api::EVENTS_PATH, get(events))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(auth_state, authenticate))
        .with_state(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("secs-local-api".into())
        .spawn(move || {
            runtime.block_on(async move {
                let result = axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await;
                if let Err(error) = result {
                    eprintln!("local API server stopped with an error: {error}");
                }
            });
        })
        .map_err(LocalApiError::Thread)?;
    Ok(ServerHandle {
        port,
        shutdown_requested,
        shutdown: Mutex::new(Some(shutdown_tx)),
        thread: Mutex::new(Some(thread)),
    })
}

async fn authenticate(
    State(state): State<AppState>,
    request: HttpRequest<Body>,
    next: Next,
) -> HttpResponse {
    let expected_ip = format!("127.0.0.1:{}", state.port);
    let expected_name = format!("localhost:{}", state.port);
    let valid_host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| {
            host.eq_ignore_ascii_case(&expected_ip) || host.eq_ignore_ascii_case(&expected_name)
        });
    if !valid_host {
        return envelope_error(
            StatusCode::FORBIDDEN,
            ErrorCode::InvalidRequest,
            "forbidden host",
        );
    }
    if request.headers().contains_key(ORIGIN) {
        return envelope_error(
            StatusCode::FORBIDDEN,
            ErrorCode::InvalidRequest,
            "origin requests are forbidden",
        );
    }
    let supplied = request
        .headers()
        .get(AUTHORIZATION)
        .map(HeaderValue::as_bytes)
        .unwrap_or_default();
    let expected = format!("Bearer {}", state.token);
    if !constant_time_eq(supplied, expected.as_bytes()) {
        let mut response = envelope_error(
            StatusCode::UNAUTHORIZED,
            ErrorCode::InvalidRequest,
            "bearer token is missing or invalid",
        );
        response
            .headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }
    let (parts, body) = request.into_parts();
    match read_body(body, &state.shutdown_requested).await {
        Ok(body) => {
            next.run(HttpRequest::from_parts(parts, Body::from(body)))
                .await
        }
        Err(ReadBodyError::TooLarge) => envelope_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::InvalidRequest,
            "request body exceeds 1 MiB",
        ),
        Err(ReadBodyError::IdleTimeout) => envelope_error(
            StatusCode::REQUEST_TIMEOUT,
            ErrorCode::InvalidRequest,
            "request body was idle for 2 seconds",
        ),
        Err(ReadBodyError::Shutdown) => close_response(StatusCode::NO_CONTENT.into_response()),
        Err(ReadBodyError::Read(error)) => {
            envelope_error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest, &error)
        }
    }
}

fn close_response(mut response: HttpResponse) -> HttpResponse {
    response
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

#[derive(Debug)]
enum ReadBodyError {
    TooLarge,
    IdleTimeout,
    Shutdown,
    Read(String),
}

async fn read_body(
    mut body: Body,
    shutdown_requested: &AtomicBool,
) -> Result<Vec<u8>, ReadBodyError> {
    let mut bytes = Vec::new();
    let mut last_progress = tokio::time::Instant::now();
    loop {
        if shutdown_requested.load(Ordering::Acquire) {
            return Err(ReadBodyError::Shutdown);
        }
        let idle_remaining = BODY_IDLE_TIMEOUT.saturating_sub(last_progress.elapsed());
        if idle_remaining.is_zero() {
            return Err(ReadBodyError::IdleTimeout);
        }
        let frame = match tokio::time::timeout(
            idle_remaining.min(BODY_SHUTDOWN_POLL),
            std::future::poll_fn(|context| std::pin::Pin::new(&mut body).poll_frame(context)),
        )
        .await
        {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        let Some(frame) = frame else {
            break;
        };
        let frame = frame.map_err(|error| ReadBodyError::Read(error.to_string()))?;
        if let Ok(data) = frame.into_data()
            && !data.is_empty()
        {
            if bytes.len().saturating_add(data.len()) > MAX_BODY {
                discard_ready_frames(&mut body).await;
                return Err(ReadBodyError::TooLarge);
            }
            bytes.extend_from_slice(&data);
            last_progress = tokio::time::Instant::now();
        }
    }
    Ok(bytes)
}

async fn discard_ready_frames(body: &mut Body) {
    // Hyper cannot send a clean HTTP/1 response on Windows if unread bytes that
    // already reached the socket are reset. Discard only frames available during
    // this short fixed grace; never wait for a slow oversized sender.
    let deadline = tokio::time::Instant::now() + OVERSIZE_CLOSE_GRACE;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(
            remaining,
            std::future::poll_fn(|context| std::pin::Pin::new(&mut *body).poll_frame(context)),
        )
        .await
        {
            Ok(Some(Ok(_))) => {}
            _ => break,
        }
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

async fn call(State(state): State<AppState>, body: Bytes) -> HttpResponse {
    let value = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(value) => value,
        Err(error) => {
            return Json(Envelope::Err(IpcError::new(
                ErrorCode::InvalidRequest,
                error.to_string(),
            )))
            .into_response();
        }
    };
    let method = value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let access = method.as_deref().and_then(secs_types::local_api::access);
    if access.is_some_and(|access| access.ui_only) {
        return Json(Envelope::Err(IpcError::new(
            ErrorCode::UiOnly,
            "the method is available only from the application screen",
        )))
        .into_response();
    }
    let service = state.service.clone();
    let envelope =
        match tokio::task::spawn_blocking(move || service.call_value(value, Arc::new(NoEvents)))
            .await
        {
            Ok(envelope) => envelope,
            Err(error) => Envelope::Err(IpcError::new(ErrorCode::Internal, error.to_string())),
        };
    if matches!(envelope, Envelope::Ok(_))
        && access.is_some_and(|access| access.mutates)
        && let Some(method) = method
    {
        (state.on_change)(&method);
        state.hub.changed(&method);
    }
    Json(envelope).into_response()
}

async fn status(State(state): State<AppState>) -> HttpResponse {
    let service = state.service.clone();
    match tokio::task::spawn_blocking(move || service.status()).await {
        Ok(status) => Json(status).into_response(),
        Err(error) => envelope_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            &error.to_string(),
        ),
    }
}

async fn events(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> HttpResponse {
    let receiver = state.hub.subscribe();
    upgrade
        .on_upgrade(move |socket| websocket(socket, receiver))
        .into_response()
}

async fn websocket(mut socket: WebSocket, mut receiver: broadcast::Receiver<ServerEvent>) {
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Ok(event) => {
                    let Ok(text) = serde_json::to_string(&event) else { continue };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    let _ = socket.send(Message::Close(Some(CloseFrame {
                        code: 1013,
                        reason: format!("missed {missed} events").into(),
                    }))).await;
                    return;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(Message::Ping(payload))) => {
                    if socket.send(Message::Pong(payload)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(_)) => {}
            }
        }
    }
}

async fn not_found() -> HttpResponse {
    envelope_error(
        StatusCode::NOT_FOUND,
        ErrorCode::InvalidRequest,
        "local API endpoint not found",
    )
}

fn envelope_error(status: StatusCode, code: ErrorCode, detail: &str) -> HttpResponse {
    (status, Json(Envelope::Err(IpcError::new(code, detail)))).into_response()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn token_comparison_checks_contents_and_length() {
        assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer abd", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer abc0", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer ab", b"Bearer abc"));
    }
}

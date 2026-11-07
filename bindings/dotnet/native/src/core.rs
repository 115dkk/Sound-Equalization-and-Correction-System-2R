use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use secs::types::ipc::{Envelope, JobId};
use secs::{Client, Engine, EngineBuilder, Error};
use serde_json::{Value, json};

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static ENGINES: OnceLock<Mutex<HashMap<u64, Arc<Engine>>>> = OnceLock::new();
static CLIENTS: OnceLock<Mutex<HashMap<u64, Arc<Client>>>> = OnceLock::new();
static SUBSCRIPTIONS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Subscription>>>>> = OnceLock::new();
static RESULTS: OnceLock<Mutex<HashMap<u64, Arc<Vec<u8>>>>> = OnceLock::new();

macro_rules! success {
    ($data:expr) => {
        store_value(json!({ "ok": true, "data": $data }))
    };
}

struct Subscription {
    inner: secs::Subscription,
    closed: bool,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.inner.closer.close();
    }
}

pub(crate) fn guard<T>(operation: impl FnOnce() -> T, recover: impl FnOnce(String) -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
        Ok(value) => value,
        Err(payload) => recover(panic_message(payload.as_ref())),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

pub(crate) fn panic_result(detail: String) -> u64 {
    binding_error("internal", "native binding panicked", detail)
}

pub(crate) fn invalid_argument(detail: impl Into<String>) -> u64 {
    let detail = detail.into();
    binding_error("invalid_argument", &detail, &detail)
}

pub(crate) fn validate_pointer<T>(pointer: *const T, length: usize) -> Result<(), String> {
    if length > isize::MAX as usize {
        return Err(format!(
            "buffer length {length} exceeds the maximum supported length"
        ));
    }
    if pointer.is_null() && length != 0 {
        return Err(format!(
            "null pointer cannot describe a buffer of length {length}"
        ));
    }
    Ok(())
}

pub(crate) fn engine_new(path: &[u8]) -> u64 {
    let path = match std::str::from_utf8(path) {
        Ok(path) => path,
        Err(error) => return invalid_argument(format!("app settings path is not UTF-8: {error}")),
    };
    let engine = if path.is_empty() {
        EngineBuilder::default().build()
    } else {
        EngineBuilder::default()
            .app_settings_path(PathBuf::from(path))
            .build()
    };
    let handle = insert(engines(), Arc::new(engine));
    success!(json!({ "handle": handle }))
}

pub(crate) fn engine_free(handle: u64) {
    remove(engines(), handle);
}

pub(crate) fn engine_call(handle: u64, request: &[u8]) -> u64 {
    let engine = match get(engines(), handle) {
        Some(engine) => engine,
        None => return invalid_handle("engine", handle),
    };
    let request = match serde_json::from_slice::<Value>(request) {
        Ok(request) => request,
        Err(error) => {
            return binding_error(
                "invalid_request",
                "request is not valid JSON",
                error.to_string(),
            );
        }
    };
    match engine.call_json(request) {
        Envelope::Ok(response) => success!(response),
        Envelope::Err(error) => api_error(Error::Api(error)),
    }
}

pub(crate) fn engine_wait_job(handle: u64, job_id: u64, timeout_ms: u64) -> u64 {
    let engine = match get(engines(), handle) {
        Some(engine) => engine,
        None => return invalid_handle("engine", handle),
    };
    let timeout = match timeout(timeout_ms) {
        Ok(timeout) => timeout,
        Err(detail) => return invalid_argument(detail),
    };
    match engine.wait_job(JobId(job_id), timeout) {
        Ok(event) => success!(event),
        Err(error) => api_error(error),
    }
}

pub(crate) fn engine_subscribe(handle: u64) -> u64 {
    let engine = match get(engines(), handle) {
        Some(engine) => engine,
        None => return invalid_handle("engine", handle),
    };
    let inner = match engine.subscribe() {
        Ok(subscription) => subscription,
        Err(error) => return api_error(error),
    };
    let subscription = Subscription {
        inner,
        closed: false,
    };
    let handle = insert(subscriptions(), Arc::new(Mutex::new(subscription)));
    success!(json!({ "handle": handle }))
}

pub(crate) fn client_new(port: u16, token: &[u8], token_path: &[u8]) -> u64 {
    if !token.is_empty() && !token_path.is_empty() {
        return invalid_argument("token and token path are mutually exclusive");
    }
    let token = match std::str::from_utf8(token) {
        Ok(token) => token,
        Err(error) => return invalid_argument(format!("token is not UTF-8: {error}")),
    };
    let token_path = match std::str::from_utf8(token_path) {
        Ok(path) => path,
        Err(error) => return invalid_argument(format!("token path is not UTF-8: {error}")),
    };
    let client = if !token.is_empty() {
        Client::new(port, token)
    } else {
        let path = if token_path.is_empty() {
            Client::default_token_path()
        } else {
            PathBuf::from(token_path)
        };
        match Client::from_token_file(port, path) {
            Ok(client) => client,
            Err(error) => return api_error(error),
        }
    };
    let handle = insert(clients(), Arc::new(client));
    success!(json!({ "handle": handle }))
}

pub(crate) fn client_free(handle: u64) {
    remove(clients(), handle);
}

pub(crate) fn client_call(handle: u64, request: &[u8]) -> u64 {
    let client = match get(clients(), handle) {
        Some(client) => client,
        None => return invalid_handle("client", handle),
    };
    let request = match serde_json::from_slice::<Value>(request) {
        Ok(request) => request,
        Err(error) => {
            return binding_error(
                "invalid_request",
                "request is not valid JSON",
                error.to_string(),
            );
        }
    };
    match client.call_json(request) {
        Ok(response) => success!(response),
        Err(error) => api_error(error),
    }
}

pub(crate) fn client_status(handle: u64) -> u64 {
    let client = match get(clients(), handle) {
        Some(client) => client,
        None => return invalid_handle("client", handle),
    };
    match client.status() {
        Ok(status) => success!(status),
        Err(error) => api_error(error),
    }
}

pub(crate) fn client_events(handle: u64) -> u64 {
    let client = match get(clients(), handle) {
        Some(client) => client,
        None => return invalid_handle("client", handle),
    };
    let inner = match client.subscribe() {
        Ok(subscription) => subscription,
        Err(error) => return api_error(error),
    };
    let subscription = Subscription {
        inner,
        closed: false,
    };
    let handle = insert(subscriptions(), Arc::new(Mutex::new(subscription)));
    success!(json!({ "handle": handle }))
}

pub(crate) fn events_next(handle: u64, timeout_ms: u64) -> u64 {
    let subscription = match get(subscriptions(), handle) {
        Some(subscription) => subscription,
        None => return invalid_handle("subscription", handle),
    };
    let timeout = match timeout(timeout_ms) {
        Ok(timeout) => timeout,
        Err(detail) => return invalid_argument(detail),
    };
    let mut subscription = lock(&subscription);
    let dropped = subscription.inner.dropped.get();
    if subscription.closed {
        return success!(event_data(None, true, dropped));
    }
    match subscription.inner.receiver.recv_timeout(timeout) {
        Ok(event) => success!(event_data(
            Some(serde_json::to_value(event).expect("event serialization")),
            false,
            dropped,
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            success!(event_data(None, false, dropped))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            subscription.closed = true;
            success!(event_data(None, true, dropped))
        }
    }
}

fn event_data(event: Option<Value>, closed: bool, dropped: u64) -> Value {
    json!({ "event": event, "closed": closed, "dropped": dropped })
}

pub(crate) fn events_free(handle: u64) {
    remove(subscriptions(), handle);
}

pub(crate) fn result_len(handle: u64) -> usize {
    get(results(), handle).map_or(0, |bytes| bytes.len())
}

pub(crate) fn result_take(handle: u64, buffer: &mut [u8]) -> i32 {
    take_result(results(), handle, buffer)
}

fn take_result(table: &Mutex<HashMap<u64, Arc<Vec<u8>>>>, handle: u64, buffer: &mut [u8]) -> i32 {
    let mut results = lock(table);
    let Some(bytes) = results.get(&handle) else {
        return 1;
    };
    if buffer.len() < bytes.len() {
        return 2;
    }
    buffer[..bytes.len()].copy_from_slice(bytes);
    results.remove(&handle);
    0
}

pub(crate) fn result_free(handle: u64) {
    remove(results(), handle);
}

fn timeout(milliseconds: u64) -> Result<Duration, String> {
    let duration = Duration::from_millis(milliseconds);
    if Instant::now().checked_add(duration).is_none() {
        Err(format!("timeout {milliseconds} ms is out of range"))
    } else {
        Ok(duration)
    }
}

fn invalid_handle(kind: &str, handle: u64) -> u64 {
    let detail = format!("unknown or freed {kind} handle {handle}");
    binding_error("invalid_handle", &detail, &detail)
}

fn api_error(error: Error) -> u64 {
    store_value(json!({ "ok": false, "error": secs::WireError::from(&error) }))
}

fn binding_error(code: &str, message: &str, detail: impl Into<String>) -> u64 {
    store_value(json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
            "detail": detail.into(),
            "params": {},
            "retryable": false
        }
    }))
}

fn store_value(value: Value) -> u64 {
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|_| {
        br#"{"ok":false,"error":{"code":"internal","message":"could not encode result","detail":"JSON serialization failed","params":{},"retryable":false}}"#.to_vec()
    });
    store_result(results(), bytes)
}

fn store_result(table: &Mutex<HashMap<u64, Arc<Vec<u8>>>>, bytes: Vec<u8>) -> u64 {
    let mut results = lock(table);
    if results.len() >= 1024
        && let Some(oldest) = results.keys().copied().min()
    {
        results.remove(&oldest);
    }
    let handle = next_handle();
    results.insert(handle, Arc::new(bytes));
    handle
}

fn next_handle() -> u64 {
    loop {
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        if handle != 0 {
            return handle;
        }
    }
}

fn insert<T>(table: &Mutex<HashMap<u64, Arc<T>>>, value: Arc<T>) -> u64 {
    let handle = next_handle();
    lock(table).insert(handle, value);
    handle
}

fn get<T>(table: &Mutex<HashMap<u64, Arc<T>>>, handle: u64) -> Option<Arc<T>> {
    lock(table).get(&handle).cloned()
}

fn remove<T>(table: &Mutex<HashMap<u64, Arc<T>>>, handle: u64) {
    lock(table).remove(&handle);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn engines() -> &'static Mutex<HashMap<u64, Arc<Engine>>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clients() -> &'static Mutex<HashMap<u64, Arc<Client>>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn subscriptions() -> &'static Mutex<HashMap<u64, Arc<Mutex<Subscription>>>> {
    SUBSCRIPTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn results() -> &'static Mutex<HashMap<u64, Arc<Vec<u8>>>> {
    RESULTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use std::fs;
    use std::net::TcpListener;
    use std::ptr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use super::*;

    static TEMP_ID: AtomicU64 = AtomicU64::new(1);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("secs-dotnet-sys-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create temporary directory");
            Self(path)
        }

        fn settings(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn take_json(handle: u64) -> Value {
        let length = result_len(handle);
        assert_ne!(length, 0);
        let mut bytes = vec![0; length];
        assert_eq!(result_take(handle, &mut bytes), 0);
        serde_json::from_slice(&bytes).expect("result JSON")
    }

    fn new_engine(temp: &TempDir) -> u64 {
        let settings = temp.settings();
        let path = settings.to_string_lossy();
        let result = take_json(engine_new(path.as_bytes()));
        result["data"]["handle"].as_u64().expect("engine handle")
    }

    #[test]
    fn results_table_is_capped_and_evicts_the_oldest_td205() {
        // Isolate the result table from the other ABI tests running in parallel.
        let table = Mutex::new(HashMap::new());
        let first = store_result(&table, br#"{"ok":true,"data":0}"#.to_vec());
        let mut newest = first;
        for _ in 0..1024 {
            newest = store_result(&table, br#"{"ok":true,"data":1024}"#.to_vec());
        }
        let mut buffer = [0; 64];
        // The result-copy ABI reports invalid handles with status 1, not JSON.
        assert_eq!(take_result(&table, first, &mut buffer), 1);
        assert_eq!(lock(&table).len(), 1024);
        assert_eq!(take_result(&table, newest, &mut buffer), 0);
        assert!(buffer.starts_with(br#"{"ok":true,"data":1024}"#));
    }

    #[test]
    fn unknown_engine_handle_is_an_error_and_free_is_a_no_op() {
        engine_free(u64::MAX);
        let result = take_json(engine_call(u64::MAX, br#"{"method":"bootstrap"}"#));
        assert_eq!(result["error"]["code"], "invalid_handle");
        assert!(
            result["error"]["detail"]
                .as_str()
                .unwrap()
                .contains("engine")
        );
    }

    #[test]
    fn pointer_validation_accepts_only_representable_buffers() {
        assert!(validate_pointer::<u8>(ptr::null(), 0).is_ok());
        assert!(validate_pointer::<u8>(ptr::null(), 5).is_err());
        let pointer = std::ptr::NonNull::<u8>::dangling().as_ptr();
        assert!(validate_pointer(pointer, isize::MAX as usize + 1).is_err());
    }

    #[test]
    fn invalid_json_is_an_invalid_request() {
        let temp = TempDir::new();
        let engine = new_engine(&temp);
        let result = take_json(engine_call(engine, b"not JSON"));
        assert_eq!(result["error"]["code"], "invalid_request");
        engine_free(engine);
    }

    #[test]
    fn bootstrap_reports_the_native_crate_version() {
        let temp = TempDir::new();
        let engine = new_engine(&temp);
        let result = take_json(engine_call(engine, br#"{"method":"bootstrap"}"#));
        assert_eq!(result["data"]["method"], "bootstrap");
        assert_eq!(result["data"]["data"]["version"], env!("CARGO_PKG_VERSION"));
        engine_free(engine);
    }

    #[test]
    fn fresh_engine_results_preserve_structured_api_error() {
        let temp = TempDir::new();
        let engine = new_engine(&temp);
        let result = take_json(engine_call(engine, br#"{"method":"results"}"#));
        assert_eq!(result["error"]["code"], "project_not_open");
        assert!(!result["error"]["message"].as_str().unwrap().is_empty());
        assert!(!result["error"]["detail"].as_str().unwrap().is_empty());
        engine_free(engine);
    }

    #[test]
    fn short_result_buffer_keeps_the_result() {
        let handle = success!(json!({ "answer": 42 }));
        let length = result_len(handle);
        assert!(length > 1);
        assert_eq!(result_take(handle, &mut [0]), 2);
        assert_eq!(result_len(handle), length);
        result_free(handle);
    }

    #[test]
    fn fresh_engine_subscription_times_out_without_closing() {
        let temp = TempDir::new();
        let engine = new_engine(&temp);
        let subscription_result = take_json(engine_subscribe(engine));
        let subscription = subscription_result["data"]["handle"].as_u64().unwrap();
        let result = take_json(events_next(subscription, 1));
        assert!(result["data"]["event"].is_null());
        assert_eq!(result["data"]["closed"], false);
        assert_eq!(result["data"]["dropped"], 0);
        events_free(subscription);
        engine_free(engine);
    }

    #[test]
    fn closed_port_client_call_is_a_transport_error_within_five_seconds() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let client_result = take_json(client_new(port, b"test", b""));
        let client = client_result["data"]["handle"].as_u64().unwrap();
        let started = Instant::now();
        let result = take_json(client_call(client, br#"{"method":"bootstrap"}"#));
        assert_eq!(result["error"]["code"], "transport_error");
        assert!(started.elapsed() < Duration::from_secs(5));
        client_free(client);
    }

    #[test]
    fn guard_turns_a_panic_into_an_internal_error() {
        let result = guard(|| panic!("guard test panic"), panic_result);
        let result = take_json(result);
        assert_eq!(result["error"]["code"], "internal");
        assert!(
            result["error"]["detail"]
                .as_str()
                .unwrap()
                .contains("guard test panic")
        );
    }
}

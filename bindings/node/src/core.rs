use std::any::Any;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

use secs::types::ipc::{Envelope, IpcError, JobEvent, JobId};
use secs::types::local_api::{ServerEvent, Status as ServerStatus};
use secs::{Error as RustError, Response};
use serde_json::{Value, json};

pub(crate) const EVENT_QUEUE_CAPACITY: usize = 1024;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SecsErrorData {
    pub(crate) code: String,
    pub(crate) detail: String,
    pub(crate) params: Box<Value>,
    pub(crate) retryable: bool,
}

impl SecsErrorData {
    pub(crate) fn invalid_argument(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            code: "invalid_argument".to_owned(),
            detail,
            params: Box::new(json!({})),
            retryable: false,
        }
    }

    pub(crate) fn internal(detail: impl Into<String>) -> Self {
        Self {
            code: "internal".to_owned(),
            detail: detail.into(),
            params: Box::new(json!({})),
            retryable: false,
        }
    }

    pub(crate) fn json_line(&self) -> String {
        serde_json::to_string(&json!({
            "code": self.code,
            "detail": self.detail,
            "params": self.params,
            "retryable": self.retryable,
        }))
        .unwrap_or_else(|_| {
            r#"{"code":"internal","detail":"could not encode error","params":{},"retryable":false}"#
                .to_owned()
        })
    }
}

impl From<IpcError> for SecsErrorData {
    fn from(error: IpcError) -> Self {
        let code = serde_json::to_value(error.code)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "internal".to_owned());
        Self {
            code,
            detail: error.detail,
            params: Box::new(json!(error.params)),
            retryable: error.retryable,
        }
    }
}

impl From<RustError> for SecsErrorData {
    fn from(error: RustError) -> Self {
        match error {
            RustError::Api(error) => error.into(),
            RustError::Transport(detail) => Self {
                code: "transport_error".to_owned(),
                detail,
                params: Box::new(json!({})),
                retryable: false,
            },
            RustError::Protocol(detail) => Self {
                code: "protocol_error".to_owned(),
                detail,
                params: Box::new(json!({})),
                retryable: false,
            },
        }
    }
}

fn serialization_error(error: serde_json::Error) -> SecsErrorData {
    SecsErrorData::internal(format!("could not encode JSON: {error}"))
}

pub(crate) fn unwrap_envelope(envelope: Envelope) -> Result<Value, SecsErrorData> {
    match envelope {
        Envelope::Ok(response) => serialize_response(*response),
        Envelope::Err(error) => Err(error.into()),
    }
}

pub(crate) fn serialize_response(response: Response) -> Result<Value, SecsErrorData> {
    serde_json::to_value(response).map_err(serialization_error)
}

pub(crate) fn serialize_job_event(event: JobEvent) -> Result<Value, SecsErrorData> {
    serde_json::to_value(event).map_err(serialization_error)
}

pub(crate) fn serialize_server_event(event: ServerEvent) -> Result<Value, SecsErrorData> {
    serde_json::to_value(event).map_err(serialization_error)
}

pub(crate) fn serialize_status(status: ServerStatus) -> Result<Value, SecsErrorData> {
    serde_json::to_value(status).map_err(serialization_error)
}

pub(crate) fn validate_wait(
    job_id: f64,
    timeout_ms: f64,
) -> Result<(JobId, Duration), SecsErrorData> {
    if !job_id.is_finite() || job_id < 0.0 || job_id.fract() != 0.0 || job_id > MAX_SAFE_INTEGER {
        return Err(SecsErrorData::invalid_argument(
            "jobId must be a safe non-negative integer",
        ));
    }
    if !timeout_ms.is_finite() || timeout_ms < 0.0 {
        return Err(SecsErrorData::invalid_argument(
            "timeoutMs must be a finite non-negative number",
        ));
    }
    let timeout = Duration::try_from_secs_f64(timeout_ms / 1000.0).map_err(|error| {
        SecsErrorData::invalid_argument(format!("timeoutMs is out of range: {error}"))
    })?;
    Ok((JobId(job_id as u64), timeout))
}

pub(crate) fn validate_client_options(
    token: &Option<String>,
    token_path: &Option<String>,
) -> Result<(), SecsErrorData> {
    if token.is_some() && token_path.is_some() {
        Err(SecsErrorData::invalid_argument(
            "token and tokenPath are mutually exclusive",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn guarded<T>(
    operation: impl FnOnce() -> Result<T, SecsErrorData>,
) -> Result<T, SecsErrorData> {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|payload| {
        Err(SecsErrorData::internal(format!(
            "native binding panicked: {}",
            panic_message(payload.as_ref())
        )))
    })
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

#[derive(Debug)]
struct QueueState<T> {
    items: VecDeque<T>,
    closed: bool,
    dropped: u64,
}

#[derive(Debug)]
pub(crate) struct EventQueue<T> {
    capacity: usize,
    state: Mutex<QueueState<T>>,
    changed: Condvar,
}

impl<T> EventQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "event queue capacity must be positive");
        Self {
            capacity,
            state: Mutex::new(QueueState {
                items: VecDeque::with_capacity(capacity),
                closed: false,
                dropped: 0,
            }),
            changed: Condvar::new(),
        }
    }

    pub(crate) fn push(&self, item: T) {
        let mut state = lock(&self.state);
        if state.closed {
            return;
        }
        if state.items.len() == self.capacity {
            state.items.pop_front();
            state.dropped = state.dropped.saturating_add(1);
        }
        state.items.push_back(item);
        self.changed.notify_one();
    }

    pub(crate) fn next(&self) -> Option<T> {
        let mut state = lock(&self.state);
        loop {
            if let Some(item) = state.items.pop_front() {
                return Some(item);
            }
            if state.closed {
                return None;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    pub(crate) fn finish(&self) {
        lock(&self.state).closed = true;
        self.changed.notify_all();
    }

    pub(crate) fn close(&self) {
        let mut state = lock(&self.state);
        state.closed = true;
        state.items.clear();
        self.changed.notify_all();
    }

    pub(crate) fn dropped(&self) -> u64 {
        lock(&self.state).dropped
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use secs::types::ipc::{ErrorCode, IpcError, Response};

    use super::*;

    #[test]
    fn rejects_invalid_job_and_timeout() {
        for job in [-1.0, 1.5, f64::INFINITY, MAX_SAFE_INTEGER + 1.0] {
            assert_eq!(
                validate_wait(job, 1.0).unwrap_err().code,
                "invalid_argument"
            );
        }
        for timeout in [-1.0, f64::INFINITY, f64::NAN] {
            assert_eq!(
                validate_wait(1.0, timeout).unwrap_err().code,
                "invalid_argument"
            );
        }
        let (job, timeout) = validate_wait(MAX_SAFE_INTEGER, 1.5).unwrap();
        assert_eq!(job.0, MAX_SAFE_INTEGER as u64);
        assert_eq!(timeout, Duration::from_micros(1500));
    }

    #[test]
    fn unwraps_success_and_preserves_error() {
        let success = unwrap_envelope(Envelope::Ok(Box::new(Response::LevelStop(())))).unwrap();
        assert_eq!(success, json!({ "method": "level_stop", "data": null }));

        let mut params = BTreeMap::new();
        params.insert("path".to_owned(), "room.secs".to_owned());
        let failure = unwrap_envelope(Envelope::Err(IpcError {
            code: ErrorCode::ProjectIo,
            params,
            retryable: true,
            detail: "disk failed".to_owned(),
        }))
        .unwrap_err();
        assert_eq!(failure.code, "project_io");
        assert_eq!(failure.detail, "disk failed");
        assert_eq!(*failure.params, json!({ "path": "room.secs" }));
        assert!(failure.retryable);
    }

    #[test]
    fn bounded_queue_drops_oldest_and_counts() {
        let queue = EventQueue::new(2);
        queue.push(1);
        queue.push(2);
        queue.push(3);
        assert_eq!(queue.dropped(), 1);
        assert_eq!(queue.next(), Some(2));
        assert_eq!(queue.next(), Some(3));
    }

    #[test]
    fn closed_queue_returns_none() {
        let queue = Arc::new(EventQueue::new(2));
        queue.push(1);
        queue.close();
        assert_eq!(queue.next(), None);
    }

    #[test]
    fn token_and_path_are_mutually_exclusive() {
        assert!(validate_client_options(&None, &None).is_ok());
        assert!(validate_client_options(&Some("token".to_owned()), &None).is_ok());
        let error =
            validate_client_options(&Some("token".to_owned()), &Some("token.txt".to_owned()))
                .unwrap_err();
        assert_eq!(error.code, "invalid_argument");
    }
}

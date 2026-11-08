#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use secs::types::ipc::{Envelope, JobId};
use secs::{
    Client as RustClient, DroppedEventCount, Engine as RustEngine, EngineBuilder,
    Error as RustError, Event, SubscriptionCloser,
};
use serde_json::Value;

macro_rules! rust_to_python {
    ($py:expr, $value:expr) => {{
        serde_json::to_value($value)
            .map_err(|error| PyValueError::new_err(format!("could not encode JSON: {error}")))
            .and_then(|value| json_to_python($py, value))
    }};
}

create_exception!(_secs2r, SecsError, PyException);

#[pyclass(name = "Engine", module = "secs2r._secs2r")]
struct Engine {
    inner: RustEngine,
}

#[pymethods]
impl Engine {
    #[new]
    #[pyo3(signature = (app_settings_path=None))]
    fn new(app_settings_path: Option<PathBuf>) -> Self {
        let builder = EngineBuilder::default();
        let inner = match app_settings_path {
            Some(path) => builder.app_settings_path(path).build(),
            None => builder.build(),
        };
        Self { inner }
    }

    fn call(&self, py: Python<'_>, request: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let request = python_to_json(py, request)?;
        let envelope = py.detach(|| self.inner.call_json(request));
        match envelope {
            Envelope::Ok(response) => rust_to_python!(py, *response),
            Envelope::Err(error) => Err(secs_error(py, RustError::Api(error))),
        }
    }

    fn wait_job(&self, py: Python<'_>, job_id: u64, timeout: f64) -> PyResult<Py<PyAny>> {
        let timeout = Duration::try_from_secs_f64(timeout)
            .map_err(|error| PyValueError::new_err(format!("invalid timeout: {error}")))?;
        let event = py
            .detach(|| self.inner.wait_job(JobId(job_id), timeout))
            .map_err(|error| secs_error(py, error))?;
        rust_to_python!(py, event)
    }

    #[pyo3(signature = (timeout=None))]
    fn events(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Events> {
        let timeout = event_timeout(timeout)?;
        let subscription = self
            .inner
            .subscribe()
            .map_err(|error| secs_error(py, error))?;
        Ok(Events::new(subscription, timeout))
    }
}

#[pyclass(name = "_Events", module = "secs2r._secs2r")]
struct Events {
    receiver: EventReceiver<Event>,
    dropped: DroppedEventCount,
    closer: SubscriptionCloser,
}

impl Events {
    fn new(subscription: secs::Subscription, timeout: Option<Duration>) -> Self {
        Self {
            receiver: EventReceiver::new(subscription.receiver, timeout),
            dropped: subscription.dropped,
            closer: subscription.closer,
        }
    }
}

#[pymethods]
impl Events {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn dropped_events(&self) -> u64 {
        self.dropped.get()
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.receiver.next(py) {
            Ok(Some(event)) => rust_to_python!(py, event).map(Some),
            Ok(None) => {
                self.close();
                Ok(None)
            }
            Err(error) => {
                self.close();
                Err(error)
            }
        }
    }

    fn close(&self) {
        self.closer.close();
        self.receiver.close();
    }

    #[getter]
    fn closed(&self) -> bool {
        self.receiver.closed.load(Ordering::Acquire)
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        self.close();
    }
}

#[pyclass(name = "Client", module = "secs2r._secs2r")]
struct Client {
    inner: RustClient,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (port=4712, token=None, token_path=None))]
    fn new(
        py: Python<'_>,
        port: u16,
        token: Option<String>,
        token_path: Option<PathBuf>,
    ) -> PyResult<Self> {
        if token.is_some() && token_path.is_some() {
            return Err(PyValueError::new_err(
                "token and token_path are mutually exclusive",
            ));
        }
        let inner = if let Some(token) = token {
            RustClient::new(port, token)
        } else {
            let path = token_path.unwrap_or_else(RustClient::default_token_path);
            py.detach(|| RustClient::from_token_file(port, path))
                .map_err(|error| secs_error(py, error))?
        };
        Ok(Self { inner })
    }

    fn call(&self, py: Python<'_>, request: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let request = python_to_json(py, request)?;
        let response = py
            .detach(|| self.inner.call_json(request))
            .map_err(|error| secs_error(py, error))?;
        rust_to_python!(py, response)
    }

    fn status(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let status = py
            .detach(|| self.inner.status())
            .map_err(|error| secs_error(py, error))?;
        rust_to_python!(py, status)
    }

    #[pyo3(signature = (timeout=None))]
    fn events(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Events> {
        let timeout = event_timeout(timeout)?;
        let subscription = py
            .detach(|| self.inner.subscribe())
            .map_err(|error| secs_error(py, error))?;
        Ok(Events::new(subscription, timeout))
    }
}

fn event_timeout(timeout: Option<f64>) -> PyResult<Option<Duration>> {
    timeout
        .map(|seconds| {
            let duration = Duration::try_from_secs_f64(seconds)
                .map_err(|error| PyValueError::new_err(format!("invalid timeout: {error}")))?;
            if Instant::now().checked_add(duration).is_none() {
                return Err(PyValueError::new_err("timeout is out of range"));
            }
            Ok(duration)
        })
        .transpose()
}

struct EventReceiver<T> {
    receiver: Mutex<Option<mpsc::Receiver<T>>>,
    closed: AtomicBool,
    timeout: Option<Duration>,
}

impl<T: Send> EventReceiver<T> {
    fn new(receiver: mpsc::Receiver<T>, timeout: Option<Duration>) -> Self {
        Self {
            receiver: Mutex::new(Some(receiver)),
            closed: AtomicBool::new(false),
            timeout,
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    fn next(&self, py: Python<'_>) -> PyResult<Option<T>> {
        let started = Instant::now();
        loop {
            py.check_signals()?;
            if self.closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            let remaining = self
                .timeout
                .map(|timeout| timeout.saturating_sub(started.elapsed()));
            let poll = remaining
                .unwrap_or(Duration::from_millis(100))
                .min(Duration::from_millis(100));
            let result = py.detach(|| {
                let receiver = self
                    .receiver
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                receiver
                    .as_ref()
                    .map(|receiver| receiver.recv_timeout(poll))
            });
            py.check_signals()?;
            if self.closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            match result {
                Some(Ok(event)) => return Ok(Some(event)),
                Some(Err(mpsc::RecvTimeoutError::Timeout))
                    if self
                        .timeout
                        .is_none_or(|timeout| started.elapsed() < timeout) => {}
                Some(Err(mpsc::RecvTimeoutError::Timeout)) => {
                    return Err(PyTimeoutError::new_err("event wait timed out"));
                }
                _ => {
                    self.close();
                    return Ok(None);
                }
            }
        }
    }
}

// The stdlib JSON round trip keeps Python on the same wire contract as the
// Rust API without adding another Python/Rust conversion dependency.
fn python_to_json(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let json = py.import("json")?;
    let text: String = json.call_method1("dumps", (value,))?.extract()?;
    serde_json::from_str(&text)
        .map_err(|error| PyValueError::new_err(format!("request is not valid JSON: {error}")))
}

fn json_to_python(py: Python<'_>, value: Value) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(&value)
        .map_err(|error| PyValueError::new_err(format!("could not encode JSON: {error}")))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

fn secs_error(py: Python<'_>, error: RustError) -> PyErr {
    let wire = secs::WireError::from(&error);
    let message = wire.message;
    let exception = SecsError::new_err(message.clone());
    let value = exception.value(py);
    if let Err(error) = value.setattr("code", wire.code) {
        return error;
    }
    if let Err(error) = value.setattr("message", message) {
        return error;
    }
    if let Err(error) = value.setattr("detail", wire.detail) {
        return error;
    }
    let params = match rust_to_python!(py, wire.params) {
        Ok(params) => params,
        Err(error) => return error,
    };
    if let Err(error) = value.setattr("params", params) {
        return error;
    }
    if let Err(error) = value.setattr("retryable", wire.retryable) {
        return error;
    }
    exception
}

#[pymodule]
fn _secs2r(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("SecsError", py.get_type::<SecsError>())?;
    module.add_class::<Engine>()?;
    module.add_class::<Events>()?;
    module.add_class::<Client>()?;
    Ok(())
}

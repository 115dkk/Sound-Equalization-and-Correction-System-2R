#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use secs::types::ipc::{Envelope, JobEvent, JobId};
use secs::{
    Client as RustClient, DroppedEventCount, Engine as RustEngine, EngineBuilder,
    Error as RustError,
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

    fn events(&self) -> EngineEvents {
        let (receiver, dropped) = self.inner.subscribe_with_drop_count();
        EngineEvents {
            receiver: Mutex::new(receiver),
            dropped,
        }
    }
}

#[pyclass(name = "_EngineEvents", module = "secs2r._secs2r")]
struct EngineEvents {
    receiver: Mutex<mpsc::Receiver<JobEvent>>,
    dropped: DroppedEventCount,
}

#[pymethods]
impl EngineEvents {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn dropped_events(&self) -> u64 {
        self.dropped.get()
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match py.detach(|| {
            self.receiver
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .recv()
        }) {
            Ok(event) => rust_to_python!(py, event).map(Some),
            Err(_) => Ok(None),
        }
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

    fn events(&self, py: Python<'_>) -> PyResult<ClientEvents> {
        let events = py
            .detach(|| self.inner.events())
            .map_err(|error| secs_error(py, error))?;
        Ok(ClientEvents { inner: events })
    }
}

#[pyclass(name = "_ClientEvents", module = "secs2r._secs2r")]
struct ClientEvents {
    inner: secs::Events,
}

#[pymethods]
impl ClientEvents {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match py.detach(|| self.inner.next()) {
            Some(Ok(event)) => rust_to_python!(py, event).map(Some),
            Some(Err(error)) => Err(secs_error(py, error)),
            None => Ok(None),
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
    let (code, detail) = match &error {
        RustError::Api(error) => {
            let code = serde_json::to_value(error.code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "internal".to_owned());
            (code, Some(error.detail.clone()))
        }
        RustError::Transport(detail) => ("transport_error".to_owned(), Some(detail.clone())),
        RustError::Protocol(detail) => ("protocol_error".to_owned(), Some(detail.clone())),
    };
    let message = error.to_string();
    let exception = SecsError::new_err(message.clone());
    let value = exception.value(py);
    if let Err(error) = value.setattr("code", code) {
        return error;
    }
    if let Err(error) = value.setattr("message", message) {
        return error;
    }
    if let Err(error) = value.setattr("detail", detail) {
        return error;
    }
    exception
}

#[pymodule]
fn _secs2r(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("SecsError", py.get_type::<SecsError>())?;
    module.add_class::<Engine>()?;
    module.add_class::<EngineEvents>()?;
    module.add_class::<Client>()?;
    module.add_class::<ClientEvents>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_event_count_does_not_lock_the_blocking_receiver_td127() {
        let (_sender, receiver) = mpsc::channel();
        let events = EngineEvents {
            receiver: Mutex::new(receiver),
            dropped: DroppedEventCount::default(),
        };
        let _receiver = events
            .receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(events.dropped_events(), 0);
    }
}

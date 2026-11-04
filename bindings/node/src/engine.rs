use std::path::PathBuf;
use std::sync::Arc;

use napi::Result;
use napi_derive::napi;
use secs::{Engine as RustEngine, EngineBuilder};
use serde_json::Value;

use crate::core::{guarded, serialize_job_event, unwrap_envelope, validate_wait};
use crate::events::{EngineEvents, napi_error};

#[napi]
#[derive(Debug)]
pub struct Engine {
    inner: Arc<RustEngine>,
}

#[napi]
impl Engine {
    #[napi(factory)]
    pub async fn create(app_settings_path: Option<String>) -> Result<Self> {
        let inner = tokio::task::spawn_blocking(move || {
            guarded(|| {
                let builder = EngineBuilder::default();
                Ok(match app_settings_path {
                    Some(path) => builder.app_settings_path(PathBuf::from(path)).build(),
                    None => builder.build(),
                })
            })
        })
        .await
        .map_err(|error| {
            napi_error(crate::core::SecsErrorData::internal(format!(
                "engine creation task failed: {error}"
            )))
        })?
        .map_err(napi_error)?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    #[napi]
    pub async fn call(&self, request: Value) -> Result<Value> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || guarded(|| unwrap_envelope(inner.call_json(request))))
            .await
            .map_err(|error| {
                napi_error(crate::core::SecsErrorData::internal(format!(
                    "engine call task failed: {error}"
                )))
            })?
            .map_err(napi_error)
    }

    #[napi(js_name = "waitJob")]
    pub async fn wait_job(&self, job_id: f64, timeout_ms: f64) -> Result<Value> {
        let (job_id, timeout) = validate_wait(job_id, timeout_ms).map_err(napi_error)?;
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            guarded(|| {
                inner
                    .wait_job(job_id, timeout)
                    .map_err(Into::into)
                    .and_then(serialize_job_event)
            })
        })
        .await
        .map_err(|error| {
            napi_error(crate::core::SecsErrorData::internal(format!(
                "job wait task failed: {error}"
            )))
        })?
        .map_err(napi_error)
    }

    #[napi]
    pub fn events(&self) -> Result<EngineEvents> {
        guarded(|| {
            let (receiver, dropped) = self.inner.subscribe_with_drop_count();
            Ok(EngineEvents::new(receiver, dropped))
        })
        .map_err(napi_error)
    }
}

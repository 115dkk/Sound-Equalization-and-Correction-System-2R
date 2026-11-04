use std::path::PathBuf;
use std::sync::Arc;

use napi::Result;
use napi_derive::napi;
use secs::Client as RustClient;
use serde_json::Value;

use crate::core::{
    SecsErrorData, guarded, serialize_response, serialize_status, validate_client_options,
};
use crate::events::{ClientEvents, napi_error};

const DEFAULT_PORT: u16 = 4712;

#[napi]
#[derive(Debug)]
pub struct Client {
    inner: Arc<RustClient>,
}

#[napi]
impl Client {
    #[napi(factory)]
    pub async fn create(
        port: Option<u16>,
        token: Option<String>,
        token_path: Option<String>,
    ) -> Result<Self> {
        validate_client_options(&token, &token_path).map_err(napi_error)?;
        let inner = tokio::task::spawn_blocking(move || {
            guarded(|| {
                let port = port.unwrap_or(DEFAULT_PORT);
                match token {
                    Some(token) => Ok(RustClient::new(port, token)),
                    None => {
                        let path = token_path
                            .map(PathBuf::from)
                            .unwrap_or_else(RustClient::default_token_path);
                        RustClient::from_token_file(port, path).map_err(Into::into)
                    }
                }
            })
        })
        .await
        .map_err(|error| {
            napi_error(SecsErrorData::internal(format!(
                "client creation task failed: {error}"
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
        tokio::task::spawn_blocking(move || {
            guarded(|| {
                inner
                    .call_json(request)
                    .map_err(Into::into)
                    .and_then(serialize_response)
            })
        })
        .await
        .map_err(|error| {
            napi_error(SecsErrorData::internal(format!(
                "client call task failed: {error}"
            )))
        })?
        .map_err(napi_error)
    }

    #[napi]
    pub async fn status(&self) -> Result<Value> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            guarded(|| {
                inner
                    .status()
                    .map_err(Into::into)
                    .and_then(serialize_status)
            })
        })
        .await
        .map_err(|error| {
            napi_error(SecsErrorData::internal(format!(
                "client status task failed: {error}"
            )))
        })?
        .map_err(napi_error)
    }

    #[napi]
    pub async fn events(&self) -> Result<ClientEvents> {
        let inner = Arc::clone(&self.inner);
        let (events, closer) = tokio::task::spawn_blocking(move || {
            guarded(|| {
                let events = inner.events().map_err(SecsErrorData::from)?;
                let closer = events.closer().map_err(SecsErrorData::from)?;
                Ok((events, closer))
            })
        })
        .await
        .map_err(|error| {
            napi_error(SecsErrorData::internal(format!(
                "client event subscription task failed: {error}"
            )))
        })?
        .map_err(napi_error)?;
        Ok(ClientEvents::new(events, closer))
    }
}

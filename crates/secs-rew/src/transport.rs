use crate::{CONNECT_TIMEOUT, RewError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::VecDeque, time::Duration};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
    #[serde(default)]
    pub query: Vec<(String, String)>,
    pub timeout: Duration,
}
impl Request {
    pub fn new(method: &str, path: &str, body: Option<Value>, timeout: Duration) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            body,
            query: vec![],
            timeout,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}
impl Response {
    pub fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }
}
pub trait Transport: Send + Sync {
    fn request(&self, request: &Request) -> Result<Response, RewError>;
}

#[derive(Debug, Clone)]
pub struct HttpTransport {
    base: String,
    agent: ureq::Agent,
}
impl Default for HttpTransport {
    fn default() -> Self {
        Self::new("http://127.0.0.1:4735").expect("literal HTTP address")
    }
}
impl HttpTransport {
    pub fn new(base: &str) -> Result<Self, RewError> {
        if !base.starts_with("http://") {
            return Err(RewError::schema("base", "REW requires plain HTTP"));
        }
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build();
        Ok(Self {
            base: base.trim_end_matches('/').into(),
            agent: config.into(),
        })
    }
}
impl Transport for HttpTransport {
    fn request(&self, request: &Request) -> Result<Response, RewError> {
        let url = format!("{}/{}", self.base, request.path.trim_start_matches('/'));
        let result = match request.method.as_str() {
            "GET" => self
                .agent
                .get(&url)
                .query_pairs(request.query.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .config()
                .timeout_connect(Some(CONNECT_TIMEOUT.min(request.timeout)))
                .timeout_recv_response(Some(request.timeout))
                .timeout_recv_body(Some(request.timeout))
                .build()
                .call(),
            "POST" => self
                .agent
                .post(&url)
                .query_pairs(request.query.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .header("Content-Type", "application/json")
                .config()
                .timeout_connect(Some(CONNECT_TIMEOUT.min(request.timeout)))
                .timeout_recv_response(Some(request.timeout))
                .timeout_recv_body(Some(request.timeout))
                .build()
                .send(
                    request
                        .body
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_default()
                        .as_bytes(),
                ),
            _ => return Err(RewError::schema(&request.path, "unsupported method")),
        };
        let mut response = result.map_err(|e| RewError::connection(&request.path, e))?;
        let status = response.status().as_u16();
        // IRs can exceed ureq's small convenience-reader default.
        let text = response
            .body_mut()
            .with_config()
            .limit(128 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| RewError::connection(&request.path, e))?;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok(Response { status, body })
    }
}

#[derive(Debug, Clone)]
pub struct RecordedExchange {
    pub request: Request,
    pub response: Result<Response, RewError>,
}
impl RecordedExchange {
    pub fn new(request: Request, body: Value) -> Self {
        Self {
            request,
            response: Ok(Response::ok(body)),
        }
    }
}
/// Strict scripted protocol oracle. Mismatches panic even when the caller would
/// normally swallow connection errors; always call `assert_finished` as well.
#[derive(Debug, Clone)]
pub struct RecordedTransport {
    state: std::sync::Arc<std::sync::Mutex<Recording>>,
}
#[derive(Debug)]
struct Recording {
    remaining: VecDeque<RecordedExchange>,
    calls: Vec<Request>,
}
impl RecordedTransport {
    pub fn new(script: impl IntoIterator<Item = RecordedExchange>) -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(Recording {
                remaining: script.into_iter().collect(),
                calls: vec![],
            })),
        }
    }
    pub fn push(&self, exchange: RecordedExchange) {
        self.state.lock().unwrap().remaining.push_back(exchange);
    }
    pub fn calls(&self) -> Vec<Request> {
        self.state.lock().unwrap().calls.clone()
    }
    pub fn assert_finished(&self) {
        assert!(
            self.state.lock().unwrap().remaining.is_empty(),
            "unconsumed REW requests"
        );
    }
}
impl Transport for RecordedTransport {
    fn request(&self, request: &Request) -> Result<Response, RewError> {
        let mut state = self.state.lock().unwrap();
        let expected = state
            .remaining
            .pop_front()
            .expect("unexpected REW request after script ended");
        assert_eq!(
            &expected.request, request,
            "REW request does not match next recorded request"
        );
        state.calls.push(request.clone());
        expected.response
    }
}

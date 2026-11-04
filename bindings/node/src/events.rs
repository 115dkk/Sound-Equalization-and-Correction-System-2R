use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use napi::bindgen_prelude::BigInt;
use napi::{Error, Result, Status};
use napi_derive::napi;
use secs::types::ipc::JobEvent;
use secs::{DroppedEventCount, Events as RustEvents, EventsCloser};
use serde_json::Value;

use crate::core::{
    EVENT_QUEUE_CAPACITY, EventQueue, SecsErrorData, guarded, serialize_job_event,
    serialize_server_event,
};

type EventItem = std::result::Result<Value, SecsErrorData>;

pub(crate) fn napi_error(error: SecsErrorData) -> Error {
    Error::new(Status::GenericFailure, error.json_line())
}

fn join_error(error: tokio::task::JoinError) -> Error {
    napi_error(SecsErrorData::internal(format!(
        "event wait task failed: {error}"
    )))
}

#[derive(Debug)]
struct EngineEventState {
    queue: Arc<EventQueue<EventItem>>,
    stop: Arc<AtomicBool>,
    upstream_dropped: DroppedEventCount,
}

impl EngineEventState {
    fn close(&self) {
        self.stop.store(true, Ordering::Release);
        self.queue.close();
    }
}

#[napi]
#[derive(Debug)]
pub struct EngineEvents {
    state: Arc<EngineEventState>,
}

impl EngineEvents {
    pub(crate) fn new(
        receiver: mpsc::Receiver<JobEvent>,
        upstream_dropped: DroppedEventCount,
    ) -> Self {
        let queue = Arc::new(EventQueue::new(EVENT_QUEUE_CAPACITY));
        let stop = Arc::new(AtomicBool::new(false));
        let pump_queue = Arc::clone(&queue);
        let pump_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let result = guarded(|| {
                loop {
                    if pump_stop.load(Ordering::Acquire) {
                        break;
                    }
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(event) => pump_queue.push(serialize_job_event(event)),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                pump_queue.push(Err(error));
            }
            pump_queue.finish();
        });
        Self {
            state: Arc::new(EngineEventState {
                queue,
                stop,
                upstream_dropped,
            }),
        }
    }
}

#[napi]
impl EngineEvents {
    #[napi]
    pub async fn next(&self) -> Result<Option<Value>> {
        let queue = Arc::clone(&self.state.queue);
        match tokio::task::spawn_blocking(move || queue.next())
            .await
            .map_err(join_error)?
        {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => Err(napi_error(error)),
            None => Ok(None),
        }
    }

    #[napi]
    pub fn close(&self) {
        self.state.close();
    }

    #[napi(js_name = "droppedEvents")]
    pub fn dropped_events(&self) -> BigInt {
        self.state
            .upstream_dropped
            .get()
            .saturating_add(self.state.queue.dropped())
            .into()
    }
}

impl Drop for EngineEvents {
    fn drop(&mut self) {
        self.state.close();
    }
}

#[derive(Debug)]
struct ClientEventState {
    queue: Arc<EventQueue<EventItem>>,
    stop: Arc<AtomicBool>,
    closer: EventsCloser,
}

impl ClientEventState {
    fn close(&self) {
        self.stop.store(true, Ordering::Release);
        self.closer.close();
        self.queue.close();
    }
}

#[napi]
#[derive(Debug)]
pub struct ClientEvents {
    state: Arc<ClientEventState>,
}

impl ClientEvents {
    pub(crate) fn new(mut events: RustEvents, closer: EventsCloser) -> Self {
        let queue = Arc::new(EventQueue::new(EVENT_QUEUE_CAPACITY));
        let stop = Arc::new(AtomicBool::new(false));
        let pump_queue = Arc::clone(&queue);
        let pump_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let result = guarded(|| {
                while !pump_stop.load(Ordering::Acquire) {
                    match events.next() {
                        Some(Ok(event)) => pump_queue.push(serialize_server_event(event)),
                        Some(Err(error)) => {
                            pump_queue.push(Err(error.into()));
                            break;
                        }
                        None => break,
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                pump_queue.push(Err(error));
            }
            pump_queue.finish();
        });
        Self {
            state: Arc::new(ClientEventState {
                queue,
                stop,
                closer,
            }),
        }
    }
}

#[napi]
impl ClientEvents {
    #[napi]
    pub async fn next(&self) -> Result<Option<Value>> {
        let queue = Arc::clone(&self.state.queue);
        match tokio::task::spawn_blocking(move || queue.next())
            .await
            .map_err(join_error)?
        {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => Err(napi_error(error)),
            None => Ok(None),
        }
    }

    #[napi]
    pub fn close(&self) {
        self.state.close();
    }
}

impl Drop for ClientEvents {
    fn drop(&mut self) {
        self.state.close();
    }
}

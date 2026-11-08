//! Unclaimed results retain at most 1024 envelopes. Storing another result
//! evicts the oldest handle; its length becomes zero and copying it returns
//! `SECS_RESULT_UNKNOWN`. Callers must take or free results promptly.
//!
//! `secs_events_next` holds the subscription lock while polling. Freeing its
//! handle returns without waiting, but a concurrent poll keeps the subscription
//! and socket alive until that poll finishes (up to its `timeout_ms`).

use std::slice;

use crate::core;

/// Result-copy status: all bytes were copied and the result was removed.
pub const SECS_RESULT_OK: i32 = 0;
/// Result-copy status: the result handle is unknown.
pub const SECS_RESULT_UNKNOWN: i32 = 1;
/// Result-copy status: the supplied buffer is too short.
pub const SECS_RESULT_BUFFER_TOO_SMALL: i32 = 2;
/// Result-copy status: the supplied buffer pointer or length is invalid.
pub const SECS_RESULT_INVALID_BUFFER: i32 = 3;

/// Creates an embedded engine.
///
/// # Safety
///
/// `app_settings_path` and its length must describe readable memory for the duration
/// of this call, unless the length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn secs_engine_new(
    app_settings_path: *const u8,
    app_settings_path_len: usize,
) -> u64 {
    core::guard(
        || {
            if let Err(detail) = core::validate_pointer(app_settings_path, app_settings_path_len) {
                return core::invalid_argument(detail);
            }
            let path = if app_settings_path_len == 0 {
                &[]
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is readable.
                unsafe { slice::from_raw_parts(app_settings_path, app_settings_path_len) }
            };
            core::engine_new(path)
        },
        core::panic_result,
    )
}

/// Frees an embedded engine handle. Unknown handles are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn secs_engine_free(engine: u64) {
    core::guard(|| core::engine_free(engine), |_| ());
}

/// Sends one JSON request to an embedded engine.
///
/// # Safety
///
/// `request` and its length must describe readable memory for the duration of this
/// call, unless the length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn secs_engine_call(
    engine: u64,
    request: *const u8,
    request_len: usize,
) -> u64 {
    core::guard(
        || {
            if let Err(detail) = core::validate_pointer(request, request_len) {
                return core::invalid_argument(detail);
            }
            let request = if request_len == 0 {
                &[]
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is readable.
                unsafe { slice::from_raw_parts(request, request_len) }
            };
            core::engine_call(engine, request)
        },
        core::panic_result,
    )
}

/// Waits for one engine job to reach a terminal state.
#[unsafe(no_mangle)]
pub extern "C" fn secs_engine_wait_job(engine: u64, job_id: u64, timeout_ms: u64) -> u64 {
    core::guard(
        || core::engine_wait_job(engine, job_id, timeout_ms),
        core::panic_result,
    )
}

/// Subscribes to later engine job events.
#[unsafe(no_mangle)]
pub extern "C" fn secs_engine_subscribe(engine: u64) -> u64 {
    core::guard(|| core::engine_subscribe(engine), core::panic_result)
}

/// Creates a local-API client.
///
/// # Safety
///
/// Each pointer and its length must describe readable memory for the duration of
/// this call, unless that length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn secs_client_new(
    port: u16,
    token: *const u8,
    token_len: usize,
    token_path: *const u8,
    token_path_len: usize,
) -> u64 {
    core::guard(
        || {
            if let Err(detail) = core::validate_pointer(token, token_len) {
                return core::invalid_argument(detail);
            }
            if let Err(detail) = core::validate_pointer(token_path, token_path_len) {
                return core::invalid_argument(detail);
            }
            let token = if token_len == 0 {
                &[]
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is readable.
                unsafe { slice::from_raw_parts(token, token_len) }
            };
            let token_path = if token_path_len == 0 {
                &[]
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is readable.
                unsafe { slice::from_raw_parts(token_path, token_path_len) }
            };
            core::client_new(port, token, token_path)
        },
        core::panic_result,
    )
}

/// Frees a local-API client handle. Unknown handles are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn secs_client_free(client: u64) {
    core::guard(|| core::client_free(client), |_| ());
}

/// Sends one JSON request to a local-API client.
///
/// # Safety
///
/// `request` and its length must describe readable memory for the duration of this
/// call, unless the length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn secs_client_call(
    client: u64,
    request: *const u8,
    request_len: usize,
) -> u64 {
    core::guard(
        || {
            if let Err(detail) = core::validate_pointer(request, request_len) {
                return core::invalid_argument(detail);
            }
            let request = if request_len == 0 {
                &[]
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is readable.
                unsafe { slice::from_raw_parts(request, request_len) }
            };
            core::client_call(client, request)
        },
        core::panic_result,
    )
}

/// Reads the running application's aggregate status.
#[unsafe(no_mangle)]
pub extern "C" fn secs_client_status(client: u64) -> u64 {
    core::guard(|| core::client_status(client), core::panic_result)
}

/// Opens a subscription to running-application events.
#[unsafe(no_mangle)]
pub extern "C" fn secs_client_events(client: u64) -> u64 {
    core::guard(|| core::client_events(client), core::panic_result)
}

/// Polls one subscription for its next event.
#[unsafe(no_mangle)]
pub extern "C" fn secs_events_next(subscription: u64, timeout_ms: u64) -> u64 {
    core::guard(
        || core::events_next(subscription, timeout_ms),
        core::panic_result,
    )
}

/// Frees an event-subscription handle. Unknown handles are ignored.
/// A concurrent poll can defer closing its socket by up to that poll's timeout.
#[unsafe(no_mangle)]
pub extern "C" fn secs_events_free(subscription: u64) {
    core::guard(|| core::events_free(subscription), |_| ());
}

/// Returns a result envelope's byte length, or zero for an unknown handle.
#[unsafe(no_mangle)]
pub extern "C" fn secs_result_len(result: u64) -> usize {
    core::guard(|| core::result_len(result), |_| 0)
}

/// Copies and removes a result envelope.
///
/// # Safety
///
/// `buffer` and its length must describe writable memory for the duration of this
/// call, unless the length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn secs_result_take(result: u64, buffer: *mut u8, buffer_len: usize) -> i32 {
    core::guard(
        || {
            if core::validate_pointer(buffer.cast_const(), buffer_len).is_err() {
                return SECS_RESULT_INVALID_BUFFER;
            }
            let buffer = if buffer_len == 0 {
                &mut []
            } else {
                // SAFETY: The caller promises this validated pointer-length pair is writable.
                unsafe { slice::from_raw_parts_mut(buffer, buffer_len) }
            };
            core::result_take(result, buffer)
        },
        |_| SECS_RESULT_INVALID_BUFFER,
    )
}

/// Frees a result envelope without copying it. Unknown handles are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn secs_result_free(result: u64) {
    core::guard(|| core::result_free(result), |_| ());
}

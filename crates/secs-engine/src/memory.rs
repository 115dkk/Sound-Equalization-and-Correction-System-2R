//! Reservations precede ordered pool work, including work with nested maps.

use secs_compute::{Context, MapError, MemoryError};

use crate::EngineError;

pub(crate) fn memory_error(error: MemoryError) -> EngineError {
    match error {
        MemoryError::Cancelled => EngineError::Cancelled,
        other => EngineError::Input(other.to_string()),
    }
}

pub(crate) fn try_map<T: Send, U: Send>(
    ctx: &Context,
    items: Vec<T>,
    retained: u64,
    bytes: impl Fn(&T) -> u64,
    f: impl Fn(T) -> Result<U, EngineError> + Sync + Send,
) -> Result<Vec<U>, EngineError> {
    let capacity = ctx.budget().total().saturating_sub(retained);
    let mut pending = items
        .into_iter()
        .map(|item| {
            let cost = bytes(&item);
            (item, cost)
        })
        .peekable();
    let mut output = Vec::new();
    while pending.peek().is_some() {
        let mut batch = Vec::new();
        let mut batch_bytes = 0u64;
        while let Some((_, cost)) = pending.peek() {
            if *cost > capacity {
                return Err(memory_error(MemoryError::TooLarge {
                    requested: retained.saturating_add(*cost),
                    total: ctx.budget().total(),
                }));
            }
            if *cost > capacity - batch_bytes {
                break;
            }
            let (item, cost) = pending.next().expect("peeked item");
            batch.push(item);
            batch_bytes += cost;
        }
        // Never acquire in a Rayon task: a nested map may execute another
        // input on the stack of the task that must release its permit (TD-23).
        let _permit = ctx
            .budget()
            .acquire(batch_bytes, ctx.cancel())
            .map_err(memory_error)?;
        output.extend(ctx.try_map(batch, &f).map_err(|error| match error {
            MapError::Cancelled => EngineError::Cancelled,
            MapError::Item { error, .. } => error,
        })?);
    }
    Ok(output)
}

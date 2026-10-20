//! Calculation-only sample-rate normalization. Stored measurements stay intact.

use secs_compute::{Context, MemoryError};
use secs_dsp::resample::{IrResamplePlan, MAX_RESAMPLE_RATE, MIN_RESAMPLE_RATE};
use secs_types::calibration::SampleRatePolicy;

use crate::{EngineError, MeasurementSet, Samples, grid::SR};

fn check(ctx: &Context) -> Result<(), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)
}

/// Convert non-48 kHz IRs only when explicitly selected.
///
/// Same-rate and reject paths keep the original sample Arcs and t0 bits. The
/// transient reservation covers all new sample buffers, the largest Vec-to-Arc
/// transfer and the largest kernel. It ends on return; the caller retains the
/// result under the same input-memory rules as other engine measurements.
pub fn normalize_measurements(
    irs: &MeasurementSet,
    policy: SampleRatePolicy,
    ctx: &Context,
) -> Result<MeasurementSet, EngineError> {
    check(ctx)?;
    if policy == SampleRatePolicy::Reject {
        return Ok(irs.clone());
    }
    let mut output_bytes = 0_u64;
    let mut largest_output = 0_u64;
    let mut largest_kernel = 0_u64;
    let mut count = 0_u64;
    for (position, roles) in &irs.positions {
        for (role, measurement) in roles {
            check(ctx)?;
            if measurement.sample_rate == SR {
                continue;
            }
            if !(MIN_RESAMPLE_RATE..=MAX_RESAMPLE_RATE).contains(&measurement.sample_rate) {
                return Err(EngineError::UnsupportedResamplingRate {
                    position: *position,
                    role: role.clone(),
                    actual_hz: measurement.sample_rate,
                });
            }
            if measurement.samples.is_empty() || !measurement.t0.is_finite() {
                return Err(EngineError::InvalidMeasurement {
                    position: *position,
                    role: role.clone(),
                });
            }
            for n in 0..measurement.samples.len() {
                if n % 4096 == 0 {
                    check(ctx)?;
                }
                if !measurement.samples.at(n).is_finite() {
                    return Err(EngineError::InvalidMeasurement {
                        position: *position,
                        role: role.clone(),
                    });
                }
            }
            let plan = IrResamplePlan::new(measurement.samples.len(), measurement.sample_rate, SR)?;
            let bytes = (plan.output_len() * size_of::<f64>()) as u64;
            output_bytes = output_bytes.checked_add(bytes).ok_or_else(|| {
                EngineError::Input("IR resampling memory estimate overflow".into())
            })?;
            largest_output = largest_output.max(bytes);
            largest_kernel = largest_kernel.max(plan.kernel_bytes() as u64);
            count += 1;
        }
    }
    if output_bytes == 0 {
        return Ok(irs.clone());
    }
    let bytes = output_bytes
        .checked_add(largest_output)
        .and_then(|n| n.checked_add(largest_kernel))
        .ok_or_else(|| EngineError::Input("IR resampling memory estimate overflow".into()))?;
    let _permit = ctx
        .budget()
        .acquire(bytes, ctx.cancel())
        .map_err(|error| match error {
            MemoryError::Cancelled => EngineError::Cancelled,
            other => EngineError::Input(other.to_string()),
        })?;
    let progress = ctx.stage("engine.resample", count);
    let mut normalized = irs.clone();
    for (_, roles) in &mut normalized.positions {
        for (_, measurement) in roles {
            check(ctx)?;
            if measurement.sample_rate == SR {
                continue;
            }
            let plan = IrResamplePlan::new(measurement.samples.len(), measurement.sample_rate, SR)?;
            let samples = plan.process(|n| measurement.samples.at(n), || check(ctx))?;
            measurement.t0 -= plan.leading_output_samples() as f64 / f64::from(SR);
            measurement.sample_rate = SR;
            measurement.samples = Samples::F64(samples.into());
            progress.advance(1);
        }
    }
    Ok(normalized)
}

//! Optional acceleration of the ideal-FIR shifted systems (ADR 0008).
//! The CPU owns lambda selection, support energy and all acoustic guards.

use std::sync::{Arc, Mutex};

use secs_compute::CancelToken;
use secs_types::compute::{
    ComputeBackend, ComputePreference, ComputeReason, ComputeReport, ComputeStage, ComputeStatus,
};

use super::{design::LambdaStep, solve::RidgeSweep};
use crate::{EngineError, decay};

/// A failed optional accelerator can be replaced by a fresh CPU search.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AccelerationError {
    #[error("cancelled")]
    Cancelled,
    #[error("{detail}")]
    Failed {
        reason: ComputeReason,
        detail: String,
    },
}

impl AccelerationError {
    pub fn failed(reason: ComputeReason, detail: impl Into<String>) -> Self {
        Self::Failed {
            reason,
            detail: detail.into(),
        }
    }
}

/// A prepared system stays on its creating worker; Send is deliberately absent.
pub trait ShiftedSystem {
    fn solve(&mut self, lambda: f64, cancel: &CancelToken) -> Result<Vec<f64>, AccelerationError>;
}

/// A factory can be shared by input workers. Its prepared objects cannot.
pub trait RidgeBackend: std::fmt::Debug + Send + Sync {
    fn factor<'a>(
        &'a self,
        ata: &'a [f64],
        atb: &'a [f64],
        cancel: &CancelToken,
    ) -> Result<Box<dyn ShiftedSystem + 'a>, AccelerationError>;

    /// Auto may keep small systems on the CPU; an explicit GPU overrides this.
    fn minimum_dimension(&self) -> usize {
        0
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccelerationSelection {
    pub status: ComputeStatus,
    pub backend: Option<Arc<dyn RidgeBackend>>,
}

/// The unchanged sequential Householder/refinement implementation.
#[derive(Debug, Default)]
pub struct CpuRidgeBackend;

struct CpuSystem<'a>(RidgeSweep<'a>);

impl RidgeBackend for CpuRidgeBackend {
    fn factor<'a>(
        &'a self,
        ata: &'a [f64],
        atb: &'a [f64],
        cancel: &CancelToken,
    ) -> Result<Box<dyn ShiftedSystem + 'a>, AccelerationError> {
        cancelled(cancel)?;
        if atb.is_empty()
            || atb.len().checked_mul(atb.len()) != Some(ata.len())
            || ata.iter().chain(atb).any(|v| !v.is_finite())
        {
            return Err(AccelerationError::failed(
                ComputeReason::DeviceFailure,
                "invalid ridge system",
            ));
        }
        let system = CpuSystem(RidgeSweep::new(ata, atb));
        cancelled(cancel)?;
        Ok(Box::new(system))
    }
}

impl ShiftedSystem for CpuSystem<'_> {
    fn solve(&mut self, lambda: f64, cancel: &CancelToken) -> Result<Vec<f64>, AccelerationError> {
        cancelled(cancel)?;
        let result = self
            .0
            .solve(lambda)
            .map_err(|e| AccelerationError::failed(ComputeReason::DeviceFailure, e.to_string()))?;
        cancelled(cancel)?;
        Ok(result)
    }
}

fn cancelled(cancel: &CancelToken) -> Result<(), AccelerationError> {
    cancel.check().map_err(|_| AccelerationError::Cancelled)
}

pub(crate) struct RunAcceleration<'a> {
    selection: &'a AccelerationSelection,
    report: Mutex<ComputeReport>,
}

impl<'a> RunAcceleration<'a> {
    pub(crate) fn new(selection: &'a AccelerationSelection) -> Self {
        Self {
            selection,
            report: Mutex::new(ComputeReport {
                requested: selection.status.preference,
                reason: Some(if selection.status.preference == ComputePreference::Cpu {
                    ComputeReason::UserCpu
                } else {
                    selection.status.reason
                }),
                ..ComputeReport::default()
            }),
        }
    }

    pub(crate) fn report(&self) -> ComputeReport {
        self.report
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn completed(&self, gpu: bool, fallback: Option<ComputeReason>, small: bool) {
        let mut report = self.report.lock().unwrap_or_else(|e| e.into_inner());
        if gpu {
            report.accelerated_systems += 1;
            report.backend = ComputeBackend::Cuda;
            report.stage = Some(ComputeStage::IdealFirSweep);
            report.device = self
                .selection
                .status
                .device
                .as_ref()
                .map(|d| d.name.clone());
            if report.fallbacks == 0 {
                report.reason = None;
            }
        } else {
            report.cpu_systems += 1;
            if small && report.accelerated_systems == 0 && report.fallbacks == 0 {
                report.reason = Some(ComputeReason::SmallProblem);
            }
        }
        if let Some(reason) = fallback {
            report.fallbacks += 1;
            report.stage = Some(ComputeStage::IdealFirSweep);
            report.device = self
                .selection
                .status
                .device
                .as_ref()
                .map(|d| d.name.clone());
            // A stable severity rule makes parallel worker reporting order irrelevant.
            if report
                .reason
                .is_none_or(|old| reason_priority(reason) > reason_priority(old))
            {
                report.reason = Some(reason);
            }
        }
    }
}

fn reason_priority(reason: ComputeReason) -> u8 {
    match reason {
        ComputeReason::IirReference => 13,
        ComputeReason::DeviceFailure => 12,
        ComputeReason::RuntimeUnavailable => 11,
        ComputeReason::InsufficientMemory => 10,
        ComputeReason::UnsupportedPlatform => 9,
        ComputeReason::NoDevice => 8,
        ComputeReason::Integrated => 7,
        ComputeReason::Legacy => 6,
        ComputeReason::LowPerformance => 5,
        ComputeReason::SmallProblem => 4,
        ComputeReason::UserCpu => 3,
        ComputeReason::UserGpu => 2,
        ComputeReason::Recommended => 1,
    }
}

pub(crate) struct RidgeProblem<'a> {
    pub ata: &'a [f64],
    pub atb: &'a [f64],
    pub lp: &'a [f64],
    pub taps: usize,
    pub normalization: f64,
    pub target: f64,
    pub direct_energy: f64,
}

pub(crate) struct SweepResult {
    pub u: Vec<f64>,
    pub ratio: f64,
    pub log_lambda: f64,
    pub trajectory: Vec<LambdaStep>,
}

fn search(
    problem: &RidgeProblem<'_>,
    system: &mut dyn ShiftedSystem,
    cancel: &CancelToken,
) -> Result<SweepResult, AccelerationError> {
    let mut evaluate = |log: f64| -> Result<(Vec<f64>, f64), AccelerationError> {
        cancelled(cancel)?;
        let u = system.solve(problem.normalization * 10f64.powf(log), cancel)?;
        if u.len() != problem.atb.len() || u.iter().any(|v| !v.is_finite()) {
            return Err(AccelerationError::failed(
                ComputeReason::DeviceFailure,
                "invalid accelerated solution",
            ));
        }
        let mut support_energy = 0.0;
        for k in 0..problem.atb.len() / problem.taps {
            support_energy += decay::energy(&super::design::convolve(
                problem.lp,
                &u[k * problem.taps..(k + 1) * problem.taps],
            ));
        }
        let ratio = support_energy / problem.direct_energy;
        if !ratio.is_finite() {
            return Err(AccelerationError::failed(
                ComputeReason::DeviceFailure,
                "nonfinite support energy",
            ));
        }
        Ok((u, ratio))
    };
    let (mut lo, mut hi) = (-8.0, 4.0);
    let (mut u, mut ratio) = evaluate(hi)?;
    let mut trajectory = vec![LambdaStep {
        mid: hi,
        ratio,
        accepted: ratio <= problem.target,
    }];
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        let (trial, r) = evaluate(mid)?;
        trajectory.push(LambdaStep {
            mid,
            ratio: r,
            accepted: r <= problem.target,
        });
        if r > problem.target {
            lo = mid;
        } else {
            hi = mid;
            u = trial;
            ratio = r;
        }
    }
    cancelled(cancel)?;
    Ok(SweepResult {
        u,
        ratio,
        log_lambda: hi,
        trajectory,
    })
}

pub(crate) fn run_sweep(
    problem: RidgeProblem<'_>,
    cancel: &CancelToken,
    acceleration: Option<&RunAcceleration<'_>>,
) -> Result<SweepResult, EngineError> {
    let mut fallback = None;
    let mut small = false;
    if let Some(run) = acceleration
        && run.selection.status.preference != ComputePreference::Cpu
        && run.selection.status.selected == ComputeBackend::Cuda
        && let Some(backend) = &run.selection.backend
    {
        small = run.selection.status.preference == ComputePreference::Auto
            && problem.atb.len() < backend.minimum_dimension();
        if !small {
            // The prepared GPU object/lease is dropped before a CPU restart.
            let attempt = backend
                .factor(problem.ata, problem.atb, cancel)
                .and_then(|mut system| search(&problem, &mut *system, cancel));
            match attempt {
                Ok(result) => {
                    run.completed(true, None, false);
                    return Ok(result);
                }
                Err(AccelerationError::Cancelled) => return Err(EngineError::Cancelled),
                Err(AccelerationError::Failed { reason, .. }) => fallback = Some(reason),
            }
        }
    }
    let cpu = CpuRidgeBackend;
    let result = cpu
        .factor(problem.ata, problem.atb, cancel)
        .and_then(|mut system| search(&problem, &mut *system, cancel))
        .map_err(|error| match error {
            AccelerationError::Cancelled => EngineError::Cancelled,
            AccelerationError::Failed { detail, .. } => EngineError::Input(detail),
        })?;
    if let Some(run) = acceleration {
        run.completed(false, fallback, small);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_types::compute::GpuDeviceInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Fake {
        cpu: CpuRidgeBackend,
        calls: Arc<AtomicUsize>,
        failure: usize,
        cancel_at_failure: bool,
        minimum: usize,
    }
    struct FakeSystem<'a> {
        inner: Box<dyn ShiftedSystem + 'a>,
        fake: &'a Fake,
    }
    impl RidgeBackend for Fake {
        fn factor<'a>(
            &'a self,
            ata: &'a [f64],
            atb: &'a [f64],
            cancel: &CancelToken,
        ) -> Result<Box<dyn ShiftedSystem + 'a>, AccelerationError> {
            Ok(Box::new(FakeSystem {
                inner: self.cpu.factor(ata, atb, cancel)?,
                fake: self,
            }))
        }
        fn minimum_dimension(&self) -> usize {
            self.minimum
        }
    }
    impl ShiftedSystem for FakeSystem<'_> {
        fn solve(
            &mut self,
            lambda: f64,
            cancel: &CancelToken,
        ) -> Result<Vec<f64>, AccelerationError> {
            let count = self.fake.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if count == self.fake.failure {
                if self.fake.cancel_at_failure {
                    cancel.cancel();
                    return Err(AccelerationError::Cancelled);
                }
                return Err(AccelerationError::failed(
                    ComputeReason::DeviceFailure,
                    "injected device loss",
                ));
            }
            self.inner.solve(lambda, cancel)
        }
    }
    fn problem() -> RidgeProblem<'static> {
        RidgeProblem {
            ata: &[4.0, 1.0, 1.0, 3.0],
            atb: &[1.0, 2.0],
            lp: &[1.0],
            taps: 1,
            normalization: 3.5,
            target: 0.1,
            direct_energy: 1.0,
        }
    }
    fn selection(
        preference: ComputePreference,
        calls: Arc<AtomicUsize>,
        failure: usize,
        cancel_at_failure: bool,
        minimum: usize,
    ) -> AccelerationSelection {
        AccelerationSelection {
            status: ComputeStatus {
                preference,
                selected: ComputeBackend::Cuda,
                gpu_available: true,
                reason: ComputeReason::Recommended,
                device: Some(GpuDeviceInfo {
                    id: 0,
                    name: "Fake CUDA".into(),
                    compute_major: 7,
                    compute_minor: 5,
                    multiprocessors: 48,
                    memory_bytes: 8 << 30,
                    integrated: false,
                    supported: true,
                    recommended: true,
                    reason: ComputeReason::Recommended,
                }),
                devices: Vec::new(),
            },
            backend: Some(Arc::new(Fake {
                cpu: CpuRidgeBackend,
                calls,
                failure,
                cancel_at_failure,
                minimum,
            })),
        }
    }
    #[test]
    fn middle_failure_restarts_all_nineteen_cpu_steps_and_retains_provenance() {
        let cancel = CancelToken::new();
        let expected = run_sweep(problem(), &cancel, None).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let selection = selection(ComputePreference::Gpu, calls.clone(), 3, false, 0);
        let run = RunAcceleration::new(&selection);
        let actual = run_sweep(problem(), &cancel, Some(&run)).unwrap();
        assert_eq!(actual.trajectory.len(), 19);
        assert_eq!(actual.trajectory, expected.trajectory);
        assert_eq!(actual.u, expected.u);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let report = run.report();
        assert_eq!(report.cpu_systems, 1);
        assert_eq!(report.accelerated_systems, 0);
        assert_eq!(report.fallbacks, 1);
        assert_eq!(report.backend, ComputeBackend::Cpu);
        assert_eq!(report.stage, Some(ComputeStage::IdealFirSweep));
        assert_eq!(report.device.as_deref(), Some("Fake CUDA"));
    }
    #[test]
    fn cancellation_during_shift_never_falls_back() {
        let cancel = CancelToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let selection = selection(ComputePreference::Gpu, calls.clone(), 3, true, 0);
        let run = RunAcceleration::new(&selection);
        assert!(matches!(
            run_sweep(problem(), &cancel, Some(&run)),
            Err(EngineError::Cancelled)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(run.report().cpu_systems, 0);
        assert_eq!(run.report().fallbacks, 0);
    }
    #[test]
    fn auto_size_floor_and_explicit_gpu_have_distinct_actual_reports() {
        for (preference, expected_calls) in
            [(ComputePreference::Auto, 0), (ComputePreference::Gpu, 19)]
        {
            let calls = Arc::new(AtomicUsize::new(0));
            let selection = selection(preference, calls.clone(), 0, false, 10);
            let run = RunAcceleration::new(&selection);
            run_sweep(problem(), &CancelToken::new(), Some(&run)).unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            let report = run.report();
            assert_eq!(report.accelerated_systems, u32::from(expected_calls > 0));
            assert_eq!(
                report.reason,
                if expected_calls == 0 {
                    Some(ComputeReason::SmallProblem)
                } else {
                    None
                }
            );
        }
    }
    #[test]
    fn an_inconsistent_provider_cannot_override_forced_cpu_or_cpu_selection() {
        for forced in [true, false] {
            let calls = Arc::new(AtomicUsize::new(0));
            let mut selection = selection(
                if forced {
                    ComputePreference::Cpu
                } else {
                    ComputePreference::Auto
                },
                calls.clone(),
                0,
                false,
                0,
            );
            if !forced {
                selection.status.selected = ComputeBackend::Cpu;
            }
            let run = RunAcceleration::new(&selection);
            run_sweep(problem(), &CancelToken::new(), Some(&run)).unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(run.report().cpu_systems, 1);
            if forced {
                assert_eq!(run.report().reason, Some(ComputeReason::UserCpu));
            }
        }
    }
    #[test]
    fn parallel_failure_reason_selection_is_order_independent() {
        let selection = selection(
            ComputePreference::Auto,
            Arc::new(AtomicUsize::new(0)),
            0,
            false,
            0,
        );
        let first = RunAcceleration::new(&selection);
        let second = RunAcceleration::new(&selection);
        first.completed(false, Some(ComputeReason::InsufficientMemory), false);
        first.completed(false, Some(ComputeReason::RuntimeUnavailable), false);
        second.completed(false, Some(ComputeReason::RuntimeUnavailable), false);
        second.completed(false, Some(ComputeReason::InsufficientMemory), false);
        assert_eq!(first.report(), second.report());
        assert_eq!(
            first.report().reason,
            Some(ComputeReason::RuntimeUnavailable)
        );
    }
}

/// Recorded-input replay surface for hardware audits; not in production builds.
#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct RidgeReplayInput<'a> {
    pub ata: &'a [f64],
    pub atb: &'a [f64],
    pub lp: &'a [f64],
    pub direct: &'a [f64],
    pub taps: usize,
    pub level_db: f64,
}

#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct RidgeReplayOutput {
    pub u: Vec<f64>,
    pub ratio: f64,
    pub log_lambda: f64,
    pub trajectory: Vec<(f64, f64, bool)>,
}

/// Runs precisely the production lambda controller with the selected solver.
#[cfg(feature = "test-support")]
pub fn replay_ridge(
    input: &RidgeReplayInput<'_>,
    backend: &dyn RidgeBackend,
    cancel: &CancelToken,
) -> Result<RidgeReplayOutput, AccelerationError> {
    let n = input.atb.len();
    if input.taps == 0
        || !n.is_multiple_of(input.taps)
        || n == 0
        || n.checked_mul(n) != Some(input.ata.len())
        || !input.level_db.is_finite()
    {
        return Err(AccelerationError::failed(
            ComputeReason::DeviceFailure,
            "invalid ridge replay inputs",
        ));
    }
    let diagonal: Vec<_> = (0..n).map(|k| input.ata[k * n + k]).collect();
    let problem = RidgeProblem {
        ata: input.ata,
        atb: input.atb,
        lp: input.lp,
        taps: input.taps,
        normalization: secs_dsp::pairwise_sum(&diagonal) / n as f64,
        target: 10f64.powf(input.level_db / 10.0),
        direct_energy: decay::energy(input.direct).max(1e-30),
    };
    let mut system = backend.factor(input.ata, input.atb, cancel)?;
    let result = search(&problem, &mut *system, cancel)?;
    Ok(RidgeReplayOutput {
        u: result.u,
        ratio: result.ratio,
        log_lambda: result.log_lambda,
        trajectory: result
            .trajectory
            .into_iter()
            .map(|v| (v.mid, v.ratio, v.accepted))
            .collect(),
    })
}

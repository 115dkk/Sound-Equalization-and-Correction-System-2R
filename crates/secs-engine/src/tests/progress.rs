//! Progress counts completed work; it must not change the design or replay it.
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use secs_compute::{CancelToken, Context, MemoryBudget, Progress, ProgressSink};
use secs_types::calibration::{CalibrationSettings, RcHiBasis};

#[derive(Debug, Default)]
struct Sink(Mutex<Vec<Progress>>);
impl ProgressSink for Sink {
    fn report(&self, progress: &Progress) {
        self.0.lock().unwrap().push(progress.clone());
    }
}

/// The stages whose amount of work cannot be known before they run: they
/// are announced once with no total, so the screen names them without a bar.
const UNCOUNTED: [&str; 2] = ["engine.ba", "engine.ba_probe"];

pub(super) fn assert_contract(reports: &[Progress]) -> Vec<&'static str> {
    let mut stages = Vec::new();
    let mut start = 0;
    while start < reports.len() {
        let first = &reports[start];
        assert!(
            !stages.contains(&first.stage),
            "stage restarted: {}",
            first.stage
        );
        stages.push(first.stage);
        assert_eq!(first.done, 0, "{}", first.stage);
        if UNCOUNTED.contains(&first.stage) {
            assert_eq!(first.total, 0, "{}", first.stage);
            assert!(
                reports
                    .get(start + 1)
                    .is_none_or(|next| next.stage != first.stage),
                "an uncounted stage is announced once: {}",
                first.stage
            );
            start += 1;
            continue;
        }
        assert!(first.total > 0, "{}", first.stage);
        let end = reports[start..]
            .iter()
            .position(|p| p.stage != first.stage)
            .map_or(reports.len(), |offset| start + offset);
        let group = &reports[start..end];
        assert!(
            group
                .iter()
                .all(|p| p.total == first.total && p.done <= p.total)
        );
        assert!(group.windows(2).all(|pair| pair[0].done <= pair[1].done));
        assert_eq!(group.last().unwrap().done, first.total, "{}", first.stage);
        start = end;
    }
    stages
}

#[test]
fn calibration_progress_distinguishes_probe_and_final_and_counts_rc_tasks() {
    let case = super::cases::load(&crate::tests::Oracle, "room_dba");
    let mut settings: CalibrationSettings = serde_json::from_value(case.settings.clone()).unwrap();
    settings.rc_hi = None;
    settings.rc_hi_basis = RcHiBasis::Agreement;
    let view = crate::ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
        inverted: &[],
    };
    let sink = Arc::new(Sink::default());
    let ctx = Context::new(
        1,
        MemoryBudget::new(u64::MAX),
        sink.clone(),
        CancelToken::new(),
    );
    let calibration = crate::calibrate(
        &crate::CalibrationRequest {
            view: &view,
            irs: &case.measurements,
            settings: &settings,
            subs_only: false,
        },
        &ctx,
    )
    .unwrap();
    let silent = crate::late_energy(&calibration.design).unwrap();
    assert_eq!(calibration.design.late_energy_until(&ctx).unwrap(), silent);
    let reports = sink.0.lock().unwrap();
    let stages = assert_contract(&reports);
    assert_eq!(
        stages,
        [
            "engine.ba_probe",
            "engine.top_probe",
            "engine.design_probe",
            "engine.ba",
            "engine.top",
            "engine.design",
            "engine.late_energy"
        ]
    );
    assert!(
        reports
            .iter()
            .all(|p| UNCOUNTED.contains(&p.stage) || p.total >= 3)
    );
    for stage in ["engine.design_probe", "engine.design"] {
        let counts: BTreeSet<_> = reports
            .iter()
            .filter(|p| p.stage == stage)
            .map(|p| p.done)
            .collect();
        assert!(
            counts.len() > case.config.mains.len(),
            "{stage}: {counts:?}"
        );
    }
}

#[test]
fn empty_iir_supports_do_not_start_a_stage() {
    let case = super::cases::load(&crate::tests::Oracle, "room_dba");
    let design = super::design::replay("room_dba");
    let sink = Arc::new(Sink::default());
    let ctx = Context::new(
        1,
        MemoryBudget::new(u64::MAX),
        sink.clone(),
        CancelToken::new(),
    );
    crate::design_mimo_iir(
        &design,
        &case.measurements,
        &case.config,
        &crate::MimoDesign::default(),
        &crate::MimoRequest::default(),
        &case.tuning,
        &ctx,
    )
    .unwrap();
    assert!(sink.0.lock().unwrap().is_empty());
}

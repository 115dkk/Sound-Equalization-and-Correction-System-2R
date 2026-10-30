#[cfg(test)]
#[path = "curves_tests.rs"]
mod tests;

use std::sync::Arc;

use secs_engine::{
    Design, display,
    preview::{XoPreview, sub_combine},
};
use secs_project::{Project, calibration_from_project, store_calibration};
use secs_types::{
    calibration::CalibrationSettings,
    ipc::*,
    role::{RoleKind, role_kind},
};

use crate::{Service, lock};

pub(crate) fn dispatch(service: &Service, request: Request) -> Result<Response, IpcError> {
    match request {
        Request::MeasurementCurves(params) => service
            .measurement_curves(params)
            .map(Response::MeasurementCurves),
        Request::ResultCurves(params) => service.result_curves(params).map(Response::ResultCurves),
        Request::ResultLines => service.result_lines().map(Response::ResultLines),
        Request::SubTakeValues => service.sub_take_values().map(Response::SubTakeValues),
        _ => unreachable!("curve dispatch only receives curve requests"),
    }
}

fn axes(frequency_hz: [f64; 2], value: [f64; 2], unit: &str) -> CurveAxes {
    CurveAxes {
        frequency_hz,
        value,
        unit: unit.into(),
    }
}
fn series(kind: &str, role: Option<&str>, values: &[f64], offset: f64) -> ResultCurve {
    ResultCurve {
        kind: kind.into(),
        role: role.map(str::to_owned),
        values: values.iter().map(|v| v - offset).collect(),
    }
}
fn band_values(grid: &[f64], values: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    grid.iter()
        .zip(values)
        .filter(|(f, _)| (lo..=hi).contains(*f))
        .map(|(_, v)| *v)
        .collect()
}
fn offset(design: &Design) -> f64 {
    let target = design.target.sample(&display::spl_grid());
    display::percentile(
        &design
            .target_display
            .iter()
            .zip(target)
            .map(|(v, t)| v - t)
            .collect::<Vec<_>>(),
        50.0,
    )
}
impl Service {
    fn measurement_curves(
        &self,
        params: MeasurementCurvesParams,
    ) -> Result<MeasurementCurvesResult, IpcError> {
        let mut inputs = Vec::new();
        {
            let mut session = lock(&self.session);
            let open = session.require_mut().map_err(IpcError::from)?;
            for record in open.store.records() {
                if params
                    .positions
                    .as_ref()
                    .is_some_and(|p| !p.contains(&record.pos))
                {
                    continue;
                }
                for role in record.ir.keys() {
                    if params.roles.as_ref().is_some_and(|r| !r.contains(role)) {
                        continue;
                    }
                    let (samples, rate) =
                        crate::calibration::measured_ir(&mut open.store, role, record.pos)?;
                    inputs.push((role.clone(), record.pos, samples, rate));
                }
            }
        }
        let context = secs_compute::Context::new(
            std::thread::available_parallelism().map_or(1, |v| v.get()),
            secs_compute::MemoryBudget::new(self.deps.memory.available_bytes() / 2),
            Arc::new(secs_compute::NoProgress),
            secs_compute::CancelToken::new(),
        );
        let curves = context
            .map(inputs, |(role, position, samples, rate)| {
                let kind = match role_kind(&role) {
                    RoleKind::Main => "main",
                    RoleKind::Sub => "sub",
                    RoleKind::Top | RoleKind::Height => "top",
                };
                MeasurementCurve {
                    kind: kind.into(),
                    role,
                    position,
                    values: display::magnitude_db(
                        &samples.iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
                        rate as f64,
                    ),
                }
            })
            .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
        Ok(MeasurementCurvesResult {
            grid: display::measurement_grid(),
            curves,
            axes: axes([5.0, 22000.0], [-35.0, 20.0], "db"),
        })
    }

    fn result_curves(&self, params: ResultCurvesParams) -> Result<ResultCurvesResult, IpcError> {
        if params.xo_hz.is_some_and(|hz| !hz.is_finite() || hz <= 0.0) {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "XO frequency must be finite and positive",
            ));
        }
        let compute_key = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let mut session = lock(&self.session);
        let open = session.require_mut().map_err(IpcError::from)?;
        // The BA tab shows the last subs-only run while there is one, as
        // upstream's `_draw_subs` does (`source = sub_run or run`).
        let subs_only = params.tab == ResultTab::Ba && open.results.subs().is_some();
        let run = if subs_only {
            open.results.subs()
        } else {
            open.results.live_calibration()
        };
        let project = open.store.project();
        let settings = calibration_from_project(project);
        let mut result = ResultCurvesResult {
            tab: params.tab,
            grid: Vec::new(),
            curves: Vec::new(),
            stale: run.is_some_and(|r| crate::results::calibration_stale(r, project)),
            subs_only,
            axes: match params.tab {
                ResultTab::Spl => axes([5.0, 20000.0], [-30.0, 15.0], "db"),
                ResultTab::Gd => axes([5.0, 5000.0], [-10.0, 100.0], "ms"),
                ResultTab::Xo => axes([5.0, 500.0], [-30.0, 12.0], "db"),
                ResultTab::Ba => axes([5.0, 300.0], [-40.0, 6.0], "db"),
            },
            rc_band_hz: matches!(params.tab, ResultTab::Spl | ResultTab::Gd).then(|| {
                run.map_or((settings.rc_lo, settings.rc_hi), |r| {
                    (r.settings.rc_lo, Some(r.design().rc_hi))
                })
            }),
            xo_hz: None,
        };
        let Some(run) = run else {
            if params.tab == ResultTab::Spl {
                let roles = open
                    .store
                    .records()
                    .into_iter()
                    .find(|r| r.pos == 1)
                    .map(|r| r.ir)
                    .unwrap_or_default();
                let role = params.channel.or_else(|| {
                    project
                        .config
                        .speakers
                        .iter()
                        .find(|s| roles.contains_key(&s.role))
                        .map(|s| s.role.clone())
                });
                if let Some(role) = role.filter(|r| roles.contains_key(r)) {
                    let target = project.target.clone();
                    let (samples, rate) =
                        crate::calibration::measured_ir(&mut open.store, &role, 1)?;
                    let curve = display::measurement_spl(
                        &samples.iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
                        rate as f64,
                    );
                    result.grid = display::spl_grid();
                    let target = target.sample(&result.grid);
                    let level = display::percentile(
                        &band_values(&result.grid, &curve, 200.0, 2000.0),
                        50.0,
                    ) - display::percentile(
                        &band_values(&result.grid, &target, 200.0, 2000.0),
                        50.0,
                    );
                    let mid = display::percentile(
                        &band_values(&result.grid, &curve, 20.0, 20000.0),
                        50.0,
                    ) - level;
                    result.axes.value = [mid - 30.0, mid + 15.0];
                    result.curves = vec![
                        series("before", None, &curve, level),
                        series("target", None, &target, 0.0),
                    ];
                }
            }
            return Ok(result);
        };
        let design = run.design();
        let offset = offset(design);
        let role = params
            .channel
            .as_deref()
            .or_else(|| design.scores.first().map(|s| s.role.as_str()));
        match params.tab {
            ResultTab::Spl | ResultTab::Gd => {
                let Some(score) = design.scores.iter().find(|s| Some(s.role.as_str()) == role)
                else {
                    return Ok(result);
                };
                if params.tab == ResultTab::Spl {
                    result.grid = display::spl_grid();
                    result.curves = vec![
                        series("before", None, &score.spl_before, offset),
                        series("after", None, &score.spl_after, offset),
                        series("target", None, &project.target.sample(&result.grid), 0.0),
                    ];
                    if let Some(crate::results::MimoView::Live(mimo)) =
                        open.results.mimo(project, &compute_key).shown()
                        && let Some((_, values)) =
                            mimo.run.spl_mimo.iter().find(|(r, _)| r == &score.role)
                    {
                        result.curves.push(series("mimo", None, values, offset));
                    }
                    let mid = display::percentile(
                        &band_values(&result.grid, &design.target_display, 20.0, 20000.0),
                        50.0,
                    ) - offset;
                    result.axes.value = [mid - 30.0, mid + 15.0];
                } else {
                    result.grid = display::gd_grid();
                    result.curves = vec![
                        series("before", None, &score.gd_before, 0.0),
                        series("after", None, &score.gd_after, 0.0),
                    ];
                    let both: Vec<_> = score
                        .gd_before
                        .iter()
                        .chain(&score.gd_after)
                        .copied()
                        .filter(|v| v.is_finite())
                        .collect();
                    if !both.is_empty() {
                        result.axes.value = [
                            (display::percentile(&both, 1.0) - 5.0).clamp(-50.0, -5.0),
                            (display::percentile(&both, 99.0) + 10.0).clamp(30.0, 400.0),
                        ];
                    }
                }
            }
            ResultTab::Xo => {
                let group = params.group.as_deref().or_else(|| {
                    design
                        .groups
                        .iter()
                        .find(|(g, rs)| {
                            g != "sub" && role.is_some_and(|r| rs.iter().any(|s| s == r))
                        })
                        .map(|(g, _)| g.as_str())
                });
                if let Some(group) =
                    group.filter(|g| design.groups.iter().any(|(name, _)| name == g))
                    && design.subs.is_some()
                {
                    let hz = params.xo_hz.unwrap_or_else(|| {
                        settings
                            .xo
                            .iter()
                            .chain(&project.xo)
                            .find(|(g, _)| g == group)
                            .map_or(80.0, |(_, hz)| *hz)
                    });
                    let view = XoPreview::new(design).curves(
                        design,
                        group,
                        hz,
                        params.corrected.unwrap_or(true),
                        role,
                    );
                    let mid = display::percentile(&view.target, 50.0) - offset;
                    result.axes.value = [mid - 30.0, mid + 12.0];
                    result.grid = view.grid;
                    result.xo_hz = Some(hz);
                    result.curves = vec![
                        series("target", None, &view.target, offset),
                        series("main", None, &view.main, offset),
                        series("sub", None, &view.sub, offset),
                        series("sum", None, &view.sum, offset),
                    ];
                }
            }
            ResultTab::Ba => {
                if let Some(view) = sub_combine(design) {
                    result.grid = view.grid;
                    result.curves.push(series(
                        "target",
                        None,
                        &project.target.sample(&result.grid),
                        0.0,
                    ));
                    for (role, before) in &view.before {
                        result
                            .curves
                            .push(series("before", Some(role), before, offset));
                        if let Some((_, after)) = view.after.iter().find(|(r, _)| r == role) {
                            result
                                .curves
                                .push(series("after", Some(role), after, offset));
                        }
                    }
                    result
                        .curves
                        .push(series("sum_before", None, &view.sum_before, offset));
                    result
                        .curves
                        .push(series("sum_after", None, &view.sum_after, offset));
                    let top = band_values(&result.grid, &view.sum_before, view.band.0, view.band.1)
                        .into_iter()
                        .chain(band_values(
                            &result.grid,
                            &view.sum_after,
                            view.band.0,
                            view.band.1,
                        ))
                        .reduce(f64::max)
                        .unwrap_or(0.0)
                        - offset;
                    result.axes.value = [top - 40.0, top + 6.0];
                }
            }
        }
        Ok(result)
    }

    fn result_lines(&self) -> Result<ResultLinesResult, IpcError> {
        let summary = self.results()?;
        let compute_key = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        let project = open.store.project();
        let live_mimo = match open.results.mimo(project, &compute_key).shown() {
            Some(crate::results::MimoView::Live(result)) => Some(result),
            _ => None,
        };
        let (stale, channels, total_delay_ms, rc) = open.results.live_calibration().map_or_else(
            || (false, Vec::new(), None, Vec::new()),
            |run| {
                let design = run.design();
                let mimo = live_mimo;
                let channels = design
                    .scores
                    .iter()
                    .filter_map(|score| {
                        let main = design.chain.mains.iter().find(|m| m.role == score.role)?;
                        Some(ChannelResultLine {
                            role: score.role.clone(),
                            within_1db_pct: [score.before.in1_20, score.after.in1_20],
                            gd_spread_ms: [score.before.gd_20, score.after.gd_20],
                            peq_count: main.input_peq.len(),
                            fir_taps: (main.fir.len() > 1).then_some(main.fir.len()),
                            xo_hz: main.xo_hz,
                            mimo_support_count: mimo.map(|m| {
                                m.run
                                    .paths
                                    .paths
                                    .iter()
                                    .filter(|p| p.input == score.role)
                                    .count()
                            }),
                        })
                    })
                    .collect();
                // The design's own rc_limit note carries the basis and its
                // evidence (ADR 0021); a design without one gets today's line.
                let rc_note = design
                    .notes
                    .iter()
                    .find(|note| matches!(note, secs_engine::Note::RcLimit { .. }));
                let mut rc = vec![match rc_note {
                    Some(note) => crate::calibration::scalar_notice(note),
                    None => {
                        let mut line = notice(
                            "rc_limit",
                            [
                                ("hz", design.rc_hi.to_string()),
                                ("auto", design.rc_auto.to_string()),
                            ],
                        );
                        if let Some(hz) = design.schroeder_hz {
                            line.params.insert("schroeder_hz".into(), hz.to_string());
                        }
                        line
                    }
                }];
                let band_hz = match rc_note {
                    Some(secs_engine::Note::RcLimit { band_hz, .. }) => *band_hz,
                    _ => None,
                };
                // The room volume decides only the two Schroeder bases.
                if design.rc_auto
                    && run.settings.rc_hi_basis != secs_types::calibration::RcHiBasis::Agreement
                {
                    rc.push(match run.settings.room_volume_m3.filter(|v| *v != 0.0) {
                        Some(volume) => {
                            notice("rc_room_volume", [("volume_m3", volume.to_string())])
                        }
                        None => notice("rc_no_room_volume", []),
                    });
                    let factor = f64::from(run.settings.rc_hi_transition);
                    let value = match (design.schroeder_hz, band_hz) {
                        (Some(strict), Some(band)) => Some(strict.max(band)),
                        (strict, band) => strict.or(band),
                    };
                    if run.settings.room_volume_m3.is_some_and(|v| v != 0.0)
                        && value.is_some_and(|v| v * factor > design.rc_hi + 0.5)
                    {
                        rc.push(notice("rc_auto_cap", [("hz", design.rc_hi.to_string())]));
                    }
                }
                (
                    crate::results::calibration_stale(run, project),
                    channels,
                    Some(design.chain.latency_ms),
                    rc,
                )
            },
        );
        // BA lines come from the last subs-only run while there is one, like
        // the BA curves (`source = sub_run or run`).
        let ba = open
            .results
            .subs()
            .or(open.results.live_calibration())
            .and_then(|ba_run| {
                let ba_design = ba_run.design();
                let ba_stale = crate::results::calibration_stale(ba_run, project);
                sub_combine(ba_design).map(|view| {
                    let warnings = secs_export::sub_filter_warnings(&ba_run.settings, &view.subs)
                        .into_iter()
                        .chain(secs_export::sub_gain_warnings(ba_design))
                        .map(warning)
                        .collect();
                    let mut layouts: Vec<_> = view
                        .layout_info
                        .iter()
                        .map(|(bundle, info)| SubLayoutLine {
                            bundle: bundle.clone(),
                            kind: format!("{:?}", info.kind).to_lowercase(),
                            members: info.members.clone(),
                            front: info.front.clone(),
                            rear: info.rear.clone(),
                            rear_ms: info.rear_ms,
                            auto_ms: info.auto_ms,
                            trim_ms: info.trim_ms,
                            peq: info.peq.clone(),
                        })
                        .collect();
                    layouts.sort_by(|a, b| a.bundle.cmp(&b.bundle));
                    SubResultLines {
                        stale: ba_stale,
                        subs_only: open.results.subs().is_some(),
                        band_hz: [view.band.0, view.band.1],
                        flatness_db: [view.flat.0, view.flat.1],
                        seat_spread_db: view.seat.map(|(a, b)| [a, b]),
                        positions: view.positions,
                        rows: ba_design
                            .chain
                            .subs
                            .iter()
                            .map(|s| SubResultRow {
                                role: s.role.clone(),
                                mode: s.mode.clone(),
                                gain_db: s.gain_db,
                                delay_ms: s.delay_ms,
                                polarity: s.polarity,
                                allpass: s.allpass.clone(),
                                high_pass: (s.hp_hz, s.hp_type.name().into()),
                                low_pass: (s.lp_hz, s.lp_type.name().into()),
                                peq: s.peq.clone(),
                            })
                            .collect(),
                        layouts,
                        warnings,
                    }
                })
            });
        Ok(ResultLinesResult {
            stale,
            channels,
            total_delay_ms,
            notes: summary.calibration.map_or_else(Vec::new, |s| s.notes),
            rc,
            ba,
        })
    }

    pub(crate) fn sub_warnings(&self) -> Result<Vec<Notice>, IpcError> {
        let session = lock(&self.session);
        let project = session.require()?.store.project();
        let settings = calibration_from_project(project);
        let roles = project
            .config
            .groups()
            .into_iter()
            .find(|(group, _)| *group == "sub")
            .map(|(_, speakers)| {
                speakers
                    .into_iter()
                    .map(|s| s.role.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Upstream sub_gain_warnings needs a completed run, not manual settings.
        Ok(secs_export::sub_filter_warnings(&settings, &roles)
            .into_iter()
            .map(warning)
            .collect())
    }

    fn sub_take_values(&self) -> Result<CalibrationSettings, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut().map_err(IpcError::from)?;
        let run = open.results.live_calibration().ok_or_else(|| {
            IpcError::new(
                ErrorCode::EngineInput,
                "calibration must run before taking sub values",
            )
        })?;
        let mut settings = calibration_from_project(open.store.project());
        take_values(&mut settings, run.design());
        store_calibration(open.store.project_mut(), &settings);
        open.dirty = true;
        Ok(settings)
    }
}

fn take_values(settings: &mut CalibrationSettings, design: &Design) {
    for sub in &design.chain.subs {
        if !settings.subs_manual.iter().any(|(r, _)| r == &sub.role) {
            settings
                .subs_manual
                .push((sub.role.clone(), Default::default()));
        }
        let entry = &mut settings
            .subs_manual
            .iter_mut()
            .find(|(r, _)| r == &sub.role)
            .unwrap()
            .1;
        entry.gain_db = sub.gain_db;
        entry.delay_ms = sub.delay_ms;
        entry.polarity = sub.polarity;
        entry.allpass = sub.allpass.clone();
    }
    settings.sub_main_delay_ms = design.chain.main_extra_ms;
    // set_sub calls sub_entry upstream: reuse the project's normalization
    // rather than duplicating the manual-setting bounds here.
    let mut project = Project::default();
    store_calibration(&mut project, settings);
    let normalized = calibration_from_project(&project);
    settings.subs_manual = normalized.subs_manual;
}

fn notice<const N: usize>(code: &str, params: [(&str, String); N]) -> Notice {
    Notice {
        code: code.into(),
        params: params.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}
fn warning(warning: secs_export::SubWarning) -> Notice {
    match warning {
        secs_export::SubWarning::NarrowBand { role, hp_hz, lp_hz } => notice(
            "sub_narrow_band",
            [
                ("role", role),
                ("hp_hz", hp_hz.to_string()),
                ("lp_hz", lp_hz.to_string()),
            ],
        ),
        secs_export::SubWarning::HighGain { role, gain_db } => notice(
            "sub_high_gain",
            [("role", role), ("gain_db", gain_db.to_string())],
        ),
    }
}

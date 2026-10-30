//! Resolve project calibration settings into the engine constructor surface.

use std::collections::BTreeSet;

use secs_types::{
    calibration::{CalibrationSettings, SubEntry, mode_params},
    chain::BandType,
    target::TargetCurve,
};

use crate::{
    EngineError,
    input::{BundleLayout, DbaRow, EngineConfig, LayoutKind, MeasurementSet, SubManual, SubMode},
};

#[derive(Debug)]
pub struct ProjectView<'a> {
    pub groups: &'a [(String, Vec<String>)],
    pub project_xo: &'a [(String, f64)],
    pub target: &'a TargetCurve,
}

#[derive(Debug)]
pub struct EngineInputs {
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    pub notes: Vec<String>,
}

#[derive(Debug)]
pub struct CalibrationRequest<'a> {
    pub view: &'a ProjectView<'a>,
    pub irs: &'a MeasurementSet,
    pub settings: &'a CalibrationSettings,
    pub subs_only: bool,
}

#[derive(Debug)]
pub struct Calibration {
    pub design: crate::Design,
    /// The normalized measurements and config the design was made from.
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    /// IRs resampled to 48 kHz by the sample-rate policy.
    pub resampled: usize,
}

/// Resolve, normalize and design with the context's cancellable production fitters.
pub fn calibrate(
    req: &CalibrationRequest<'_>,
    ctx: &secs_compute::Context,
) -> Result<Calibration, EngineError> {
    let derivatives = if req.settings.peq_analytic_derivatives {
        crate::PeqDerivatives::Analytic
    } else {
        crate::PeqDerivatives::TwoPoint
    };
    calibrate_with(
        req,
        ctx,
        &*crate::production_fitters_with(ctx.cancel(), derivatives),
    )
}

/// The calibration entry point with caller-supplied fitters (oracle replay).
pub fn calibrate_with(
    req: &CalibrationRequest<'_>,
    ctx: &secs_compute::Context,
    fitters: &crate::FitterFactory<'_>,
) -> Result<Calibration, EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
    let inputs = engine_inputs(req.view, req.irs, req.settings)?;
    let (inputs, resampled) = prepare_inputs(inputs, req.settings.sample_rate_policy, ctx)?;
    let design = crate::design(
        &crate::DesignRequest {
            measurements: &inputs.measurements,
            config: &inputs.config,
            tuning: &crate::Tuning::default(),
            subs_only: req.subs_only,
            adapter_notes: &inputs.notes,
        },
        ctx,
        fitters,
    )?;
    Ok(Calibration {
        design,
        measurements: inputs.measurements,
        config: inputs.config,
        resampled,
    })
}

pub(crate) fn prepare_inputs(
    mut inputs: EngineInputs,
    policy: secs_types::calibration::SampleRatePolicy,
    ctx: &secs_compute::Context,
) -> Result<(EngineInputs, usize), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)?;
    let resampled = inputs
        .measurements
        .positions
        .iter()
        .flat_map(|(_, roles)| roles)
        .filter(|(_, measurement)| measurement.sample_rate != crate::grid::SR)
        .count();
    inputs.measurements = crate::normalize_measurements(&inputs.measurements, policy, ctx)?;
    Ok((inputs, resampled))
}

pub fn engine_inputs(
    view: &ProjectView<'_>,
    irs: &MeasurementSet,
    settings: &CalibrationSettings,
) -> Result<EngineInputs, EngineError> {
    let mut skipped = BTreeSet::new();
    let mut positions = Vec::new();
    for (position, roles) in &irs.positions {
        let clean: Vec<_> = roles
            .iter()
            .filter_map(|(role, measurement)| {
                if role.contains(':') {
                    skipped.insert(role.clone());
                    None
                } else {
                    Some((role.clone(), measurement.clone()))
                }
            })
            .collect();
        if !clean.is_empty() {
            positions.push((*position, clean));
        }
    }
    let Some(main_position) = positions.iter().map(|(position, _)| *position).min() else {
        return Err(EngineError::Input("no measured impulse responses".into()));
    };
    let have: BTreeSet<_> = positions
        .iter()
        .find(|(position, _)| *position == main_position)
        .expect("minimum position exists")
        .1
        .iter()
        .map(|(role, _)| role.as_str())
        .collect();

    let groups: Vec<_> = view
        .groups
        .iter()
        .filter_map(|(group, roles)| {
            let roles: Vec<_> = roles
                .iter()
                .filter(|role| have.contains(role.as_str()))
                .cloned()
                .collect();
            (!roles.is_empty()).then(|| (group.clone(), roles))
        })
        .collect();
    let subs = groups
        .iter()
        .find(|(group, _)| group == "sub")
        .map_or_else(Vec::new, |(_, roles)| roles.clone());
    let mains: Vec<_> = groups
        .iter()
        .filter(|(group, _)| group != "sub")
        .flat_map(|(_, roles)| roles.iter().cloned())
        .collect();
    if mains.is_empty() {
        return Err(EngineError::Input("no measured main channels".into()));
    }

    let keep: BTreeSet<_> = mains.iter().chain(&subs).map(String::as_str).collect();
    for (_, roles) in &mut positions {
        roles.retain(|(role, _)| keep.contains(role.as_str()));
    }

    let xo = resolve_xo(&groups, &settings.xo, view.project_xo);
    let mode = mode_params(settings.mode, settings.ultra_ms);
    let sub_manual = settings
        .subs_manual
        .iter()
        .filter(|(_, entry)| entry.mode != "auto" || sub_has_filters(entry))
        .map(|(role, entry)| (role.clone(), manual(entry)))
        .collect();
    let bundle_layouts = settings
        .bundle_layouts
        .iter()
        .filter_map(|(bundle, entry)| {
            let kind = match entry.kind.as_str() {
                "dba" => LayoutKind::Dba,
                "multi" => LayoutKind::Multi,
                _ => return None,
            };
            Some((
                bundle.clone(),
                BundleLayout {
                    kind,
                    rear_trim_ms: entry.rear_trim_ms,
                },
            ))
        })
        .collect();
    let notes = if skipped.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "바이앰프 대역 측정 제외: {}",
            skipped.into_iter().collect::<Vec<_>>().join(", ")
        )]
    };

    Ok(EngineInputs {
        measurements: MeasurementSet { positions },
        config: EngineConfig {
            mains,
            subs,
            groups,
            target: view.target.clone(),
            xo,
            main_position,
            taps: mode.taps,
            lift_ms: mode.lift_ms,
            max_boost_db: 10.0_f64.min(settings.boost_max_db),
            peq_bands: mode.peq_bands,
            max_latency_ms: Some(mode.max_latency_ms),
            rc_range: (settings.rc_lo, settings.rc_hi),
            low_protect: settings.low_protect,
            room_volume_m3: settings.room_volume_m3,
            rc_hi_basis: settings.rc_hi_basis,
            rc_hi_transition: settings.rc_hi_transition,
            phase_frac: mode.phase_frac,
            sub_manual,
            sub_main_delay_ms: settings.sub_main_delay_ms,
            bundle_layouts,
            infra_boost_max_db: settings.infra_boost_max_db,
            xo_peq_on: settings.xo_peq_on,
            xo_peq_lo_hz: settings.xo_peq_lo,
            xo_peq_hi_hz: settings.xo_peq_hi,
        },
        notes,
    })
}

pub(crate) fn resolve_xo(
    groups: &[(String, Vec<String>)],
    selected: &[(String, f64)],
    project: &[(String, f64)],
) -> Vec<(String, f64)> {
    groups
        .iter()
        .filter(|(group, _)| group != "sub")
        .map(|(group, _)| {
            let value = selected
                .iter()
                .find(|(name, value)| name == group && *value != 0.0)
                .or_else(|| {
                    project
                        .iter()
                        .find(|(name, value)| name == group && *value != 0.0)
                })
                .map_or(80.0, |(_, value)| *value);
            (group.clone(), value)
        })
        .collect()
}

fn sub_has_filters(entry: &SubEntry) -> bool {
    entry.hp_hz != 0.0 || entry.lp_hz != 0.0 || entry.peq.iter().any(|band| band.2 != 0.0)
}

fn manual(entry: &SubEntry) -> SubManual {
    SubManual {
        mode: match entry.mode.as_str() {
            "fixed" => SubMode::Fixed,
            "bundle" => SubMode::Bundle,
            _ => SubMode::Auto,
        },
        bundle: entry.bundle.clone(),
        gain_db: entry.gain_db,
        delay_ms: entry.delay_ms,
        polarity: entry.polarity,
        allpass: entry.allpass.clone(),
        mimo_support: entry.mimo_support,
        hp_hz: entry.hp_hz,
        hp_type: BandType::from_name(&entry.hp_type),
        lp_hz: entry.lp_hz,
        lp_type: BandType::from_name(&entry.lp_type),
        peq: entry.peq.clone(),
        dba_row: if entry.dba_row == "rear" {
            DbaRow::Rear
        } else {
            DbaRow::Front
        },
    }
}

#[cfg(test)]
mod direct_tests {
    use super::*;
    use crate::input::{Measurement, Samples};
    use std::sync::Arc;

    fn measurement() -> Measurement {
        Measurement {
            samples: Samples::F32(Arc::from([1.0])),
            sample_rate: 48_000,
            t0: 0.0,
        }
    }

    #[test]
    fn excludes_biamp_and_non_reference_only_roles() {
        let groups = vec![
            ("front".into(), vec!["L".into(), "R".into()]),
            ("sub".into(), vec!["SW1".into()]),
        ];
        let target = TargetCurve::default();
        for (fixture, positions) in [
            (
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../oracle/goldens/calibration/adapter_biamp.json"
                )),
                vec![(
                    1,
                    vec![("L".into(), measurement()), ("L:LF".into(), measurement())],
                )],
            ),
            (
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../oracle/goldens/calibration/adapter_non_reference_only.json"
                )),
                vec![
                    (1, vec![("L".into(), measurement())]),
                    (
                        2,
                        vec![
                            ("L".into(), measurement()),
                            ("R".into(), measurement()),
                            ("SW1".into(), measurement()),
                        ],
                    ),
                ],
            ),
        ] {
            let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let expected = &expected["outputs"];
            let out = engine_inputs(
                &ProjectView {
                    groups: &groups,
                    project_xo: &[],
                    target: &target,
                },
                &MeasurementSet { positions },
                &CalibrationSettings::default(),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(&out.config.mains).unwrap(),
                expected["mains"]
            );
            assert_eq!(
                serde_json::to_value(&out.config.subs).unwrap(),
                expected["subs"]
            );
            assert_eq!(
                serde_json::to_value(&out.config.groups).unwrap(),
                expected["groups"]
            );
            assert_eq!(
                serde_json::to_value(&out.config.xo).unwrap(),
                expected["xo"]
            );
            assert_eq!(
                out.config.main_position,
                expected["main_position"].as_u64().unwrap() as u32
            );
            assert_eq!(serde_json::to_value(&out.notes).unwrap(), expected["notes"]);
            let actual_positions: Vec<_> = out
                .measurements
                .positions
                .iter()
                .map(|(position, roles)| {
                    (
                        *position,
                        roles.iter().map(|(role, _)| role).collect::<Vec<_>>(),
                    )
                })
                .collect();
            assert_eq!(
                serde_json::to_value(actual_positions).unwrap(),
                expected["positions"]
            );
        }
    }
}

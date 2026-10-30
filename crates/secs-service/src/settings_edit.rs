//! Calibration screen edit rules from calibration.py and step_calibration.py
//! (upstream SECS 2, GPL-3.0-only). The project adapter owns normalization.
use secs_project::{Project, calibration_from_project, store_calibration};
use secs_types::calibration::{BundleLayoutEntry, CalibrationSettings, SubEntry};
use secs_types::ipc::{CalibrationSettingsEditParams, IpcError};
use secs_types::rules::CALIBRATION_LIMITS as LIMITS;
use secs_types::settings_edit::*;

use crate::{Service, lock};

impl Service {
    pub(crate) fn calibration_settings_edit(
        &self,
        params: CalibrationSettingsEditParams,
    ) -> Result<CalibrationSettings, IpcError> {
        if let SettingEdit::RcHiTransition { factor } = params.edit
            && !matches!(factor, 1 | 2 | 4)
        {
            return Err(IpcError::new(
                secs_types::ipc::ErrorCode::InvalidRequest,
                "RC transition factor must be 1, 2 or 4",
            )
            .with_param("field", "rc_hi_transition"));
        }
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let mut settings = calibration_from_project(open.store.project());
        apply(&mut settings, params.edit, open.store.project());
        store_calibration(open.store.project_mut(), &settings);
        open.dirty = true;
        Ok(calibration_from_project(open.store.project()))
    }
}

fn sub(s: &CalibrationSettings, role: &str) -> SubEntry {
    s.subs_manual
        .iter()
        .find(|(r, _)| r == role)
        .map(|(_, e)| e.clone())
        .unwrap_or_default()
}

fn layout(s: &CalibrationSettings, bundle: &str) -> String {
    s.bundle_layouts
        .iter()
        .find(|(b, _)| b == bundle)
        .map(|(_, e)| e.kind.clone())
        .unwrap_or_else(|| "values".into())
}

fn members(s: &CalibrationSettings, bundle: &str) -> Vec<String> {
    s.subs_manual
        .iter()
        .filter(|(_, e)| e.mode == "bundle" && e.bundle == bundle)
        .map(|(r, _)| r.clone())
        .collect()
}

fn put<T>(items: &mut Vec<(String, T)>, key: String, value: T) {
    if let Some((_, entry)) = items.iter_mut().find(|(k, _)| *k == key) {
        *entry = value;
    } else {
        items.push((key, value));
    }
}

fn normalized(entry: SubEntry) -> SubEntry {
    let mut project = Project::default();
    let settings = CalibrationSettings {
        subs_manual: vec![("sub".into(), entry)],
        ..Default::default()
    };
    store_calibration(&mut project, &settings);
    calibration_from_project(&project).subs_manual.remove(0).1
}

fn share_filters(s: &mut CalibrationSettings, role: &str) {
    let e = sub(s, role);
    if e.mode != "bundle" || layout(s, &e.bundle) == "values" {
        return;
    }
    for (r, other) in &mut s.subs_manual {
        if r != role && other.mode == "bundle" && other.bundle == e.bundle {
            other.hp_hz = e.hp_hz;
            other.hp_type.clone_from(&e.hp_type);
            other.lp_hz = e.lp_hz;
            other.lp_type.clone_from(&e.lp_type);
            other.peq.clone_from(&e.peq);
        }
    }
}

fn set_sub(
    s: &mut CalibrationSettings,
    role: String,
    entry: SubEntry,
    joining: bool,
    filters: bool,
) {
    let entry = normalized(entry);
    let bundle = entry.bundle.clone();
    put(&mut s.subs_manual, role.clone(), entry);
    if joining {
        let others: Vec<_> = members(s, &bundle)
            .into_iter()
            .filter(|r| r != &role)
            .collect();
        if let Some(first) = others.first() {
            share_filters(s, first);
            return;
        }
        if !s.bundle_layouts.iter().any(|(b, _)| b == &bundle) {
            s.bundle_layouts
                .push((bundle, BundleLayoutEntry::default()));
        }
    }
    if filters {
        share_filters(s, &role);
    }
}

fn patch_sub(s: &mut CalibrationSettings, role: String, patch: SubPatch) {
    let filters = patch.hp_hz.is_some()
        || patch.hp_type.is_some()
        || patch.lp_hz.is_some()
        || patch.lp_type.is_some()
        || patch.peq.is_some();
    let mut e = sub(s, &role);
    if let Some(v) = patch.gain_db {
        e.gain_db = v;
    }
    if let Some(v) = patch.delay_ms {
        e.delay_ms = v;
    }
    if let Some(v) = patch.polarity {
        e.polarity = if v < 0.0 { -1 } else { 1 };
    }
    if let Some(v) = patch.allpass {
        e.allpass = v;
    }
    if let Some(v) = patch.mimo_support {
        e.mimo_support = v;
    }
    if let Some(v) = patch.hp_hz {
        e.hp_hz = v;
    }
    if let Some(v) = patch.lp_hz {
        e.lp_hz = v;
    }
    if let Some(v) = patch.peq {
        e.peq = v;
    }
    if let Some(v) = patch.hp_type {
        e.hp_type = v;
    }
    if let Some(v) = patch.lp_type {
        e.lp_type = v;
    }
    if let Some(v) = patch.dba_row {
        e.dba_row = v;
    }
    set_sub(s, role, e, false, filters);
}

fn set_layout(
    s: &mut CalibrationSettings,
    bundle: String,
    kind: Option<SubLayout>,
    trim: Option<f64>,
) {
    let mut e = s
        .bundle_layouts
        .iter()
        .find(|(b, _)| b == &bundle)
        .map(|(_, e)| e.clone())
        .unwrap_or_default();
    if let Some(kind) = kind {
        e.kind = match kind {
            SubLayout::Values => "values",
            SubLayout::Dba => "dba",
            SubLayout::Multi => "multi",
        }
        .into();
    }
    if let Some(trim) = trim {
        e.rear_trim_ms = trim.clamp(LIMITS.rear_trim.0, LIMITS.rear_trim.1);
    }
    put(&mut s.bundle_layouts, bundle.clone(), e);
    if let Some(first) = members(s, &bundle).first() {
        share_filters(s, first);
    }
}

fn infra_state(s: &CalibrationSettings, roles: &[String]) -> (Option<String>, f64) {
    let lows: Vec<_> = roles
        .iter()
        .filter(|r| {
            let e = sub(s, r);
            e.lp_hz > 0.0 && e.hp_hz == 0.0
        })
        .collect();
    if let Some(first) = lows.first()
        && roles
            .iter()
            .filter(|r| !lows.contains(r))
            .all(|r| sub(s, r).hp_hz > 0.0)
    {
        return (Some((*first).clone()), sub(s, first).lp_hz);
    }
    (None, 0.0)
}

fn set_infra(
    s: &mut CalibrationSettings,
    role: Option<String>,
    hz: f64,
    typ: &str,
    roles: &[String],
) {
    let (before, old_hz) = infra_state(s, roles);
    let Some(role) = role.filter(|r| !r.is_empty()) else {
        if let Some(before) = before {
            for r in roles {
                let mut e = sub(s, r);
                if r == &before {
                    e.lp_hz = 0.0;
                } else if e.hp_hz == old_hz {
                    e.hp_hz = 0.0;
                } else {
                    continue;
                }
                set_sub(s, r.clone(), e, false, true);
            }
        }
        return;
    };
    let hz = hz.clamp(LIMITS.infra_hz.0, LIMITS.infra_hz.1);
    let e = sub(s, &role);
    let mut group = if e.mode == "bundle" && layout(s, &e.bundle) != "values" {
        members(s, &e.bundle)
    } else {
        vec![]
    };
    group.push(role);
    for r in roles {
        let mut e = sub(s, r);
        if group.contains(r) {
            e.lp_hz = hz;
            e.lp_type = typ.into();
            e.hp_hz = 0.0;
        } else {
            if before.as_ref() == Some(r) || (e.lp_hz > 0.0 && e.lp_hz < 2.0 * hz.max(old_hz)) {
                e.lp_hz = 0.0;
            }
            e.hp_hz = hz;
            e.hp_type = typ.into();
        }
        set_sub(s, r.clone(), e, false, true);
    }
}

fn volume(dims: Option<[f64; 3]>) -> Option<f64> {
    let (lo, hi) = LIMITS.dim_valid;
    dims.filter(|d| d.iter().all(|v| v.is_finite() && (lo..=hi).contains(v)))
        .map(|d| d[0] * d[1] * d[2])
}

fn apply(s: &mut CalibrationSettings, edit: SettingEdit, project: &Project) {
    match edit {
        SettingEdit::Mode { mode } => s.mode = mode,
        SettingEdit::SampleRatePolicy { policy } => s.sample_rate_policy = policy,
        SettingEdit::UltraMs { ms } => s.ultra_ms = ms.clamp(LIMITS.ultra_ms.0, LIMITS.ultra_ms.1),
        SettingEdit::RcLo { hz } => s.rc_lo = hz.clamp(LIMITS.rc_lo.0, LIMITS.rc_lo.1),
        SettingEdit::RcHi { hz } => s.rc_hi = hz.map(|v| v.clamp(LIMITS.rc_hi.0, LIMITS.rc_hi.1)),
        SettingEdit::RcHiBasis { basis } => s.rc_hi_basis = basis,
        SettingEdit::RcHiTransition { factor } => s.rc_hi_transition = factor,
        SettingEdit::RoomVolume { m3 } => {
            s.room_volume_m3 = (m3 > LIMITS.volume.0).then_some(m3.min(LIMITS.volume.1));
            if !volume(s.room_dims_m)
                .zip(s.room_volume_m3)
                .is_some_and(|(a, b)| (a - b).abs() <= 0.5)
            {
                s.room_dims_m = None;
            }
        }
        SettingEdit::RoomDims { dims } => {
            let dims = dims.map(|v| v.clamp(LIMITS.dim.0, LIMITS.dim.1));
            s.room_dims_m = volume(Some(dims)).map(|_| dims);
            if let Some(v) = volume(s.room_dims_m) {
                // Python round(v, 1) rounds the original binary value to a decimal.
                s.room_volume_m3 = Some(
                    format!("{v:.1}")
                        .parse::<f64>()
                        .unwrap()
                        .min(LIMITS.volume.1),
                );
            }
        }
        SettingEdit::BoostMax { db } => s.boost_max_db = db.clamp(LIMITS.boost.0, LIMITS.boost.1),
        SettingEdit::Xo { group, hz } => {
            let groups = project.config.groups();
            if groups.iter().any(|(g, _)| *g == "sub")
                && groups.iter().any(|(g, _)| *g == group && *g != "sub")
            {
                put(
                    &mut s.xo,
                    group,
                    hz.clamp(LIMITS.xo.0, LIMITS.xo.1).round_ties_even(),
                );
            }
        }
        SettingEdit::XoPeq { on, lo, hi } => {
            if let Some(v) = on {
                s.xo_peq_on = v;
            }
            if let Some(v) = lo {
                s.xo_peq_lo = v.clamp(LIMITS.xo_peq.0, LIMITS.xo_peq.1);
            }
            if let Some(v) = hi {
                s.xo_peq_hi = v.clamp(LIMITS.xo_peq.0, LIMITS.xo_peq.1);
            }
        }
        SettingEdit::Infra {
            role,
            hz,
            typ,
            subs,
        } => set_infra(s, role, hz, typ.name(), &subs),
        SettingEdit::InfraBoost { db } => {
            s.infra_boost_max_db = db.clamp(LIMITS.infra_boost.0, LIMITS.infra_boost.1)
        }
        SettingEdit::SubMode { role, mode } => {
            let mut e = sub(s, &role);
            let (kind, bundle) = match mode {
                SubModeChoice::Auto => ("auto", e.bundle.clone()),
                SubModeChoice::Fixed => ("fixed", e.bundle.clone()),
                SubModeChoice::BundleA => ("bundle", "A".into()),
                SubModeChoice::BundleB => ("bundle", "B".into()),
            };
            let joining = kind == "bundle" && (e.mode != "bundle" || e.bundle != bundle);
            e.mode = kind.into();
            e.bundle = bundle;
            set_sub(s, role, e, joining, false);
        }
        SettingEdit::SubLayout { bundle, layout } => set_layout(s, bundle, Some(layout), None),
        SettingEdit::RearTrim { bundle, ms } => set_layout(s, bundle, None, Some(ms)),
        SettingEdit::Sub { role, patch } => patch_sub(s, role, patch),
        SettingEdit::SubBandType { role, band, typ } => {
            let e = sub(s, &role);
            let (hz, typ) = match typ {
                BandTypeChoice::Off(_) => (0.0, None),
                BandTypeChoice::Band(typ) => {
                    let (current, default) = match band {
                        SubBand::Hp => (e.hp_hz, 20.0),
                        SubBand::Lp => (e.lp_hz, 80.0),
                    };
                    (
                        if current > 0.0 { current } else { default },
                        Some(typ.name().into()),
                    )
                }
            };
            patch_sub(
                s,
                role,
                match band {
                    SubBand::Hp => SubPatch {
                        hp_hz: Some(hz),
                        hp_type: typ,
                        ..Default::default()
                    },
                    SubBand::Lp => SubPatch {
                        lp_hz: Some(hz),
                        lp_type: typ,
                        ..Default::default()
                    },
                },
            );
        }
        SettingEdit::MainDelay { ms } => {
            s.sub_main_delay_ms = ms.clamp(LIMITS.main_delay.0, LIMITS.main_delay.1)
        }
        SettingEdit::MimoOn { on } => s.mimo_on = on,
        SettingEdit::Fsiso { hz } => s.fsiso = hz.clamp(LIMITS.fsiso.0, LIMITS.fsiso.1),
        SettingEdit::MimoLow { hz } => {
            s.mimo_low = hz.map(|v| v.clamp(LIMITS.mimo_low.0, LIMITS.mimo_low.1))
        }
        SettingEdit::MimoMax { count } => {
            s.mimo_max_supports = count
                .clamp(LIMITS.mimo_max.0, LIMITS.mimo_max.1)
                .round_ties_even() as i64
        }
        SettingEdit::MimoFir { on } => s.mimo_fir = on,
        SettingEdit::AnalyticDerivatives { peq, mimo_iir } => {
            if let Some(on) = peq {
                s.peq_analytic_derivatives = on;
            }
            if let Some(on) = mimo_iir {
                s.mimo_iir_analytic_derivatives = on;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use secs_project::SpeakerConfig;
    use secs_types::chain::BandType;

    use super::*;

    /// Reads back the value one edit stored.
    type Read = fn(f64) -> f64;

    const FAR: f64 = 1e9;

    fn edited(edit: SettingEdit) -> CalibrationSettings {
        let project = Project {
            config: SpeakerConfig::from_preset("2.1", "", false).unwrap(),
            ..Project::default()
        };
        let mut s = calibration_from_project(&project);
        apply(&mut s, edit, &project);
        s
    }

    fn entry<T: Clone>(items: &[(String, T)], key: &str) -> T {
        items.iter().find(|(k, _)| k == key).unwrap().1.clone()
    }

    /// Every clamp lands on the rule constant at both edges.
    #[test]
    fn clamps_land_on_the_rule_limits() {
        let edges: [(&str, (f64, f64), Read); 16] = [
            ("ultra_ms", LIMITS.ultra_ms, |v| {
                edited(SettingEdit::UltraMs { ms: v }).ultra_ms
            }),
            ("rc_lo", LIMITS.rc_lo, |v| {
                edited(SettingEdit::RcLo { hz: v }).rc_lo
            }),
            ("rc_hi", LIMITS.rc_hi, |v| {
                edited(SettingEdit::RcHi { hz: Some(v) }).rc_hi.unwrap()
            }),
            ("boost", LIMITS.boost, |v| {
                edited(SettingEdit::BoostMax { db: v }).boost_max_db
            }),
            ("xo", LIMITS.xo, |v| {
                let group = "프론트".to_owned();
                entry(&edited(SettingEdit::Xo { group, hz: v }).xo, "프론트")
            }),
            ("xo_peq lo", LIMITS.xo_peq, |v| {
                let edit = SettingEdit::XoPeq {
                    on: None,
                    lo: Some(v),
                    hi: None,
                };
                edited(edit).xo_peq_lo
            }),
            ("xo_peq hi", LIMITS.xo_peq, |v| {
                let edit = SettingEdit::XoPeq {
                    on: None,
                    lo: None,
                    hi: Some(v),
                };
                edited(edit).xo_peq_hi
            }),
            ("infra_hz", LIMITS.infra_hz, |v| {
                let edit = SettingEdit::Infra {
                    role: Some("SW1".into()),
                    hz: v,
                    typ: BandType::Lr24,
                    subs: vec!["SW1".into()],
                };
                sub(&edited(edit), "SW1").lp_hz
            }),
            ("infra_boost", LIMITS.infra_boost, |v| {
                edited(SettingEdit::InfraBoost { db: v }).infra_boost_max_db
            }),
            ("rear_trim", LIMITS.rear_trim, |v| {
                let edit = SettingEdit::RearTrim {
                    bundle: "A".into(),
                    ms: v,
                };
                entry(&edited(edit).bundle_layouts, "A").rear_trim_ms
            }),
            ("main_delay", LIMITS.main_delay, |v| {
                edited(SettingEdit::MainDelay { ms: v }).sub_main_delay_ms
            }),
            ("fsiso", LIMITS.fsiso, |v| {
                edited(SettingEdit::Fsiso { hz: v }).fsiso
            }),
            ("mimo_low", LIMITS.mimo_low, |v| {
                edited(SettingEdit::MimoLow { hz: Some(v) })
                    .mimo_low
                    .unwrap()
            }),
            ("mimo_max", LIMITS.mimo_max, |v| {
                edited(SettingEdit::MimoMax { count: v }).mimo_max_supports as f64
            }),
            // At or below the low end the volume is cleared.
            ("volume", LIMITS.volume, |v| {
                let volume = edited(SettingEdit::RoomVolume { m3: v }).room_volume_m3;
                volume.unwrap_or(LIMITS.volume.0)
            }),
            // A low-edge dimension yields no room, so the other two sit at the high edge.
            ("dim", LIMITS.dim, |v| {
                let high = LIMITS.dim.1;
                let room = edited(SettingEdit::RoomDims {
                    dims: [v, high, high],
                });
                room.room_dims_m.map_or(LIMITS.dim.0, |d| d[0])
            }),
        ];
        for (name, (lo, hi), read) in edges {
            assert_eq!(read(-FAR), lo, "{name} low edge");
            assert_eq!(read(FAR), hi, "{name} high edge");
        }
    }

    #[test]
    fn room_dimensions_yield_a_volume_only_within_the_valid_range() {
        let room = |d: f64| edited(SettingEdit::RoomDims { dims: [d; 3] });
        let (lo, hi) = LIMITS.dim_valid;
        assert_eq!(room(lo).room_dims_m, Some([lo; 3]));
        assert_eq!(room(hi).room_dims_m, Some([hi; 3]));
        assert_eq!(room(hi).room_volume_m3, Some(LIMITS.volume.1));
        assert_eq!(room(lo - 0.01).room_dims_m, None);
        let cleared = edited(SettingEdit::RoomVolume {
            m3: LIMITS.volume.0,
        });
        assert_eq!(cleared.room_volume_m3, None);
    }
}

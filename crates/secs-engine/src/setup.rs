//! Constructor normalisation, in upstream dictionary order.
//! Upstream: `CorrectionEngine.__init__` (`correction_engine.py:418–489`).

use crate::grid::SR_F;
use crate::input::{BandType, EngineConfig, LayoutKind, Peq, SubManual, SubMode};

#[derive(Debug, Clone, PartialEq)]
pub struct SubFilter {
    pub hp_hz: f64,
    pub hp_type: BandType,
    pub lp_hz: f64,
    pub lp_type: BandType,
    pub peq: Vec<Peq>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub kind: LayoutKind,
    pub rear_trim_ms: f64,
    pub members: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Setup {
    pub sub_manual: Vec<(String, SubManual)>,
    pub sub_filters: Vec<(String, SubFilter)>,
    pub layouts: Vec<(String, Layout)>,
    pub lift_ms: f64,
    pub lift_capped_from_ms: Option<f64>,
    pub max_latency_ms: f64,
    pub rc_lo: f64,
    pub rc_hi_manual: Option<f64>,
    pub rc_hi: f64,
    pub rc_bands: Vec<(f64, f64)>,
    /// ADR 0025 switches; the agreement probe clears both.
    pub lf_align: bool,
    pub mode_decay: bool,
    /// ADR 0030; the agreement probe also clears this switch.
    pub hf_phase: bool,
    pub peq_bands: usize,
}

impl Setup {
    pub fn new(config: &EngineConfig) -> Self {
        let sub_manual: Vec<_> = config
            .sub_manual
            .iter()
            .filter(|(r, _)| config.subs.contains(r))
            .cloned()
            .collect();
        let mut sub_filters: Vec<_> = sub_manual
            .iter()
            .filter_map(|(r, m)| {
                let peq: Vec<_> = m.peq.iter().copied().filter(|b| b.2 != 0.0).collect();
                (m.hp_hz > 0.0 || m.lp_hz > 0.0 || !peq.is_empty()).then(|| {
                    (
                        r.clone(),
                        SubFilter {
                            hp_hz: m.hp_hz,
                            hp_type: m.hp_type,
                            lp_hz: m.lp_hz,
                            lp_type: m.lp_type,
                            peq,
                        },
                    )
                })
            })
            .collect();
        let mut layouts = Vec::new();
        for (name, layout) in &config.bundle_layouts {
            let members: Vec<_> = config
                .subs
                .iter()
                .filter(|r| {
                    sub_manual.iter().any(|(role, m)| {
                        role == *r
                            && m.mode == SubMode::Bundle
                            && (if m.bundle.is_empty() { "A" } else { &m.bundle }) == name
                    })
                })
                .cloned()
                .collect();
            let Some(first) = members.first() else {
                continue;
            };
            let lead = sub_filters
                .iter()
                .find(|(r, _)| r == first)
                .map(|(_, f)| f.clone());
            for r in &members[1..] {
                if let Some(filter) = &lead {
                    if let Some((_, value)) = sub_filters.iter_mut().find(|(role, _)| role == r) {
                        *value = filter.clone();
                    } else {
                        sub_filters.push((r.clone(), filter.clone()));
                    }
                } else {
                    sub_filters.retain(|(role, _)| role != r);
                }
            }
            layouts.push((
                name.clone(),
                Layout {
                    kind: layout.kind,
                    rear_trim_ms: layout.rear_trim_ms,
                    members,
                },
            ));
        }
        let cap_ms = 0.7 * config.taps as f64 / SR_F * 1000.0;
        let capped = config.lift_ms > cap_ms;
        let lift_ms = if capped { cap_ms } else { config.lift_ms };
        let max_latency_ms = config.max_latency_ms.map_or(lift_ms + 15.0, |v| {
            if capped {
                v - (config.lift_ms - cap_ms)
            } else {
                v
            }
        });
        Self {
            sub_manual,
            sub_filters,
            layouts,
            lift_ms,
            lift_capped_from_ms: capped.then_some(config.lift_ms),
            max_latency_ms,
            rc_lo: config.rc_range.0,
            rc_hi_manual: config.rc_range.1,
            rc_hi: config.rc_range.1.filter(|v| *v != 0.0).unwrap_or(500.0),
            rc_bands: config.rc_bands.clone(),
            lf_align: config.lf_align,
            mode_decay: config.mode_decay,
            hf_phase: config.hf_phase,
            peq_bands: config.peq_bands.unwrap_or(16),
        }
    }
}

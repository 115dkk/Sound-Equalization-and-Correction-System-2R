//! Stable identity for one exported correction design.

use crate::{LfeChoice, SAMPLE_RATE};
use secs_types::chain::{MimoPaths, SignalChain};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct CanonicalDesign<'a> {
    schema: u32,
    sample_rate: u32,
    chain: &'a SignalChain,
    mimo: Option<&'a MimoPaths>,
    lfe: Option<CanonicalLfe>,
}

#[derive(Serialize)]
struct CanonicalLfe {
    gain_db: f64,
    lp_hz: f64,
}

/// Lowercase hexadecimal SHA-256 of the canonical design JSON.
///
/// The canonical object's field order is fixed by these structures. The
/// workspace's `serde_json` configuration emits shortest round-trip floats.
pub fn design_id(
    chain: &SignalChain,
    mimo: Option<&MimoPaths>,
    lfe: LfeChoice,
) -> Result<String, serde_json::Error> {
    let lfe = match lfe {
        LfeChoice::Omit => None,
        LfeChoice::Include { gain_db, lp_hz } => Some(CanonicalLfe { gain_db, lp_hz }),
    };
    let canonical = CanonicalDesign {
        schema: 1,
        sample_rate: SAMPLE_RATE,
        chain,
        mimo,
        lfe,
    };
    let digest = Sha256::digest(serde_json::to_vec(&canonical)?);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_types::chain::{MainChannel, MimoFilter, MimoPath};

    fn chain() -> SignalChain {
        SignalChain {
            input_hp_hz: None,
            sample_rate: SAMPLE_RATE,
            mains: vec![MainChannel {
                role: "L".into(),
                input_peq: vec![(80.0, 1.0, -2.0)],
                fir: vec![1.0, 0.25],
                gain_db: 0.0,
                delay_ms: 1.0,
                xo_hz: Some(80.0),
                top_peq: Vec::new(),
                align_delay_ms: 0.0,
            }],
            subs: Vec::new(),
            sub_bus_peq: Vec::new(),
            main_extra_ms: 0.0,
            lift_ms: 0.0,
            latency_ms: 1.0,
        }
    }

    #[test]
    fn design_id_is_stable_and_changes_with_taps_delays_and_mimo() {
        let chain = chain();
        let first = design_id(&chain, None, LfeChoice::DOLBY).unwrap();
        assert_eq!(first, design_id(&chain, None, LfeChoice::DOLBY).unwrap());
        assert_eq!(first.len(), 64);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );

        let mut tap = chain.clone();
        tap.mains[0].fir[1] += 1e-12;
        assert_ne!(first, design_id(&tap, None, LfeChoice::DOLBY).unwrap());

        let mut delay = chain.clone();
        delay.mains[0].delay_ms += 1.0 / 48.0;
        assert_ne!(first, design_id(&delay, None, LfeChoice::DOLBY).unwrap());

        let mimo = MimoPaths {
            fsiso: 120.0,
            paths: vec![MimoPath {
                input: "L".into(),
                support: "L".into(),
                filter: MimoFilter::Iir {
                    gain_db: -6.0,
                    delay_ms: 0.0,
                    polarity: 1,
                    peq: Vec::new(),
                    allpass: Vec::new(),
                },
                low_hz: 40.0,
                delay_ms: 0.0,
            }],
        };
        assert_ne!(
            first,
            design_id(&chain, Some(&mimo), LfeChoice::DOLBY).unwrap()
        );
    }
}

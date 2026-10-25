//! The one validation of a chain, its MIMO paths and the LFE choice.
//!
//! Each rule is the strictest of the versions the writers carried before
//! #92 C11: the CamillaDSP writer's field names and finite checks, the
//! filter writers' letters-and-digits identifiers, P77's duplicate-role
//! (TD-46) and negative-delay (TD-94) rules, and one low-rate FIR rule.

use crate::plan::lowrate_fits;
use crate::{GraphError, LfeChoice, SAMPLE_RATE, is_negative_delay};
use secs_types::chain::{MainChannel, MimoFilter, MimoPaths, Peq, SignalChain, SubChannel};

fn finite(name: &str, value: f64) -> Result<(), GraphError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(GraphError::NonFinite(name.into()))
    }
}

/// A delay the writers emit: finite and not negative (#92 TD-94). The
/// message names the field and, as the filter writers did, the channel.
fn non_negative_delay(name: &str, channel: &str, milliseconds: f64) -> Result<(), GraphError> {
    finite(name, milliseconds)?;
    if is_negative_delay(milliseconds) {
        return Err(GraphError::Invalid(format!(
            "negative delay {name} = {milliseconds} ms on {channel:?}"
        )));
    }
    Ok(())
}

/// Roles become YAML keys, channel names and coefficient filenames, so an
/// identifier is non-empty ASCII letters and digits.
fn identifier(name: &str, value: &str) -> Result<(), GraphError> {
    if !value.is_ascii() {
        return Err(GraphError::NonAscii(format!("{name} = {value:?}")));
    }
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(GraphError::Invalid(format!(
            "unsafe identifier {name} = {value:?}"
        )));
    }
    Ok(())
}

/// Each role is unique and none is `LFE`, which names the LFE input (#92 TD-46).
fn reserve_role<'a>(roles: &mut Vec<&'a str>, name: &str, role: &'a str) -> Result<(), GraphError> {
    if roles.contains(&role) || role == "LFE" {
        return Err(GraphError::Invalid(format!(
            "duplicate or reserved role {role:?} at {name}"
        )));
    }
    roles.push(role);
    Ok(())
}

fn polarity(name: &str, value: i8) -> Result<(), GraphError> {
    if matches!(value, -1 | 1) {
        Ok(())
    } else {
        Err(GraphError::Invalid(format!(
            "{name} {value} (expected 1 or -1)"
        )))
    }
}

fn peqs(prefix: &str, bands: &[Peq]) -> Result<(), GraphError> {
    for (index, (frequency, q, gain)) in bands.iter().enumerate() {
        finite(&format!("{prefix}[{index}].frequency"), *frequency)?;
        finite(&format!("{prefix}[{index}].q"), *q)?;
        finite(&format!("{prefix}[{index}].gain"), *gain)?;
    }
    Ok(())
}

fn allpasses(prefix: &str, bands: &[(f64, f64)]) -> Result<(), GraphError> {
    for (index, (frequency, q)) in bands.iter().enumerate() {
        finite(&format!("{prefix}[{index}].frequency"), *frequency)?;
        finite(&format!("{prefix}[{index}].q"), *q)?;
    }
    Ok(())
}

pub(crate) fn graph(
    chain: &SignalChain,
    mimo: Option<&MimoPaths>,
    lfe: LfeChoice,
) -> Result<(), GraphError> {
    if chain.sample_rate != SAMPLE_RATE {
        return Err(GraphError::Invalid(format!(
            "chain.sample_rate {} (expected {SAMPLE_RATE})",
            chain.sample_rate
        )));
    }
    finite("chain.main_extra_ms", chain.main_extra_ms)?;
    finite("chain.lift_ms", chain.lift_ms)?;
    finite("chain.latency_ms", chain.latency_ms)?;
    for (
        index,
        MainChannel {
            role,
            input_peq,
            fir,
            gain_db,
            delay_ms,
            xo_hz,
            top_peq,
            align_delay_ms,
        },
    ) in chain.mains.iter().enumerate()
    {
        identifier(&format!("main[{index}].role"), role)?;
        peqs(&format!("main.{role}.input_peq"), input_peq)?;
        peqs(&format!("main.{role}.top_peq"), top_peq)?;
        finite(&format!("main.{role}.gain_db"), *gain_db)?;
        non_negative_delay(&format!("main.{role}.delay_ms"), role, *delay_ms)?;
        finite(&format!("main.{role}.align_delay_ms"), *align_delay_ms)?;
        if let Some(value) = xo_hz {
            finite(&format!("main.{role}.xo_hz"), *value)?;
        }
        for (tap, value) in fir.iter().enumerate() {
            finite(&format!("main.{role}.fir[{tap}]"), *value)?;
        }
    }
    let mut roles = Vec::new();
    for (index, main) in chain.mains.iter().enumerate() {
        reserve_role(&mut roles, &format!("main[{index}].role"), &main.role)?;
    }
    for (
        index,
        SubChannel {
            role,
            gain_db,
            delay_ms,
            polarity: sign,
            allpass,
            hp_hz,
            lp_hz,
            peq,
            mode,
            ..
        },
    ) in chain.subs.iter().enumerate()
    {
        identifier(&format!("sub[{index}].role"), role)?;
        reserve_role(&mut roles, &format!("sub[{index}].role"), role)?;
        finite(&format!("sub.{role}.gain_db"), *gain_db)?;
        non_negative_delay(&format!("sub.{role}.delay_ms"), role, *delay_ms)?;
        finite(&format!("sub.{role}.hp_hz"), *hp_hz)?;
        finite(&format!("sub.{role}.lp_hz"), *lp_hz)?;
        polarity(&format!("sub.{role}.polarity"), *sign)?;
        if !mode.is_ascii() {
            return Err(GraphError::NonAscii(format!("sub.{role}.mode = {mode:?}")));
        }
        allpasses(&format!("sub.{role}.allpass"), allpass)?;
        peqs(&format!("sub.{role}.peq"), peq)?;
    }
    peqs("sub_bus_peq", &chain.sub_bus_peq)?;
    if let Some(mimo) = mimo {
        validate_mimo(chain, mimo)?;
    }
    if let LfeChoice::Include { gain_db, lp_hz } = lfe {
        finite("lfe.gain_db", gain_db)?;
        finite("lfe.lp_hz", lp_hz)?;
    }
    Ok(())
}

fn validate_mimo(chain: &SignalChain, mimo: &MimoPaths) -> Result<(), GraphError> {
    finite("mimo.fsiso", mimo.fsiso)?;
    for (index, path) in mimo.paths.iter().enumerate() {
        let field = format!("mimo.paths[{index}]");
        identifier(&format!("{field}.input"), &path.input)?;
        identifier(&format!("{field}.support"), &path.support)?;
        if !chain.mains.iter().any(|main| main.role == path.input) {
            return Err(GraphError::Invalid(format!(
                "{field}.input {} is not a main channel",
                path.input
            )));
        }
        if !chain.mains.iter().any(|main| main.role == path.support)
            && !chain.subs.iter().any(|sub| sub.role == path.support)
        {
            return Err(GraphError::Invalid(format!(
                "{field}.support {} is not an output",
                path.support
            )));
        }
        let channel = format!("{}->{}", path.input, path.support);
        finite(&format!("{field}.low_hz"), path.low_hz)?;
        non_negative_delay(&format!("{field}.delay_ms"), &channel, path.delay_ms)?;
        match &path.filter {
            MimoFilter::Iir {
                gain_db,
                delay_ms,
                polarity: sign,
                peq,
                allpass,
            } => {
                finite(&format!("{field}.gain_db"), *gain_db)?;
                non_negative_delay(&format!("{field}.filter_delay_ms"), &channel, *delay_ms)?;
                polarity(&format!("{field}.polarity"), *sign)?;
                peqs(&format!("{field}.peq"), peq)?;
                allpasses(&format!("{field}.allpass"), allpass)?;
            }
            MimoFilter::Fir { taps_48k } => {
                for (tap, value) in taps_48k.iter().enumerate() {
                    finite(&format!("{field}.taps_48k[{tap}]"), *value)?;
                }
            }
            MimoFilter::FirLowRate { taps_low, fs_low } => {
                finite(&format!("{field}.fs_low"), *fs_low)?;
                if !lowrate_fits(taps_low.len(), *fs_low) {
                    return Err(GraphError::Invalid(format!(
                        "invalid or oversized low-rate FIR {field}.taps_low: {} taps at fs_low {fs_low} Hz",
                        taps_low.len()
                    )));
                }
                for (tap, value) in taps_low.iter().enumerate() {
                    finite(&format!("{field}.taps_low[{tap}]"), *value)?;
                }
            }
        }
    }
    Ok(())
}

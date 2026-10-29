#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Filter-value text and Equalizer APO configuration export.
//!
//! Upstream sources: `export_filters.py` and the calibration-level wrappers in
//! `calibration.py`. The writers serialise the plan of an [`ExportGraph`],
//! whose construction is the only validation of the [`SignalChain`] and its
//! optional [`MimoPaths`]; a run's own label, stamp and numbers are checked
//! here. Validation and serialization finish before the first filesystem
//! change.

pub mod playback;
pub mod render;

pub use render::{RenderedPath, RenderedPaths, render_paths, render_paths_at};

use secs_camilla::pyfmt::{format_fixed, round_ndigits};
use secs_dsp::Complex64;
use secs_graph::files::{safe_name, wav_bytes, write_bundle_observed};
use secs_graph::{
    ExportGraph, FirView, GraphError, LFE_GAIN_DB, LFE_LP_HZ, LfeChoice, SAMPLE_RATE,
    is_negative_delay, round_ties_even_i64,
};
pub use secs_graph::{FilterPlan, MimoFeed, Op, SubFeed};
use secs_types::chain::{BandType, MimoPaths, SignalChain};
use secs_types::ipc::{ExportBlock, ExportKind, filter_export_block};
use std::borrow::Cow;

use std::path::{Path, PathBuf};

/// Equalizer APO application guide filename.
pub const EQAPO_GUIDE: &str = "EqAPO 적용 방법.txt";
/// Complete convolution bundle application guide filename.
pub const CONVOLUTION_GUIDE: &str = "컨볼루션 적용 방법.txt";
/// Maximum impulse-response length of a convolution export.
pub const CONVOLUTION_MAX_LEN: usize = 1 << 20;

/// Band-filter family and the Q values of its second-order sections.
#[allow(clippy::approx_constant)] // Upstream deliberately rounds these Q values to five decimals.
pub const SECTION_Q: [(&str, &[f64]); 5] = [
    ("LR12", &[0.5]),
    ("LR24", &[0.70711, 0.70711]),
    ("LR48", &[0.5412, 1.30656, 0.5412, 1.30656]),
    ("BW12", &[0.70711]),
    ("BW24", &[0.5412, 1.30656]),
];

const WINDOWS_POSITION: [(&str, usize); 14] = [
    ("L", 0),
    ("R", 1),
    ("C", 2),
    ("LFE", 3),
    ("Lb", 4),
    ("Rb", 5),
    ("Lw", 6),
    ("Rw", 7),
    ("Ls", 9),
    ("Rs", 10),
    ("Tfl", 12),
    ("Tfr", 14),
    ("Tbl", 15),
    ("Tbr", 17),
];
const WINDOWS_NAMES: [&str; 18] = [
    "FL", "FR", "FC", "LFE", "BL", "BR", "FLC", "FRC", "BC", "SL", "SR", "TC", "TFL", "TFC", "TFR",
    "TBL", "TBC", "TBR",
];

/// Values read by upstream's calibration-level filter export wrappers.
#[derive(Debug, Clone, Copy)]
pub struct FilterRun<'a> {
    /// Designed signal chain.
    pub chain: &'a SignalChain,
    /// MIMO paths only when the run includes MIMO.
    pub mimo: Option<&'a MimoPaths>,
    /// Mode name (`secs_camilla::note::mode_name`).
    pub mode: &'a str,
    /// Equalizer APO's ASCII header note (`secs_camilla::note::eqapo_note`).
    pub note: &'a str,
    /// Human-facing mode label.
    pub label: &'a str,
    /// Total filter latency.
    pub latency_ms: f64,
    /// Upstream `rc_pieces(run)` strings.
    pub rc_pieces: &'a [String],
    /// Caller-supplied `%Y-%m-%d %H:%M` stamp.
    pub stamp: &'a str,
}

/// Filter export failure.
#[derive(Debug, thiserror::Error)]
pub enum FiltersError {
    /// The chosen format cannot carry the run.
    #[error("filter export scope blocked: {0:?}")]
    Scope(ExportBlock),
    /// Validation failed before any filesystem change.
    #[error("invalid filter export: {0}")]
    Invalid(String),
    /// Filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<GraphError> for FiltersError {
    fn from(error: GraphError) -> Self {
        Self::Invalid(match error {
            GraphError::NonAscii(detail) => format!("non-ASCII {detail}"),
            GraphError::NonFinite(detail) => format!("non-finite {detail}"),
            GraphError::Invalid(detail) => detail,
        })
    }
}

fn find_ops<'a>(items: &'a [(String, Vec<Op>)], role: &str) -> Option<&'a [Op]> {
    items
        .iter()
        .find(|(key, _)| key == role)
        .map(|(_, value)| value.as_slice())
}

/// Convert a signal-chain value into the upstream filter plan, with
/// upstream's Dolby LFE.
pub fn make_plan(
    chain: &SignalChain,
    mimo: Option<&MimoPaths>,
) -> Result<FilterPlan, FiltersError> {
    make_plan_with_lfe(chain, mimo, Some(LFE_GAIN_DB), LFE_LP_HZ)
}

/// Convert a signal-chain value with an explicit optional LFE feed: the
/// plan of [`ExportGraph::build`].
pub fn make_plan_with_lfe(
    chain: &SignalChain,
    mimo: Option<&MimoPaths>,
    lfe_gain_db: Option<f64>,
    lfe_lp_hz: f64,
) -> Result<FilterPlan, FiltersError> {
    let lfe = lfe_gain_db.map_or(LfeChoice::Omit, |gain_db| LfeChoice::Include {
        gain_db,
        lp_hz: lfe_lp_hz,
    });
    plain_roles(chain, mimo)?;
    let graph = ExportGraph::build(chain, mimo, lfe)?;
    // The low-pass stays a checked parameter even when no LFE input is built.
    finite("lfe_lp_hz", lfe_lp_hz)?;
    Ok(graph.plan().clone())
}

/// [`secs_graph::plan_responses`]: electrical responses from one input to
/// every reachable output, on the 48 kHz real-FFT grid when the plan holds a
/// FIR.
pub fn plan_responses(
    plan: &FilterPlan,
    f: &[f64],
    source: &str,
    skip_fixed: bool,
) -> Result<Vec<(String, Vec<Complex64>)>, FiltersError> {
    Ok(secs_graph::plan_responses(plan, f, source, skip_fixed)?)
}

/// [`secs_graph::headroom_db`]: the largest non-LFE output boost from 20 Hz
/// through 20 kHz.
pub fn headroom_db(plan: &FilterPlan) -> Result<f64, FiltersError> {
    Ok(secs_graph::headroom_db(plan)?)
}

fn position(role: &str) -> Option<usize> {
    WINDOWS_POSITION
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, value)| *value)
}

/// One ordered role-to-channel-number table.
pub type ChannelNumbers = Vec<(String, usize)>;

/// Return default one-based Windows input and output channel numbers.
pub fn windows_channels(plan: &FilterPlan) -> (ChannelNumbers, ChannelNumbers) {
    let subs = plan
        .outputs
        .iter()
        .filter(|role| plan.subs.iter().any(|(name, _)| name == *role))
        .collect::<Vec<_>>();
    let mut placed = Vec::<(String, usize)>::new();
    for role in &plan.outputs {
        if plan.subs.iter().any(|(name, _)| name == role) {
            if subs.first().is_some_and(|first| *first == role) {
                placed.push((role.clone(), 3));
            }
        } else if let Some(value) = position(role) {
            placed.push((role.clone(), value));
        }
    }
    let mut used = placed.iter().map(|(_, value)| *value).collect::<Vec<_>>();
    if plan.inputs.iter().any(|role| role == "LFE") {
        used.push(3);
    }
    used.sort_unstable();
    used.dedup();
    let mut outputs = placed
        .into_iter()
        .map(|(role, value)| {
            (
                role,
                used.iter().position(|item| *item == value).expect("used") + 1,
            )
        })
        .collect::<Vec<_>>();
    let mut next = used.len();
    for role in &plan.outputs {
        if !outputs.iter().any(|(name, _)| name == role) {
            next += 1;
            outputs.push((role.clone(), next));
        }
    }
    let inputs = plan
        .inputs
        .iter()
        .map(|role| {
            let number = if role == "LFE" {
                used.iter()
                    .position(|item| *item == 3)
                    .expect("LFE position")
                    + 1
            } else {
                outputs
                    .iter()
                    .find(|(name, _)| name == role)
                    .expect("main output")
                    .1
            };
            (role.clone(), number)
        })
        .collect();
    (inputs, outputs)
}

/// Windows speaker-position name for the guide, or `-` when none exists.
pub fn windows_name(role: &str, plan: &FilterPlan) -> &'static str {
    let first_sub = plan
        .outputs
        .iter()
        .find(|candidate| plan.subs.iter().any(|(name, _)| name == *candidate));
    if role == "LFE" || first_sub.is_some_and(|first| first == role) {
        return "LFE";
    }
    if plan.subs.iter().any(|(name, _)| name == role) {
        return "-";
    }
    position(role).map_or("-", |value| WINDOWS_NAMES[value])
}

fn virtual_channel(kind: &str, role: &str) -> String {
    format!(
        "{}{}",
        match kind {
            "main" => "VM",
            "send" => "VS",
            _ => "VB",
        },
        role.to_ascii_uppercase()
    )
}
fn path_channel(index: usize) -> String {
    format!("VP{}", index + 1)
}
fn input_channel(role: &str) -> String {
    if role == "LFE" {
        "VLFE".into()
    } else {
        virtual_channel("main", role)
    }
}
fn q_values(kind: BandType) -> &'static [f64] {
    SECTION_Q
        .iter()
        .find(|(name, _)| *name == kind.name())
        .expect("known band type")
        .1
}
fn map_number(items: &[(String, usize)], role: &str) -> usize {
    items
        .iter()
        .find(|(name, _)| name == role)
        .expect("numbered role")
        .1
}

fn eqapo_ops<'a>(
    ops: impl IntoIterator<Item = &'a Op>,
    channel: &str,
    firs: &[(String, String)],
) -> Vec<String> {
    let mut lines = Vec::new();
    for op in ops {
        match op {
            Op::Peq {
                frequency,
                q,
                gain_db,
                ..
            } => lines.push(format!(
                "Filter: ON PK Fc {} Hz Gain {} dB Q {}",
                format_fixed(*frequency, 2, false),
                format_fixed(*gain_db, 2, false),
                format_fixed(*q, 4, false)
            )),
            Op::Fir { name, .. } | Op::FirLowRate { name, .. } => lines.push(format!(
                "Convolution: {}",
                firs.iter()
                    .find(|(key, _)| key == name)
                    .expect("FIR name")
                    .1
            )),
            Op::Gain { db, .. } if db.abs() >= 0.005 => {
                lines.push(format!("Preamp: {} dB", format_fixed(*db, 2, false)))
            }
            Op::Gain { .. } => {}
            Op::Invert { .. } => lines.push(format!("Copy: {channel}=-1.0*{channel}")),
            Op::Hp {
                frequency, kind, ..
            }
            | Op::Lp {
                frequency, kind, ..
            } => {
                let prefix = if matches!(op, Op::Hp { .. }) {
                    "HPQ"
                } else {
                    "LPQ"
                };
                for q in q_values(*kind) {
                    lines.push(format!(
                        "Filter: ON {prefix} Fc {} Hz Q {}",
                        format_fixed(*frequency, 2, false),
                        format_fixed(*q, 4, false)
                    ));
                }
            }
            Op::Ap { frequency, q, .. } => lines.push(format!(
                "Filter: ON AP Fc {} Hz Q {}",
                format_fixed(*frequency, 2, false),
                format_fixed(*q, 4, false)
            )),
            Op::Delay { ms, .. } if *ms > 0.0 => {
                lines.push(format!("Delay: {} ms", format_fixed(*ms, 3, false)))
            }
            Op::Delay { .. } => {}
        }
    }
    lines
}

/// Build the ASCII Equalizer APO filter body.
pub fn eqapo_filters(plan: &FilterPlan, firs: &[(String, String)], note: &str) -> String {
    let mut lines = vec![
        "# SECS 2 calibration filters for Equalizer APO (generated - do not edit).".into(),
        "# Included by config.txt between the input Copy and the output Copy.".into(),
    ];
    if !note.is_empty() {
        lines.push(format!(
            "# {}",
            note.chars().filter(char::is_ascii).collect::<String>()
        ));
    }
    if !plan.mimo.is_empty() {
        lines.extend([
            String::new(),
            "# Raw input copies for MIMO support".into(),
            format!(
                "Copy: {}",
                plan.mimo
                    .iter()
                    .enumerate()
                    .map(|(index, path)| format!(
                        "{}={}",
                        path_channel(index),
                        virtual_channel("main", &path.input)
                    ))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ]);
    }
    lines.extend([String::new(), "# 1) input correction (PEQ -> FIR)".into()]);
    for (role, ops) in &plan.input_ops {
        let channel = virtual_channel("main", role);
        let body = eqapo_ops(ops, &channel, firs);
        if !body.is_empty() {
            lines.push(format!("Channel: {channel}"));
            lines.extend(body);
        }
    }

    // Compare rendered coefficients, so merging cannot change the writer's
    // existing Fc/Q rounding. Membership also guards hand-constructed plans
    // in which different sub outputs use different subsets of the inputs.
    struct SendGroup<'a> {
        ops: &'a [Op],
        signature: Vec<String>,
        members: Vec<&'a str>,
        membership: Vec<usize>,
    }
    let mut sends: Vec<SendGroup<'_>> = Vec::new();
    for (role, ops) in &plan.send_ops {
        let signature = eqapo_ops(ops, "VX", firs);
        let membership = plan
            .subs
            .iter()
            .map(|(_, feed)| feed.sources.iter().filter(|source| *source == role).count())
            .collect::<Vec<_>>();
        if let Some(group) = sends
            .iter_mut()
            .find(|group| group.signature == signature && group.membership == membership)
        {
            group.members.push(role);
        } else {
            sends.push(SendGroup {
                ops,
                signature,
                members: vec![role],
                membership,
            });
        }
    }
    if !sends.is_empty() {
        lines.extend([
            String::new(),
            "# 2) sum equal-XO sends, then filter once".into(),
        ]);
    }
    for (index, group) in sends.iter().enumerate() {
        let channel = format!("VX{}", index + 1);
        lines.push(format!(
            "Copy: {channel}={}",
            group
                .members
                .iter()
                .map(|role| virtual_channel("main", role))
                .collect::<Vec<_>>()
                .join("+")
        ));
        lines.push(format!("Channel: {channel}"));
        lines.extend(eqapo_ops(group.ops, &channel, firs));
    }
    lines.extend([String::new(), "# 3) main output paths".into()]);
    for (role, ops) in &plan.main_ops {
        let channel = virtual_channel("main", role);
        let body = eqapo_ops(ops, &channel, firs);
        if !body.is_empty() {
            lines.push(format!("Channel: {channel}"));
            lines.extend(body);
        }
    }
    if !plan.lfe_ops.is_empty() {
        lines.extend([
            String::new(),
            "# 4) LFE input".into(),
            "Channel: VLFE".into(),
        ]);
        lines.extend(eqapo_ops(&plan.lfe_ops, "VLFE", firs));
    }

    // Fixed sub filters can move past the final sum only if every contributing
    // support uses the same filters. make_plan establishes this invariant;
    // the check keeps the public writer safe for independently built plans.
    let fixed: Vec<Vec<Op>> = plan
        .subs
        .iter()
        .map(|(role, feed)| {
            let filters = feed
                .ops
                .iter()
                .filter(|op| op.is_fixed())
                .cloned()
                .collect::<Vec<_>>();
            if plan
                .mimo
                .iter()
                .filter(|path| path.support == *role)
                .all(|path| {
                    path.ops
                        .iter()
                        .filter(|op| op.is_fixed())
                        .eq(filters.iter())
                })
            {
                filters
            } else {
                Vec::new()
            }
        })
        .collect();
    struct BassGroup {
        sources: Vec<String>,
        signature: Vec<String>,
        channel: String,
    }
    let mut buses: Vec<BassGroup> = Vec::new();
    for (sub_index, (role, feed)) in plan.subs.iter().enumerate() {
        let prefix = feed
            .ops
            .iter()
            .take_while(|op| matches!(op, Op::Peq { fixed: false, .. }))
            .count();
        let signature = eqapo_ops(&feed.ops[..prefix], "VBASS", firs);
        let mut sources = Vec::new();
        for (index, group) in sends.iter().enumerate() {
            for _ in 0..group.membership[sub_index] {
                sources.push(format!("VX{}", index + 1));
            }
        }
        sources.extend(
            feed.sources
                .iter()
                .filter(|source| source.as_str() == "LFE")
                .map(|_| "VLFE".to_owned()),
        );
        let bus_index = if let Some(index) = buses
            .iter()
            .position(|bus| bus.sources == sources && bus.signature == signature)
        {
            index
        } else {
            let channel = format!("VBASS{}", buses.len() + 1);
            lines.extend([
                String::new(),
                format!(
                    "Copy: {channel}={}",
                    if sources.is_empty() {
                        "0.0".to_owned()
                    } else {
                        sources.join("+")
                    }
                ),
                format!("Channel: {channel}"),
            ]);
            lines.extend(eqapo_ops(&feed.ops[..prefix], &channel, firs));
            buses.push(BassGroup {
                sources,
                signature,
                channel,
            });
            buses.len() - 1
        };
        let channel = virtual_channel("sub", role);
        let factor = f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0);
        // Keep the old six-place linear gain exactly, moving its application
        // after the common bass sum and PEQ.
        lines.extend([
            format!(
                "Copy: {channel}={}*{}",
                format_fixed(factor, 6, false),
                buses[bus_index].channel
            ),
            format!("Channel: {channel}"),
        ]);
        let ops = feed.ops[prefix..]
            .iter()
            .filter(|op| fixed[sub_index].is_empty() || !op.is_fixed())
            .cloned()
            .collect::<Vec<_>>();
        lines.extend(eqapo_ops(&ops, &channel, firs));
    }

    // The export graph's support groups key on output, family and exact low
    // edge. This writer renders the shared HP (and an IIR path's Fsiso LP) at
    // two decimals and shares one VG channel among the paths of one support
    // whose rendered filters coincide, which only joins graph groups. Its
    // entry points take a FilterPlan, which may be built by hand, so it
    // buckets the plan's MIMO feeds by that rendered text itself; for a plan
    // from `ExportGraph::build` the buckets are the graph's groups joined by
    // rendered text.
    struct SupportGroup {
        support: String,
        common: Vec<Op>,
        signature: Vec<String>,
        members: Vec<usize>,
    }
    let mut groups: Vec<SupportGroup> = Vec::new();
    for (index, path) in plan.mimo.iter().enumerate() {
        let common = path
            .ops
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    Op::Hp { fixed: false, .. } | Op::Lp { fixed: false, .. }
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let signature = eqapo_ops(&common, "VG", firs);
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.support == path.support && group.signature == signature)
        {
            group.members.push(index);
        } else {
            groups.push(SupportGroup {
                support: path.support.clone(),
                common,
                signature,
                members: vec![index],
            });
        }
    }
    for (group_index, group) in groups.iter().enumerate() {
        // Equalizer APO receives millisecond delays rounded to three places.
        // Share an identical rendered delay without assuming the host's
        // ms-to-sample tie rule or rounding two split pieces independently.
        let delays = |index: usize| {
            plan.mimo[index]
                .ops
                .iter()
                .filter(|op| matches!(op, Op::Delay { fixed: false, .. }))
                .cloned()
                .collect::<Vec<_>>()
        };
        let common_delays = delays(group.members[0]);
        let delay_key = eqapo_ops(&common_delays, "VG", firs);
        let share_delay = group
            .members
            .iter()
            .all(|&index| eqapo_ops(&delays(index), "VG", firs) == delay_key);
        for &index in &group.members {
            let path = &plan.mimo[index];
            let channel = path_channel(index);
            let move_fixed = plan
                .subs
                .iter()
                .position(|(role, _)| *role == path.support)
                .is_some_and(|sub_index| !fixed[sub_index].is_empty());
            let private = path
                .ops
                .iter()
                .filter(|op| {
                    !(matches!(
                        op,
                        Op::Hp { fixed: false, .. } | Op::Lp { fixed: false, .. }
                    ) || (share_delay && matches!(op, Op::Delay { fixed: false, .. }))
                        || (move_fixed && op.is_fixed()))
                })
                .collect::<Vec<_>>();
            lines.extend([
                String::new(),
                format!("# support {} -> {}", path.input, path.support),
                format!("Channel: {channel}"),
            ]);
            lines.extend(eqapo_ops(private, &channel, firs));
        }
        let channel = format!("VG{}", group_index + 1);
        lines.extend([
            format!(
                "Copy: {channel}={}",
                group
                    .members
                    .iter()
                    .map(|&index| path_channel(index))
                    .collect::<Vec<_>>()
                    .join("+")
            ),
            format!("Channel: {channel}"),
        ]);
        if share_delay {
            lines.extend(eqapo_ops(&common_delays, &channel, firs));
        }
        lines.extend(eqapo_ops(&group.common, &channel, firs));
    }
    // Fold support sums into the ordinary output channels before applying the
    // speaker's fixed chain. config.txt then routes one channel per output.
    for role in &plan.outputs {
        let sub_index = plan.subs.iter().position(|(name, _)| name == role);
        let channel = virtual_channel(if sub_index.is_some() { "sub" } else { "main" }, role);
        let additions = groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.support == *role)
            .map(|(index, _)| format!("VG{}", index + 1))
            .collect::<Vec<_>>();
        if !additions.is_empty() {
            lines.push(format!("Copy: {channel}={channel}+{}", additions.join("+")));
        }
        if let Some(index) = sub_index.filter(|&index| !fixed[index].is_empty()) {
            lines.push(format!("Channel: {channel}"));
            lines.extend(eqapo_ops(&fixed[index], &channel, firs));
        }
    }
    lines.extend([String::new(), "Channel: all".into()]);
    lines.join("\n") + "\n"
}

fn guarded(var: &str, count: &str, line: String) -> [String; 3] {
    [
        format!("If: {var} >= 1 && {var} <= {count}"),
        format!("  {line}"),
        "EndIf:".into(),
    ]
}

/// Build the top-level ASCII Equalizer APO `config.txt`.
pub fn eqapo_config(
    plan: &FilterPlan,
    include: &str,
    preamp_db: f64,
    in_num: &[(String, usize)],
    out_num: &[(String, usize)],
    latency_ms: Option<f64>,
) -> String {
    let mut rows = plan
        .inputs
        .iter()
        .map(|role| {
            format!(
                "#   {role:<6} input {:>2}{}",
                map_number(in_num, role),
                if plan.outputs.iter().any(|item| item == role) {
                    format!("   output {:>2}", map_number(out_num, role))
                } else {
                    String::new()
                }
            )
        })
        .collect::<Vec<_>>();
    rows.extend(
        plan.outputs
            .iter()
            .filter(|role| !plan.inputs.iter().any(|item| item == *role))
            .map(|role| {
                format!(
                    "#   {role:<6}            output {:>2}",
                    map_number(out_num, role)
                )
            }),
    );
    let mut lines = vec!["# SECS 2 calibration - Equalizer APO config (generated)".into(),
        "# Copy everything in this folder into the Equalizer APO config folder".into(),
        "# (C:\\Program Files\\EqualizerAPO\\config) - keep the old config.txt somewhere first.".into(),
        "# Device format: 48000 Hz (the FIR wav files are 48 kHz). Speaker setup: enough channels for the numbers below.".into(), "#".into(),
        "# Default channel numbers (1 = first channel of the device, Windows order FL FR FC LFE BL BR SL SR ...):".into()];
    lines.extend(rows);
    if let Some(ms) = latency_ms {
        lines.push(format!("# Latency of the filters: {} ms (video: delay the picture or set the player audio offset).", format_fixed(ms, 1, false)));
    }
    lines.extend([String::new(), "# ==== EDIT ONLY THE CHANNEL NUMBERS BELOW: device channel number of each SECS input / output (0 = not used) ====".into()]);
    // F11: Equalizer APO's expression parser accepts one assignment per Eval.
    // A semicolon-separated list fails before the routing variables are set.
    for role in &plan.inputs {
        lines.push(format!("Eval: in{role} = {}", map_number(in_num, role)));
    }
    for role in &plan.outputs {
        lines.push(format!("Eval: out{role} = {}", map_number(out_num, role)));
    }
    lines.extend(["# ==== do not edit below ====".into(), String::new(), "# Headroom for the largest boost of the correction (avoids clipping).".into(), "Channel: all".into(),
        if preamp_db > 0.0 { format!("Preamp: -{} dB", format_fixed(preamp_db, 1, false)) } else { "Preamp: 0.0 dB".into() }, String::new(),
        "# Inputs: start silent, then copy each device channel that exists (a missing channel stays silent).".into(),
        format!("Copy: {}", plan.inputs.iter().map(|role| format!("{}=0.0", input_channel(role))).collect::<Vec<_>>().join(" "))]);
    for role in &plan.inputs {
        lines.extend(guarded(
            &format!("in{role}"),
            "inputChannelCount",
            format!("Copy: {}=`in{role}`", input_channel(role)),
        ));
    }
    lines.extend([String::new(), format!("Include: {include}"), String::new(), "# Outputs: clear the input channels first (no raw signal leaks), then write each SECS output.".into()]);
    for role in &plan.inputs {
        lines.extend(guarded(
            &format!("in{role}"),
            "outputChannelCount",
            format!("Copy: `in{role}`=0.0"),
        ));
    }
    for role in &plan.outputs {
        let own = if plan.subs.iter().any(|(name, _)| name == role) {
            virtual_channel("sub", role)
        } else {
            virtual_channel("main", role)
        };
        let source = own;
        lines.extend(guarded(
            &format!("out{role}"),
            "outputChannelCount",
            format!("Copy: `out{role}`={source}"),
        ));
    }
    lines.push("Channel: all".into());
    lines.join("\n") + "\n"
}

/// Build the Korean Equalizer APO application guide.
pub fn eqapo_guide(
    plan: &FilterPlan,
    in_num: &[(String, usize)],
    out_num: &[(String, usize)],
    sub_dir: &str,
    mode_label: &str,
    latency_ms: Option<f64>,
    preamp_db: f64,
) -> String {
    let mut lines = vec![format!("SECS 2 보정 → Equalizer APO ({mode_label})"), String::new(), "■ 적용 순서".into(),
        "1. Equalizer APO 를 설치하고 Configurator 에서 쓸 출력 장치(오디오 인터페이스)에 체크한 뒤 재부팅합니다.".into(),
        "2. 윈도우 소리 설정에서 그 장치의 형식을 48000 Hz 로, 스피커 구성을 아래 번호가 다 들어가는 채널 수로 맞춥니다.".into(),
        "   (FIR wav 가 48 kHz 라 샘플레이트가 다르면 FIR 이 빠집니다.)".into(),
        "3. C:\\Program Files\\EqualizerAPO\\config 의 config.txt 를 다른 이름으로 보관합니다(예: config_old.txt).".into(),
        format!("4. 이 폴더 안의 것(config.txt 와 {sub_dir} 폴더)을 그대로 C:\\Program Files\\EqualizerAPO\\config 에 복사합니다."),
        "5. 끝 — EqAPO 는 저장하면 바로 다시 읽습니다. Editor 를 열어 config.txt 에 빨간 줄(오류)이 없는지 봅니다.".into(), String::new(),
        "■ 채널 번호(장치 채널, 1부터 — 윈도우 순서 FL FR FC LFE BL BR SL SR …)".into(), "   역할    입력  출력  윈도우 이름".into()];
    for role in plan.inputs.iter().chain(
        plan.outputs
            .iter()
            .filter(|role| !plan.inputs.iter().any(|item| item == *role)),
    ) {
        let input = in_num
            .iter()
            .find(|(name, _)| name == role)
            .map_or("   -".into(), |(_, n)| format!("{n:4}"));
        let output = out_num
            .iter()
            .find(|(name, _)| name == role)
            .map_or("   -".into(), |(_, n)| format!("{n:4}"));
        lines.push(format!(
            "   {role:<6} {input}  {output}  {}",
            windows_name(role, plan)
        ));
    }
    lines.extend(["   배선이 다르면 config.txt 맨 위 입력·출력 Eval 줄의 숫자만 고칩니다(0 = 안 씀, 장치에 없는 번호는 저절로 건너뜀):".into(),
        "   · 입력: \"Eval: inL = 1\" — 그 소리가 들어오는 장치 채널".into(), "   · 출력: \"Eval: outL = 1\" — 그 스피커가 꽂힌 장치 채널".into(),
        format!("   필터 본체({sub_dir} 폴더 안)는 고치지 않습니다."), String::new(), "■ 알아 둘 것".into(),
        format!("· 머리 여유: 보정의 가장 큰 부스트만큼 Preamp 를 {} dB 낮춰 두었습니다(클리핑 방지). 앰프·인터페이스 볼륨으로 레벨을 다시 맞춥니다.", format_fixed(preamp_db, 1, false)),
        "· 기존 EQ·룸 보정(다른 config 파일, 플레이어 EQ, 장치 드라이버의 베이스 관리·업믹스)은 끕니다 — 겹치면 보정이 틀어집니다.".into()]);
    if let Some(ms) = latency_ms {
        lines.push(format!(
            "· 지연: 필터 {} ms. 영상은 플레이어의 오디오 지연으로 맞춥니다.",
            format_fixed(ms, 1, false)
        ));
    }
    if !plan.lfe_ops.is_empty() {
        lines.push("· LFE 입력은 +10 dB, 120 Hz 로우패스 뒤 모든 서브로 갑니다(돌비 재생 표준). 머리 여유에는 넣지 않았습니다 — 영화에서 서브가 찢어지면 config.txt 의 Preamp 를 더 낮춥니다.".into());
    }
    lines.join("\n") + "\n"
}

fn db(value: f64) -> String {
    let rounded = round_ndigits(value, 2) + 0.0;
    format_fixed(rounded, 2, true)
}
fn comma(value: usize) -> String {
    let text = value.to_string();
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index > 0 && (text.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
fn text_ops(ops: &[Op], firs: &[(String, String)]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut peq = 0;
    for op in ops {
        let tag = if op.is_fixed() { " (고정)" } else { "" };
        match op {
            Op::Peq {
                frequency,
                q,
                gain_db,
                ..
            } => {
                peq += 1;
                lines.push(format!(
                    "  PEQ {peq:<3}  {:>9} Hz   Q {:>7}   {:>7} dB{tag}",
                    format_fixed(*frequency, 2, false),
                    format_fixed(*q, 3, false),
                    db(*gain_db)
                ));
            }
            Op::Fir { name, taps, .. } => lines.push(format!(
                "  FIR      {} ({} 탭, 48 kHz)",
                firs.iter()
                    .find(|(key, _)| key == name)
                    .expect("FIR name")
                    .1,
                comma(taps.len())
            )),
            Op::FirLowRate {
                name,
                taps_low,
                fs_low,
                ..
            } => lines.push(format!(
                "  FIR      {} ({} 탭, 48 kHz)",
                firs.iter()
                    .find(|(key, _)| key == name)
                    .expect("FIR name")
                    .1,
                comma(FirView::LowRate(taps_low, *fs_low).len_48k())
            )),
            Op::Gain { db: value, .. } => lines.push(format!("  게인     {} dB", db(*value))),
            Op::Invert { .. } => lines.push("  극성     반전".into()),
            Op::Hp {
                frequency, kind, ..
            }
            | Op::Lp {
                frequency, kind, ..
            } => lines.push(format!(
                "  {}       {} {}   {:>9} Hz{tag}",
                if matches!(op, Op::Hp { .. }) {
                    "HP"
                } else {
                    "LP"
                },
                &kind.name()[..2],
                &kind.name()[2..],
                format_fixed(*frequency, 2, false)
            )),
            Op::Ap { frequency, q, .. } => lines.push(format!(
                "  올패스   {:>9} Hz   Q {:>7}   (2차)",
                format_fixed(*frequency, 2, false),
                format_fixed(*q, 3, false)
            )),
            Op::Delay { ms, .. } => lines.push(format!(
                "  딜레이   {} ms ({} 샘플 @ 48 kHz)",
                format_fixed(*ms, 3, false),
                round_ties_even_i64(*ms * f64::from(SAMPLE_RATE) / 1000.0)
            )),
        }
    }
    if lines.is_empty() {
        vec!["  (없음)".into()]
    } else {
        lines
    }
}

/// Build the human-readable Korean filter-value text.
pub fn filter_text(
    plan: &FilterPlan,
    firs: &[(String, String)],
    title: &str,
    info: &[String],
) -> String {
    let mut lines = vec![title.into()];
    lines.extend_from_slice(info);
    lines.extend([
        String::new(),
        "필터는 위에서 아래 순서로 겁니다.".into(),
        "· PEQ = 피킹(RBJ 바이쿼드): 중심 주파수 · Q · 게인".into(),
    ]);
    if !plan.send_ops.is_empty() || !plan.subs.is_empty() {
        lines.extend(["· HP · LP: LR 24 = 링크위츠-라일리 24 dB/oct(버터워스 2차 Q 0.7071 두 개), LR 12 = 2차 Q 0.5 하나, LR 48 = 버터워스 4차 두 번, BW = 버터워스".into(), "· 올패스 = 2차 올패스: 중심 주파수 · Q".into()]);
    }
    lines.extend([
        "· FIR = 같은 폴더의 wav(48 kHz, 32비트 float 모노)".into(),
        String::new(),
        "[신호 흐름]".into(),
    ]);
    if !plan.send_ops.is_empty() {
        lines.extend([
            "  입력 → 입력 보정(PEQ → FIR) ─┬─ 메인 경로(게인 → XO PEQ → HP → 딜레이) → 메인 출력"
                .into(),
            "                                └─ 서브 센드(LP) → 서브 버스".into(),
        ]);
    } else {
        lines.push("  채널마다: 입력 → PEQ → FIR → 게인 → 딜레이 → 출력".into());
    }
    if !plan.lfe_ops.is_empty() {
        lines.push("  LFE 입력 → 게인 → LP → 서브 버스".into());
    }
    if !plan.subs.is_empty() {
        lines.push("  서브 버스: 센드 합 × 서브별 게인 · 극성 → 버스 PEQ → 고정 필터 → 올패스 → 딜레이 → 서브 출력".into());
    }
    if !plan.mimo.is_empty() {
        lines.push(
            "  MIMO 서포트: 입력 원신호(입력 보정 전) → 경로 필터 → 서포트 스피커 출력에 더함"
                .into(),
        );
    }
    let rc_only = plan.send_ops.is_empty() && plan.subs.is_empty() && plan.mimo.is_empty();
    for (role, input) in &plan.input_ops {
        lines.push(String::new());
        if rc_only {
            lines.push(format!("== {role} =="));
            let mut ops = input.clone();
            ops.extend_from_slice(find_ops(&plan.main_ops, role).expect("main ops"));
            lines.extend(text_ops(&ops, firs));
        } else {
            lines.extend([format!("== 메인 {role} =="), "입력 보정".into()]);
            lines.extend(text_ops(input, firs));
            lines.push("메인 경로".into());
            lines.extend(text_ops(
                find_ops(&plan.main_ops, role).expect("main ops"),
                firs,
            ));
            if let Some(send) = find_ops(&plan.send_ops, role) {
                lines.push("서브 센드".into());
                lines.extend(text_ops(send, firs));
            }
        }
    }
    if !plan.lfe_ops.is_empty() {
        lines.extend([String::new(), "== LFE 입력 ==".into()]);
        lines.extend(text_ops(&plan.lfe_ops, firs));
    }
    for (role, feed) in &plan.subs {
        let source = if feed.sources.is_empty() {
            "(없음 — 무음)".into()
        } else {
            feed.sources
                .iter()
                .map(|item| {
                    if item == "LFE" {
                        item.clone()
                    } else {
                        format!("{item} 센드")
                    }
                })
                .collect::<Vec<_>>()
                .join(" · ")
        };
        lines.extend([
            String::new(),
            format!("== 서브 {role} =="),
            format!("  입력     {source} 의 합"),
            format!("  게인     {} dB", db(feed.gain_db)),
            format!(
                "  극성     {}",
                if feed.polarity > 0 {
                    "정상"
                } else {
                    "반전"
                }
            ),
        ]);
        lines.extend(text_ops(&feed.ops, firs));
    }
    for (index, path) in plan.mimo.iter().enumerate() {
        lines.extend([
            String::new(),
            format!(
                "== MIMO 서포트 {}: 입력 {} → 스피커 {} 출력에 더함 ==",
                index + 1,
                path.input,
                path.support
            ),
        ]);
        lines.extend(text_ops(&path.ops, firs));
    }
    lines.join("\n") + "\n"
}

type FirNames = Vec<(String, String)>;
type ExportFiles = Vec<(String, Vec<u8>)>;

fn fir_files(plan: &FilterPlan, prefix: &str) -> Result<(FirNames, ExportFiles), FiltersError> {
    let mut names = Vec::new();
    let mut files = Vec::new();
    for (stem, taps) in plan.firs() {
        taps.validate()?;
        let filename = format!("{prefix}{stem}.wav");
        let bytes = wav_bytes(&taps.materialize(), SAMPLE_RATE)
            .map_err(|error| FiltersError::Invalid(format!("{stem}: {error}")))?;
        files.push((filename.clone(), bytes));
        names.push((stem.to_owned(), filename));
    }
    Ok((names, files))
}
fn utf8_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(text.as_bytes());
    bytes
}
fn ensure_safe_component(name: &str) -> Result<String, FiltersError> {
    let safe = safe_name(name);
    if safe == "." || safe == ".." {
        Err(FiltersError::Invalid(format!(
            "unsafe output name {name:?}"
        )))
    } else {
        Ok(safe)
    }
}

/// Write a filter-value text file and its FIR WAV files.
pub fn write_text(
    plan: &FilterPlan,
    out: &Path,
    file_name: &str,
    wav_prefix: &str,
    title: &str,
    info: &[String],
) -> Result<Vec<PathBuf>, FiltersError> {
    write_text_observed(
        plan,
        out,
        file_name,
        wav_prefix,
        title,
        info,
        &|_, _| Ok(()),
    )
}

/// Audit-only staging seam invoked after each complete staged file.
#[doc(hidden)]
pub fn write_text_observed(
    plan: &FilterPlan,
    out: &Path,
    file_name: &str,
    wav_prefix: &str,
    title: &str,
    info: &[String],
    after_stage_write: &dyn Fn(usize, &Path) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>, FiltersError> {
    if file_name.contains(['/', '\\', '\0']) || matches!(file_name, "." | "..") {
        return Err(FiltersError::Invalid(format!(
            "unsafe text filename {file_name:?}"
        )));
    }
    let prefix = format!("{}_", ensure_safe_component(wav_prefix)?);
    validate_plan_delays(plan)?;
    let (firs, wavs) = fir_files(plan, &prefix)?;
    let text = utf8_bom(&filter_text(plan, &firs, title, info));
    let entry = PathBuf::from(file_name);
    let mut bundle = wavs
        .iter()
        .map(|(name, bytes)| (PathBuf::from(name), bytes.clone()))
        .collect::<Vec<_>>();
    bundle.push((entry.clone(), text));
    write_bundle_observed(out, &bundle, &entry, after_stage_write)?;
    let path = out.join(file_name);
    let mut written = vec![path];
    written.extend(wavs.into_iter().map(|(name, _)| out.join(name)));
    Ok(written)
}

/// Write a complete Equalizer APO folder.
pub fn write_eqapo(
    plan: &FilterPlan,
    out: &Path,
    name: &str,
    mode_label: &str,
    note: &str,
    latency_ms: Option<f64>,
) -> Result<Vec<PathBuf>, FiltersError> {
    write_eqapo_observed(
        plan,
        out,
        name,
        mode_label,
        note,
        latency_ms,
        &|_, _| Ok(()),
    )
}

/// Audit-only staging seam invoked after each complete staged file.
#[doc(hidden)]
pub fn write_eqapo_observed(
    plan: &FilterPlan,
    out: &Path,
    name: &str,
    mode_label: &str,
    note: &str,
    latency_ms: Option<f64>,
    after_stage_write: &dyn Fn(usize, &Path) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>, FiltersError> {
    let safe = ensure_safe_component(name)?;
    validate_plan_delays(plan)?;
    let sub = out.join(&safe);
    let (firs, wavs) = fir_files(plan, &format!("{safe}_"))?;
    let body = eqapo_filters(plan, &firs, note).into_bytes();
    let (inputs, outputs) = windows_channels(plan);
    let preamp = (headroom_db(plan)? * 2.0).ceil() / 2.0;
    let config = eqapo_config(
        plan,
        &format!("{safe}\\{safe}.txt"),
        preamp,
        &inputs,
        &outputs,
        latency_ms,
    )
    .into_bytes();
    let guide = utf8_bom(&eqapo_guide(
        plan, &inputs, &outputs, &safe, mode_label, latency_ms, preamp,
    ));
    let body_relative = PathBuf::from(&safe).join(format!("{safe}.txt"));
    let guide_relative = PathBuf::from(EQAPO_GUIDE);
    let config_relative = PathBuf::from("config.txt");
    let mut bundle = wavs
        .iter()
        .map(|(filename, bytes)| (PathBuf::from(&safe).join(filename), bytes.clone()))
        .collect::<Vec<_>>();
    bundle.extend([
        (body_relative.clone(), body),
        (guide_relative.clone(), guide),
        (config_relative.clone(), config),
    ]);
    write_bundle_observed(out, &bundle, &config_relative, after_stage_write)?;
    let config_path = out.join(&config_relative);
    let body_path = out.join(&body_relative);
    let guide_path = out.join(&guide_relative);
    let mut written = vec![config_path, body_path, guide_path];
    written.extend(wavs.into_iter().map(|(name, _)| sub.join(name)));
    Ok(written)
}

/// Calibration-level filter-value export wrapper.
pub fn export_filter_text(
    run: &FilterRun<'_>,
    folder: &Path,
) -> Result<Vec<PathBuf>, FiltersError> {
    if let Some(block) = filter_export_block(
        ExportKind::FilterText,
        !run.chain.subs.is_empty(),
        run.mimo.is_some(),
    ) {
        return Err(FiltersError::Scope(block));
    }
    validate_run(run)?;
    let graph = ExportGraph::build(run.chain, run.mimo, LfeChoice::DOLBY)?;
    let mode = run.mode;
    let info = vec![
        format!(
            "만든 때 {} · 전체 지연 {} ms · {}",
            run.stamp,
            format_fixed(run.latency_ms, 1, false),
            run.rc_pieces.join(" · ")
        ),
        "샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)".into(),
    ];
    write_text(
        graph.plan(),
        folder,
        &format!("SECS 필터 {}.txt", run.label),
        &format!("secs_{mode}"),
        &format!("SECS 2 보정 필터 — {}", run.label),
        &info,
    )
}

/// Calibration-level Equalizer APO export wrapper.
pub fn export_eqapo(run: &FilterRun<'_>, folder: &Path) -> Result<Vec<PathBuf>, FiltersError> {
    if let Some(block) = filter_export_block(
        ExportKind::EqualizerApo,
        !run.chain.subs.is_empty(),
        run.mimo.is_some(),
    ) {
        return Err(FiltersError::Scope(block));
    }
    validate_run(run)?;
    let graph = ExportGraph::build(run.chain, run.mimo, LfeChoice::DOLBY)?;
    let mode = run.mode;
    write_eqapo(
        graph.plan(),
        folder,
        &format!("secs_{mode}"),
        run.label,
        run.note,
        Some(run.latency_ms),
    )
}

#[derive(serde::Serialize)]
struct ConvolutionRouting<'a> {
    schema: u32,
    sample_rate: u32,
    length: usize,
    format: &'static str,
    headroom_db: f64,
    inputs: &'a [String],
    outputs: &'a [String],
    paths: Vec<ConvolutionRoute>,
}

#[derive(serde::Serialize)]
struct ConvolutionRoute {
    input: String,
    output: String,
    wav: String,
    csv: String,
    peak_gain_db: f64,
    delay_ms: f64,
}

fn convolution_grid() -> Vec<f64> {
    let mut frequencies = Vec::new();
    let mut index = 0_u32;
    loop {
        let frequency = 10.0 * 2_f64.powf(f64::from(index) / 48.0);
        if frequency >= 24_000.0 {
            break;
        }
        frequencies.push(frequency);
        index += 1;
    }
    frequencies.push(24_000.0);
    frequencies
}

fn exact_dtft(samples: &[f64], frequency: f64) -> Complex64 {
    let step = Complex64::from_polar(
        1.0,
        -2.0 * std::f64::consts::PI * frequency / f64::from(SAMPLE_RATE),
    );
    let mut phase = Complex64::new(1.0, 0.0);
    let mut response = Complex64::new(0.0, 0.0);
    for &sample in samples {
        response += phase * sample;
        phase *= step;
    }
    response
}

fn convolution_csv(samples: &[f64], frequencies: &[f64]) -> String {
    let mut text = String::from("frequency_hz,magnitude_db,phase_deg\n");
    for &frequency in frequencies {
        let response = exact_dtft(samples, frequency);
        let magnitude = response.norm();
        let magnitude_db = if magnitude == 0.0 {
            -200.0
        } else {
            20.0 * magnitude.log10()
        };
        let mut phase = if magnitude == 0.0 {
            0.0
        } else {
            response.arg().to_degrees()
        };
        if phase <= -180.0 {
            phase += 360.0;
        }
        text.push_str(&format!(
            "{frequency},{magnitude_db:.6},{:.3}\n",
            phase + 0.0
        ));
    }
    text
}

fn convolution_guide(length: usize, headroom_db: f64, paths: &[ConvolutionRoute]) -> String {
    let mut lines = vec![
        "SECS 2R 컨볼루션 IR".to_owned(),
        String::new(),
        "이 폴더의 WAV는 보정 필터 전체(FIR, PEQ, 크로스오버, 올패스, 게인, 극성, 딜레이)를 입력과 출력 쌍마다 하나로 합친 임펄스 응답입니다.".to_owned(),
        format!("형식: 48000 Hz, 32비트 float, 모노. 모든 파일의 길이는 {length} 샘플로 같습니다."),
        format!("헤드룸: 클리핑을 막으려고 모든 파일에 {headroom_db:.1} dB 감쇠를 넣었습니다. 컨볼버에서 따로 줄이지 않아도 됩니다."),
        String::new(),
        "거는 법".to_owned(),
        "- 아래 표의 한 줄은 입력 채널 하나를 그 WAV로 컨볼루션해 출력 채널 하나로 보내는 연결입니다.".to_owned(),
        "- 같은 출력으로 가는 줄이 여럿이면 결과를 더하십시오(서브 합산, LFE, MIMO 서포트).".to_owned(),
        "- 입력과 출력의 이름이 같고 그 출력으로 가는 줄이 하나뿐이면 그 WAV 하나만 거십시오.".to_owned(),
        "- 재생 장치와 컨볼버의 샘플레이트를 48000 Hz로 맞추십시오.".to_owned(),
        String::new(),
        "입력 → 출력 | 파일 | 최대 이득 | 지연".to_owned(),
    ];
    lines.extend(paths.iter().map(|path| {
        format!(
            "{} → {} | {} | {:+.1} dB | {:.2} ms",
            path.input, path.output, path.wav, path.peak_gain_db, path.delay_ms
        )
    }));
    lines.extend([
        String::new(),
        "같은 이름의 .csv에는 줄마다 주파수 응답(Hz, dB, 위상)이 있습니다. 기계가 읽는 표는 routing.json입니다.".to_owned(),
    ]);
    lines.join("\r\n") + "\r\n"
}

/// Write complete per-path convolution WAV, CSV, routing and guide files.
pub fn export_convolution(
    run: &FilterRun<'_>,
    folder: &Path,
) -> Result<Vec<PathBuf>, FiltersError> {
    if let Some(block) = filter_export_block(
        ExportKind::Convolution,
        !run.chain.subs.is_empty(),
        run.mimo.is_some(),
    ) {
        return Err(FiltersError::Scope(block));
    }
    validate_run(run)?;
    let graph = ExportGraph::build(run.chain, run.mimo, LfeChoice::DOLBY)?;
    let plan = graph.plan();
    let rendered = render_paths(plan, CONVOLUTION_MAX_LEN)?;
    let headroom_db = (headroom_db(plan)? * 2.0).ceil() / 2.0;
    let attenuation = 10_f64.powf(-headroom_db / 20.0);
    let frequencies = convolution_grid();
    let mut files = Vec::new();
    let mut routes = Vec::with_capacity(rendered.paths.len());
    for path in &rendered.paths {
        let stem = format!("{}_to_{}", path.input, path.output);
        let wav = format!("{stem}.wav");
        let csv = format!("{stem}.csv");
        let written = path
            .response
            .iter()
            .map(|sample| f64::from((*sample * attenuation) as f32))
            .collect::<Vec<_>>();
        let peak = frequencies
            .iter()
            .filter(|&&frequency| (20.0..=20_000.0).contains(&frequency))
            .map(|frequency| exact_dtft(&path.response, *frequency).norm())
            .fold(0.0_f64, f64::max);
        let peak_gain_db = if peak == 0.0 {
            -200.0
        } else {
            20.0 * peak.log10()
        };
        files.push((
            PathBuf::from(&wav),
            wav_bytes(&written, SAMPLE_RATE)
                .map_err(|error| FiltersError::Invalid(format!("{stem}: {error}")))?,
        ));
        files.push((
            PathBuf::from(&csv),
            convolution_csv(&written, &frequencies).into_bytes(),
        ));
        routes.push(ConvolutionRoute {
            input: path.input.clone(),
            output: path.output.clone(),
            wav,
            csv,
            peak_gain_db,
            delay_ms: path.delay_ms,
        });
    }
    let guide = convolution_guide(rendered.length, headroom_db, &routes);
    files.push((PathBuf::from(CONVOLUTION_GUIDE), guide.into_bytes()));
    let routing = ConvolutionRouting {
        schema: 1,
        sample_rate: SAMPLE_RATE,
        length: rendered.length,
        format: "f32",
        headroom_db,
        inputs: &rendered.inputs,
        outputs: &rendered.outputs,
        paths: routes,
    };
    let entry = PathBuf::from("routing.json");
    let mut routing_bytes = serde_json::to_vec_pretty(&routing)
        .map_err(|error| FiltersError::Invalid(format!("routing.json: {error}")))?;
    routing_bytes.push(b'\n');
    files.push((entry.clone(), routing_bytes));
    write_bundle_observed(folder, &files, &entry, &|_, _| Ok(()))?;
    Ok(files
        .into_iter()
        .map(|(relative, _)| folder.join(relative))
        .collect())
}

fn finite(name: &str, value: f64) -> Result<(), FiltersError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(FiltersError::Invalid(format!("non-finite {name}")))
    }
}
/// A delay the writers emit: finite and not negative, by the export graph's
/// rule ([`is_negative_delay`], #92 TD-94).
fn emitted_delay(name: &str, channel: &str, ms: f64) -> Result<(), FiltersError> {
    if ms.is_finite() && !is_negative_delay(ms) {
        Ok(())
    } else {
        Err(FiltersError::Invalid(format!(
            "{name} of {channel:?} must be finite and not negative, got {ms} ms"
        )))
    }
}
/// Plans may be built by hand or revived from a cache, so the writers check
/// every delay again before their first file. A plan from
/// [`ExportGraph::build`] always passes.
fn validate_plan_delays(plan: &FilterPlan) -> Result<(), FiltersError> {
    let channels = plan
        .input_ops
        .iter()
        .chain(&plan.main_ops)
        .chain(&plan.send_ops)
        .map(|(role, ops)| (Cow::Borrowed(role.as_str()), ops.as_slice()))
        .chain(std::iter::once((
            Cow::Borrowed("LFE"),
            plan.lfe_ops.as_slice(),
        )))
        .chain(
            plan.subs
                .iter()
                .map(|(role, feed)| (Cow::Borrowed(role.as_str()), feed.ops.as_slice())),
        )
        .chain(plan.mimo.iter().map(|path| {
            (
                Cow::Owned(format!("{}->{}", path.input, path.support)),
                path.ops.as_slice(),
            )
        }));
    for (channel, ops) in channels {
        for op in ops {
            if let Op::Delay { ms, .. } = op {
                emitted_delay("delay", &channel, *ms)?;
            }
        }
    }
    Ok(())
}
/// A run's mode names the export files.
fn identifier(name: &str, value: &str) -> Result<(), FiltersError> {
    if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        Err(FiltersError::Invalid(format!("unsafe {name} {value:?}")))
    } else {
        Ok(())
    }
}
/// The text and Equalizer APO formats take roles as plain names: letters and
/// digits only, stricter than the export graph's rule (`_` and `-`), which
/// the CamillaDSP writer shares.
fn plain_roles(chain: &SignalChain, mimo: Option<&MimoPaths>) -> Result<(), FiltersError> {
    for main in &chain.mains {
        identifier("main role", &main.role)?;
    }
    for sub in &chain.subs {
        identifier("sub role", &sub.role)?;
    }
    for path in mimo.into_iter().flat_map(|mimo| &mimo.paths) {
        identifier("MIMO input", &path.input)?;
        identifier("MIMO support", &path.support)?;
    }
    Ok(())
}
fn validate_run(run: &FilterRun<'_>) -> Result<(), FiltersError> {
    identifier("mode", run.mode)?;
    plain_roles(run.chain, run.mimo)?;
    finite("latency_ms", run.latency_ms)?;
    if run.label.contains(['\0', '/', '\\']) {
        return Err(FiltersError::Invalid(format!(
            "unsafe label {:?}",
            run.label
        )));
    }
    if run.stamp.contains(['\0', '\n', '\r']) {
        return Err(FiltersError::Invalid(format!(
            "unsafe stamp {:?}",
            run.stamp
        )));
    }
    Ok(())
}

#[cfg(test)]
mod optimization_tests;

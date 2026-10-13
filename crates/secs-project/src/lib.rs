#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! SECS 2 compatible `.secs` project files and `.irs` WAV storage.

use secs_types::role::{RoleKind, SUB_ROLES, role_group, role_kind, role_order};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const PROJECT_VERSION: i64 = 3;
const SERDE_NUMBER_KEY: &str = "$serde_json::private::Number";
pub const FOCUS_STEPS: [i64; 5] = [0, 25, 50, 75, 100];

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("invalid project JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid WAV: {0}")]
    Wav(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Band {
    pub band: String,
    pub out: i64,
    #[serde(default = "plus")]
    pub pol: String,
}
fn plus() -> String {
    "+".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaker {
    pub role: String,
    #[serde(default = "small")]
    pub size: String,
    #[serde(default)]
    pub bands: Vec<Band>,
}
fn small() -> String {
    "small".into()
}

fn python_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' if quote == '\'' => out.push_str("\\'"),
            '"' if quote == '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\x08"),
            '\u{000c}' => out.push_str("\\x0c"),
            ch if ch.is_control() => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push(quote);
    out
}

pub fn default_bands(role: &str, start: i64, biamp: bool) -> Vec<Band> {
    if role_kind(role) == RoleKind::Main && biamp {
        vec![
            Band {
                band: "LF".into(),
                out: start,
                pol: plus(),
            },
            Band {
                band: "HF".into(),
                out: start + 1,
                pol: plus(),
            },
        ]
    } else {
        vec![Band {
            band: if role_kind(role) == RoleKind::Sub {
                "SUB"
            } else {
                "FULL"
            }
            .into(),
            out: start,
            pol: plus(),
        }]
    }
}

const PRESETS: [(&str, &[&str]); 6] = [
    ("2.0", &["L", "R"]),
    ("2.1", &["L", "R", "SW1"]),
    ("2.2", &["L", "R", "SW1", "SW2"]),
    ("5.1", &["L", "R", "C", "SW1", "Ls", "Rs"]),
    (
        "7.1.4",
        &[
            "L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr",
        ],
    ),
    (
        "9.1.6",
        &[
            "L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb", "Lw", "Rw", "Tfl", "Tfr", "Tml", "Tmr",
            "Tbl", "Tbr",
        ],
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerConfig {
    pub speakers: Vec<Speaker>,
    pub device: String,
    pub preset: String,
    pub biamp_default: bool,
}
impl Default for SpeakerConfig {
    fn default() -> Self {
        Self {
            speakers: vec![],
            device: String::new(),
            preset: "custom".into(),
            biamp_default: true,
        }
    }
}
impl SpeakerConfig {
    pub fn from_preset(name: &str, device: &str, biamp: bool) -> Result<Self, ProjectError> {
        let roles = PRESETS
            .iter()
            .find(|x| x.0 == name)
            .ok_or_else(|| ProjectError::Invalid(format!("unknown preset {name:?}")))?
            .1;
        let mut out = 1;
        let mut speakers = vec![];
        for role in roles {
            let bands = default_bands(role, out, biamp);
            out += bands.len() as i64;
            speakers.push(Speaker {
                role: (*role).into(),
                size: if role_kind(role) == RoleKind::Sub {
                    "sub"
                } else {
                    "small"
                }
                .into(),
                bands,
            });
        }
        Ok(Self {
            speakers,
            device: device.into(),
            preset: name.into(),
            biamp_default: biamp,
        })
    }
    pub fn roles(&self) -> Vec<&str> {
        self.speakers.iter().map(|s| s.role.as_str()).collect()
    }
    pub fn sub_count(&self) -> usize {
        self.speakers
            .iter()
            .filter(|s| SUB_ROLES.contains(&s.role.as_str()))
            .count()
    }
    pub fn used_outs(&self) -> BTreeSet<i64> {
        self.speakers
            .iter()
            .flat_map(|s| &s.bands)
            .map(|b| b.out)
            .collect()
    }
    pub fn next_free_out(&self) -> i64 {
        let used = self.used_outs();
        (1..).find(|n| !used.contains(n)).unwrap()
    }
    pub fn groups(&self) -> Vec<(&'static str, Vec<&Speaker>)> {
        let mut out: Vec<(&'static str, Vec<&Speaker>)> = Vec::new();
        for speaker in &self.speakers {
            let group = role_group(&speaker.role);
            if let Some((_, speakers)) = out.iter_mut().find(|(name, _)| *name == group) {
                speakers.push(speaker);
            } else {
                out.push((group, vec![speaker]));
            }
        }
        out
    }
    pub fn set_sub_count(&mut self, n: i64) {
        let n = n.clamp(0, 8) as usize;
        while self.sub_count() < n {
            let role = SUB_ROLES
                .into_iter()
                .find(|r| !self.speakers.iter().any(|s| s.role == *r));
            if let Some(role) = role {
                self.speakers.push(Speaker {
                    role: role.into(),
                    size: "sub".into(),
                    bands: default_bands(role, self.next_free_out(), false),
                });
            } else {
                break;
            }
        }
        while self.sub_count() > n {
            if let Some(i) = self
                .speakers
                .iter()
                .rposition(|s| SUB_ROLES.contains(&s.role.as_str()))
            {
                self.speakers.remove(i);
            }
        }
        self.preset = "custom".into();
    }
    pub fn renumber(&mut self) {
        for (out, band) in (1_i64..).zip(self.speakers.iter_mut().flat_map(|s| &mut s.bands)) {
            band.out = out;
        }
    }
    pub fn validate(&self) -> Vec<String> {
        let mut msgs = vec![];
        let mut roles = BTreeSet::new();
        for role in self.roles() {
            if !roles.insert(role) {
                msgs.push(format!("역할 중복: {role}"));
            }
        }

        let mut outs = BTreeMap::new();
        for speaker in &self.speakers {
            for band in &speaker.bands {
                if band.out < 1 {
                    msgs.push(format!(
                        "{}: 출력 채널 번호가 잘못되었습니다({}, 1 이상 정수)",
                        speaker.role, band.out
                    ));
                    continue;
                }
                if let Some(first_role) = outs.get(&band.out) {
                    msgs.push(format!(
                        "출력 채널 중복: {} ({} ↔ {})",
                        band.out, first_role, speaker.role
                    ));
                } else {
                    outs.insert(band.out, speaker.role.as_str());
                }
            }
        }

        for speaker in &self.speakers {
            let names: Vec<&str> = speaker
                .bands
                .iter()
                .map(|band| band.band.as_str())
                .collect();
            for band in &speaker.bands {
                if !["FULL", "LF", "MF", "HF", "SUB"].contains(&band.band.as_str()) {
                    msgs.push(format!(
                        "{}: 알 수 없는 대역 {}",
                        speaker.role,
                        python_repr(&band.band)
                    ));
                }
                if !["+", "-"].contains(&band.pol.as_str()) {
                    msgs.push(format!(
                        "{}: 극성은 + 또는 - 여야 합니다({})",
                        speaker.role,
                        python_repr(&band.pol)
                    ));
                }
            }
            if names.is_empty() {
                msgs.push(format!("{}: 대역이 없습니다", speaker.role));
            } else if role_kind(&speaker.role) == RoleKind::Sub && names != ["SUB"] {
                msgs.push(format!(
                    "{}: 서브우퍼는 SUB 대역 하나만 가질 수 있습니다({})",
                    speaker.role,
                    names.join("/")
                ));
            } else if role_kind(&speaker.role) != RoleKind::Sub && names.contains(&"SUB") {
                msgs.push(format!(
                    "{}: 메인/탑/하이트 스피커에는 SUB 대역을 쓸 수 없습니다",
                    speaker.role
                ));
            } else if role_kind(&speaker.role) != RoleKind::Sub
                && ![vec!["FULL"], vec!["LF", "HF"], vec!["LF", "MF", "HF"]].contains(&names)
            {
                msgs.push(format!(
                    "{}: 대역 조합 오류 {} (가능: FULL · LF/HF · LF/MF/HF)",
                    speaker.role,
                    names.join("/")
                ));
            }
        }
        msgs
    }
}

pub fn ordered_speakers(speakers: &[Speaker], subs_last: bool) -> Vec<&Speaker> {
    let mut out: Vec<_> = speakers.iter().collect();
    out.sort_by_key(|s| {
        (
            subs_last && role_kind(&s.role) == RoleKind::Sub,
            role_order(&s.role),
        )
    });
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct DspProfile {
    pub profile: String,
    pub fir_taps: i64,
    pub fir_budget: i64,
    pub iir_bands: i64,
    pub support_mode: String,
    pub support_taps: i64,
    pub allpass_bands: i64,
    pub bm_path_delays: i64,
    pub bm_path_polarity: bool,
    pub bm_path_allpass: i64,
    pub path_peq_bands: i64,
    pub path_allpass_bands: i64,
    pub per_output_lp: bool,
    pub fir_taps_max: i64,
    pub fir_taps_auto: Option<bool>,
    pub support_taps_auto: Option<bool>,
    pub name: String,
}
impl Default for DspProfile {
    fn default() -> Self {
        Self {
            profile: "tesira".into(),
            fir_taps: 2048,
            fir_budget: 32768,
            iir_bands: 16,
            support_mode: "iir".into(),
            support_taps: 1024,
            allpass_bands: 16,
            bm_path_delays: 16,
            bm_path_polarity: false,
            bm_path_allpass: 0,
            path_peq_bands: 0,
            path_allpass_bands: 0,
            per_output_lp: false,
            fir_taps_max: 2048,
            fir_taps_auto: Some(true),
            support_taps_auto: Some(true),
            name: String::new(),
        }
    }
}
impl DspProfile {
    pub fn presets() -> BTreeMap<&'static str, Self> {
        let rows = [
            (
                "camilla",
                "PC · CamillaDSP",
                65536,
                65536 * 32,
                64,
                64,
                64,
                true,
                4,
                16,
                8,
                true,
            ),
            (
                "tesira",
                "Biamp Tesira SERVER (DSP-2 ×3)",
                2048,
                51200,
                16,
                16,
                16,
                false,
                0,
                0,
                0,
                false,
            ),
            (
                "qsys",
                "Q-SYS Core 8 Flex / 110f",
                16384,
                65536,
                24,
                16,
                16,
                true,
                2,
                4,
                2,
                true,
            ),
            (
                "minidsp",
                "miniDSP Flex / 2x4 HD",
                2048,
                4096,
                10,
                0,
                0,
                false,
                0,
                0,
                0,
                false,
            ),
            (
                "custom",
                "사용자 정의",
                2048,
                32768,
                16,
                16,
                16,
                false,
                0,
                0,
                0,
                false,
            ),
        ];
        rows.into_iter()
            .map(
                |(key, name, taps, budget, iir, ap, bmd, pol, bmap, ppeq, pap, lp)| {
                    (
                        key,
                        Self {
                            profile: key.into(),
                            fir_taps: taps,
                            fir_taps_max: taps,
                            fir_budget: budget,
                            iir_bands: iir,
                            support_mode: "iir".into(),
                            support_taps: 1024,
                            allpass_bands: ap,
                            bm_path_delays: bmd,
                            bm_path_polarity: pol,
                            bm_path_allpass: bmap,
                            path_peq_bands: ppeq,
                            path_allpass_bands: pap,
                            per_output_lp: lp,
                            fir_taps_auto: Some(true),
                            support_taps_auto: Some(true),
                            name: name.into(),
                        },
                    )
                },
            )
            .collect()
    }
    pub fn allocate_taps(
        &self,
        config: &SpeakerConfig,
        phase: &str,
        path_count: i64,
        min_taps: i64,
    ) -> (i64, i64) {
        let bands = config
            .speakers
            .iter()
            .map(|s| s.bands.len() as i64)
            .sum::<i64>()
            .max(1);
        let chan = if phase == "mixed" { bands } else { 0 };
        let sup = if ["hybrid", "fir"].contains(&self.support_mode.as_str()) {
            path_count
        } else {
            0
        };
        let consumers = chan + sup;
        if consumers <= 0 {
            return (self.fir_taps, self.support_taps);
        }
        let ceiling = self.fir_taps_max.max(min_taps);
        let ceiling_pow = 1_i64 << (63 - ceiling.leading_zeros());
        let target = self.fir_budget / consumers;
        let mut taps = if target >= 1 {
            1_i64 << (63 - target.leading_zeros())
        } else {
            min_taps
        };
        taps = taps.max(min_taps).min(ceiling_pow);
        while taps > min_taps && taps * consumers > self.fir_budget {
            taps /= 2;
        }
        (
            if chan > 0 { taps } else { self.fir_taps },
            if sup > 0 { taps } else { self.support_taps },
        )
    }
    pub fn to_value(&self) -> Value {
        serde_json::json!({"profile":self.profile,"fir_taps":self.fir_taps,"fir_taps_max":self.fir_taps_max,"fir_budget":self.fir_budget,"iir_bands":self.iir_bands,"support_mode":self.support_mode,"support_taps":self.support_taps,"allpass_bands":self.allpass_bands,"bm_path_delays":self.bm_path_delays,"bm_path_polarity":self.bm_path_polarity,"bm_path_allpass":self.bm_path_allpass,"path_peq_bands":self.path_peq_bands,"path_allpass_bands":self.path_allpass_bands,"per_output_lp":self.per_output_lp,"fir_taps_auto":self.fir_taps_auto.unwrap_or(false),"support_taps_auto":self.support_taps_auto.unwrap_or(false),"name":self.name})
    }
    pub fn fir_used(
        &self,
        config: &SpeakerConfig,
        phase: &str,
        path_count: i64,
    ) -> BTreeMap<&'static str, i64> {
        let bands = config
            .speakers
            .iter()
            .map(|s| s.bands.len() as i64)
            .sum::<i64>();
        let chan = if phase == "minphase" {
            0
        } else {
            bands * self.fir_taps
        };
        let sup = if self.support_mode == "iir" {
            0
        } else {
            path_count * self.support_taps
        };
        [("chan", chan), ("sup", sup), ("total", chan + sup)]
            .into_iter()
            .collect()
    }
}

fn python_sort_run(points: &mut [(f64, f64)]) -> usize {
    if points.len() < 2 {
        return points.len();
    }
    let mut n = 1;
    while n < points.len()
        && points[n].0.partial_cmp(&points[n - 1].0) != Some(std::cmp::Ordering::Less)
    {
        n += 1;
    }
    if n == points.len() {
        return n;
    }
    if n > 1 {
        if points[0].0 < points[n - 1].0 {
            return n;
        }
        points[..n].reverse();
    }
    n += 1;
    let mut equal_tail = 0;
    while n < points.len() {
        if points[n].0 < points[n - 1].0 {
            if equal_tail != 0 {
                points[n - equal_tail - 1..n].reverse();
                equal_tail = 0;
            }
        } else if points[n - 1].0 < points[n].0 {
            break;
        } else {
            equal_tail += 1;
        }
        n += 1;
    }
    if equal_tail != 0 {
        points[n - equal_tail - 1..n].reverse();
    }
    points[..n].reverse();
    while n < points.len()
        && points[n].0.partial_cmp(&points[n - 1].0) != Some(std::cmp::Ordering::Less)
    {
        n += 1;
    }
    n
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetCurve {
    pub points: Vec<(f64, f64)>,
}
impl Default for TargetCurve {
    fn default() -> Self {
        Self::new(vec![
            (25., 2.),
            (80., 2.),
            (200., 0.),
            (1000., 0.),
            (16000., -2.),
        ])
    }
}
impl TargetCurve {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        let mut s = Self { points };
        s.sort();
        s
    }
    pub fn sort(&mut self) {
        // Target curves are shorter than CPython's TimSort minimum run. Match
        // its natural-run detection before extending that run by binary sort;
        // this comparison order matters for NaN, while +0/-0 remain stable.
        let sorted = python_sort_run(&mut self.points);
        for index in sorted..self.points.len() {
            let point = self.points[index];
            let mut lo = 0;
            let mut hi = index;
            while lo < hi {
                let mid = (lo + hi) / 2;
                if point.0 < self.points[mid].0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            self.points[lo..=index].rotate_right(1);
        }
    }
    pub fn at(&self, f: f64) -> f64 {
        if self.points.is_empty() {
            return 0.;
        }
        if f <= self.points[0].0 {
            return self.points[0].1;
        }
        if f >= self.points[self.points.len() - 1].0 {
            return self.points[self.points.len() - 1].1;
        }
        for w in self.points.windows(2) {
            let ((fa, da), (fb, db)) = (w[0], w[1]);
            if fa <= f && f <= fb {
                let t = (f.ln() - fa.ln()) / (fb.ln() - fa.ln());
                let u = t * t * (3. - 2. * t);
                return da + (db - da) * u;
            }
        }
        0.
    }
    pub fn sample(&self, freqs: &[f64]) -> Vec<f64> {
        freqs.iter().map(|&f| self.at(f)).collect()
    }
    pub fn presets() -> BTreeMap<&'static str, Self> {
        [
            (
                "flat",
                vec![(25., 0.), (200., 0.), (1000., 0.), (16000., 0.)],
            ),
            (
                "harman",
                vec![(25., 6.), (60., 5.), (120., 1.), (1000., 0.), (16000., -3.)],
            ),
            (
                "cinema",
                vec![
                    (25., 4.),
                    (80., 3.),
                    (200., 0.),
                    (1000., -1.),
                    (16000., -5.),
                ],
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k, Self::new(v)))
        .collect()
    }
}

fn python_float_coerce(value: &Value, marker_key: Option<&str>) -> Option<f64> {
    match marker_literal(value, marker_key) {
        Some("NaN") => Some(f64::NAN),
        Some("Infinity") => Some(f64::INFINITY),
        Some("-Infinity") => Some(f64::NEG_INFINITY),
        _ => match value {
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Number(value) => value.to_string().parse().ok(),
            Value::String(value) => value.parse().ok(),
            _ => None,
        },
    }
}

fn snap_focus_with_marker(value: &Value, marker_key: Option<&str>) -> i64 {
    let x = python_float_coerce(value, marker_key).unwrap_or(50.0);
    let mut best = FOCUS_STEPS[0];
    let mut best_distance = (best as f64 - x).abs();
    for step in FOCUS_STEPS.into_iter().skip(1) {
        let distance = (step as f64 - x).abs();
        if distance < best_distance {
            best = step;
            best_distance = distance;
        }
    }
    best
}

pub fn snap_focus(value: &Value) -> i64 {
    snap_focus_with_marker(value, None)
}
pub fn rename_keys(value: &Value, table: &[(&str, &str)]) -> Value {
    let Some(map) = value.as_object() else {
        return value.clone();
    };
    if !table.iter().any(|(old, _)| map.contains_key(*old)) {
        return value.clone();
    }
    let mut out = Map::new();
    for (k, v) in map {
        if let Some((_, new)) = table.iter().find(|(old, _)| *old == k) {
            if !map.contains_key(*new) {
                out.insert((*new).into(), v.clone());
            }
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}
pub fn migrate_group_keys(v: &Value) -> Value {
    rename_keys(v, &[("하이트", "탑")])
}
pub fn migrate_calibration_keys(v: &Value) -> Value {
    let mut out = rename_keys(
        v,
        &[
            ("art_on", "mimo_on"),
            ("art_low", "mimo_low"),
            ("art_max_supports", "mimo_max_supports"),
        ],
    );
    if let Some(manual) = out.get("subs_manual").and_then(Value::as_object) {
        let m = manual
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    rename_keys(v, &[("art_support", "mimo_support")]),
                )
            })
            .collect();
        out.as_object_mut()
            .unwrap()
            .insert("subs_manual".into(), Value::Object(m));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ValuePathSegment {
    Key(String),
    Index(usize),
}

type ValuePath = Vec<ValuePathSegment>;

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub version: i64,
    pub name: String,
    pub config: SpeakerConfig,
    pub dsp: DspProfile,
    pub target: TargetCurve,
    pub curtain: (f64, f64),
    pub xo: Map<String, Value>,
    pub bc: Value,
    pub mso: Value,
    pub measurements: Value,
    pub verify: Value,
    pub tesira: Value,
    pub snapshots: Value,
    pub extra: Map<String, Value>,
    nonfinite_key: Option<String>,
    nonfinite_paths: BTreeSet<ValuePath>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            version: 3,
            name: String::new(),
            config: SpeakerConfig::default(),
            dsp: DspProfile::default(),
            target: TargetCurve::default(),
            curtain: (24., 300.),
            xo: Map::new(),
            bc: serde_json::json!({"mode":"full","phase":"minphase","target_preset":"flat"}),
            mso: serde_json::json!({"lo":20,"hi":150,"hp":25,"maxGain":-6,"maxDelay":20,"maxPer":4,"focus":50,"active":{},"level":{},"decay":1}),
            measurements: Value::Array(vec![]),
            verify: serde_json::json!({"A":null,"B":null}),
            tesira: serde_json::json!({"ip":"","method":"ttp","slot":1}),
            snapshots: Value::Array(vec![]),
            extra: Map::new(),
            nonfinite_key: Some("__secs_project_nonfinite_0__".into()),
            nonfinite_paths: BTreeSet::new(),
        }
    }
}

fn object(v: Option<&Value>, field: &str) -> Result<Map<String, Value>, ProjectError> {
    match v {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(value)) => Ok(value.clone()),
        Some(_) => Err(ProjectError::Invalid(format!("{field} must be an object"))),
    }
}
fn integer(v: Option<&Value>, default: i64, field: &str) -> Result<i64, ProjectError> {
    match v {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| ProjectError::Invalid(format!("{field} must be an integer"))),
    }
}
fn string(v: Option<&Value>, default: &str, field: &str) -> Result<String, ProjectError> {
    match v {
        None => Ok(default.to_owned()),
        Some(value) => value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ProjectError::Invalid(format!("{field} must be a string"))),
    }
}
fn boolv(v: Option<&Value>, default: bool, field: &str) -> Result<bool, ProjectError> {
    match v {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| ProjectError::Invalid(format!("{field} must be a boolean"))),
    }
}
fn optional_bool(v: Option<&Value>, field: &str) -> Result<Option<bool>, ProjectError> {
    match v {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| ProjectError::Invalid(format!("{field} must be a boolean"))),
    }
}
fn array<'a>(v: Option<&'a Value>, field: &str) -> Result<Option<&'a Vec<Value>>, ProjectError> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(value)) => Ok(Some(value)),
        Some(_) => Err(ProjectError::Invalid(format!("{field} must be an array"))),
    }
}
fn marker_value(key: &str, literal: &str) -> Value {
    Value::Object(Map::from_iter([(
        key.to_owned(),
        Value::String(literal.to_owned()),
    )]))
}

fn marker_literal<'a>(value: &'a Value, key: Option<&str>) -> Option<&'a str> {
    let key = key?;
    let object = value.as_object()?;
    if object.len() == 1 {
        object.get(key)?.as_str()
    } else {
        None
    }
}

fn collect_marker_paths(
    value: &Value,
    marker_key: &str,
    path: &mut ValuePath,
    paths: &mut BTreeSet<ValuePath>,
) {
    if matches!(
        marker_literal(value, Some(marker_key)),
        Some("NaN" | "Infinity" | "-Infinity")
    ) {
        paths.insert(path.clone());
        return;
    }
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(ValuePathSegment::Index(index));
                collect_marker_paths(value, marker_key, path, paths);
                path.pop();
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                path.push(ValuePathSegment::Key(key.clone()));
                collect_marker_paths(value, marker_key, path, paths);
                path.pop();
            }
        }
        _ => {}
    }
}

fn f64v(v: &Value, marker_key: Option<&str>) -> Result<f64, ProjectError> {
    match marker_literal(v, marker_key) {
        Some("NaN") => Ok(f64::NAN),
        Some("Infinity") => Ok(f64::INFINITY),
        Some("-Infinity") => Ok(f64::NEG_INFINITY),
        _ => {
            let number = v
                .as_number()
                .ok_or_else(|| ProjectError::Invalid("expected number".into()))?;
            let raw = number.to_string();
            let value = raw
                .parse::<f64>()
                .map_err(|_| ProjectError::Invalid("expected number".into()))?;
            if !raw.contains(['.', 'e', 'E']) {
                if !value.is_finite() {
                    return Err(ProjectError::Invalid(
                        "integer is too large for a Python float".into(),
                    ));
                }
                if value == 0.0 {
                    return Ok(0.0);
                }
            }
            Ok(value)
        }
    }
}

fn float_value(value: f64, marker_key: Option<&str>) -> Value {
    if value.is_finite() {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .expect("a finite f64 is always a JSON number")
    } else if let Some(key) = marker_key {
        marker_value(
            key,
            if value.is_nan() {
                "NaN"
            } else if value.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            },
        )
    } else {
        Value::Null
    }
}

impl Project {
    pub fn from_preset(name: &str) -> Result<Self, ProjectError> {
        Ok(Self {
            config: SpeakerConfig::from_preset(name, "", false)?,
            ..Self::default()
        })
    }
    pub fn from_value(v: Value) -> Result<Self, ProjectError> {
        let marker_key = unique_marker_key_for_value(&v);
        Self::from_value_with_marker(v, Some(marker_key))
    }

    fn from_value_with_marker(v: Value, marker_key: Option<String>) -> Result<Self, ProjectError> {
        let d = v
            .as_object()
            .ok_or_else(|| ProjectError::Invalid("project root must be an object".into()))?;
        let mut p = Self {
            version: integer(d.get("version"), 1, "version")?,
            name: string(d.get("name"), "", "name")?,
            nonfinite_key: marker_key,
            ..Self::default()
        };
        let c = object(d.get("config"), "config")?;
        p.config = SpeakerConfig {
            speakers: array(c.get("speakers"), "config.speakers")?
                .map(|xs| {
                    xs.iter()
                        .map(|x| {
                            let o = x.as_object().ok_or_else(|| {
                                ProjectError::Invalid("speaker must be an object".into())
                            })?;
                            let bands = array(o.get("bands"), "speaker.bands")?
                                .map(Vec::as_slice)
                                .unwrap_or_default()
                                .iter()
                                .map(|b| {
                                    let b = b.as_object().ok_or_else(|| {
                                        ProjectError::Invalid("band must be an object".into())
                                    })?;
                                    Ok(Band {
                                        band: string(b.get("band"), "", "band.band")?,
                                        out: integer(b.get("out"), 0, "band.out")?,
                                        pol: string(b.get("pol"), "+", "band.pol")?,
                                    })
                                })
                                .collect::<Result<Vec<_>, ProjectError>>()?;
                            Ok(Speaker {
                                role: string(o.get("role"), "", "speaker.role")?,
                                size: string(o.get("size"), "small", "speaker.size")?,
                                bands,
                            })
                        })
                        .collect::<Result<Vec<_>, ProjectError>>()
                })
                .transpose()?
                .unwrap_or_default(),
            device: string(c.get("device"), "", "config.device")?,
            preset: string(c.get("preset"), "custom", "config.preset")?,
            biamp_default: boolv(c.get("biamp_default"), true, "config.biamp_default")?,
        };
        let x = object(d.get("dsp"), "dsp")?;
        let profile = string(x.get("profile"), "tesira", "dsp.profile")?;
        let preset = DspProfile::presets().remove(profile.as_str());
        let taps = integer(x.get("fir_taps"), 2048, "dsp.fir_taps")?;
        let fallback = preset.as_ref().cloned().unwrap_or_default();
        p.dsp = DspProfile {
            profile,
            fir_taps: taps,
            fir_taps_max: integer(
                x.get("fir_taps_max"),
                preset.as_ref().map_or(taps, |value| value.fir_taps_max),
                "dsp.fir_taps_max",
            )?,
            fir_budget: integer(x.get("fir_budget"), 32768, "dsp.fir_budget")?,
            iir_bands: integer(x.get("iir_bands"), 16, "dsp.iir_bands")?,
            support_mode: string(x.get("support_mode"), "iir", "dsp.support_mode")?,
            support_taps: integer(x.get("support_taps"), 1024, "dsp.support_taps")?,
            allpass_bands: integer(
                x.get("allpass_bands"),
                fallback.allpass_bands,
                "dsp.allpass_bands",
            )?,
            bm_path_delays: integer(
                x.get("bm_path_delays"),
                fallback.bm_path_delays,
                "dsp.bm_path_delays",
            )?,
            fir_taps_auto: optional_bool(x.get("fir_taps_auto"), "dsp.fir_taps_auto")?,
            support_taps_auto: optional_bool(x.get("support_taps_auto"), "dsp.support_taps_auto")?,
            bm_path_polarity: boolv(
                x.get("bm_path_polarity"),
                fallback.bm_path_polarity,
                "dsp.bm_path_polarity",
            )?,
            bm_path_allpass: integer(
                x.get("bm_path_allpass"),
                fallback.bm_path_allpass,
                "dsp.bm_path_allpass",
            )?,
            path_peq_bands: integer(
                x.get("path_peq_bands"),
                fallback.path_peq_bands,
                "dsp.path_peq_bands",
            )?,
            path_allpass_bands: integer(
                x.get("path_allpass_bands"),
                fallback.path_allpass_bands,
                "dsp.path_allpass_bands",
            )?,
            per_output_lp: boolv(
                x.get("per_output_lp"),
                fallback.per_output_lp,
                "dsp.per_output_lp",
            )?,
            name: string(x.get("name"), "", "dsp.name")?,
        };
        let points = match d.get("target") {
            None | Some(Value::Null) => None,
            Some(Value::Array(points)) => Some(points),
            Some(Value::Object(target)) => array(target.get("points"), "target.points")?,
            Some(_) => {
                return Err(ProjectError::Invalid(
                    "target must be an object or array".into(),
                ));
            }
        };
        p.target = TargetCurve::new(
            points
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|q| {
                    let pair = q.as_array().ok_or_else(|| {
                        ProjectError::Invalid("target point must be a pair".into())
                    })?;
                    if pair.len() != 2 {
                        return Err(ProjectError::Invalid(
                            "target point must contain two numbers".into(),
                        ));
                    }
                    Ok((
                        f64v(&pair[0], p.nonfinite_key.as_deref())?,
                        f64v(&pair[1], p.nonfinite_key.as_deref())?,
                    ))
                })
                .collect::<Result<_, ProjectError>>()?,
        );
        if let Some(c) = array(d.get("curtain"), "curtain")? {
            if c.len() < 2 {
                return Err(ProjectError::Invalid("curtain needs two numbers".into()));
            }
            p.curtain = (
                f64v(&c[0], p.nonfinite_key.as_deref())?,
                f64v(&c[1], p.nonfinite_key.as_deref())?,
            );
        }
        p.xo = object(d.get("xo"), "xo")?;
        for value in p.xo.values_mut() {
            *value = float_value(
                f64v(value, p.nonfinite_key.as_deref())?,
                p.nonfinite_key.as_deref(),
            );
        }
        for (field, default) in [
            ("bc", &mut p.bc),
            ("mso", &mut p.mso),
            ("verify", &mut p.verify),
            ("tesira", &mut p.tesira),
        ] {
            if let Some(value) = d.get(field) {
                if value.is_null() {
                    *default = Value::Object(Map::new());
                } else if value.is_object() {
                    *default = value.clone();
                } else {
                    return Err(ProjectError::Invalid(format!("{field} must be an object")));
                }
            }
        }
        for (field, default) in [
            ("measurements", &mut p.measurements),
            ("snapshots", &mut p.snapshots),
        ] {
            if let Some(value) = d.get(field) {
                if value.is_null() {
                    *default = Value::Array(vec![]);
                } else if value.is_array() {
                    *default = value.clone();
                } else {
                    return Err(ProjectError::Invalid(format!("{field} must be an array")));
                }
            }
        }
        p.extra = object(d.get("extra"), "extra")?;
        for (k, v) in d {
            if ![
                "version",
                "name",
                "config",
                "dsp",
                "target",
                "curtain",
                "xo",
                "bc",
                "mso",
                "measurements",
                "verify",
                "tesira",
                "snapshots",
                "extra",
            ]
            .contains(&k.as_str())
            {
                p.extra.insert(k.clone(), v.clone());
            }
        }
        p.migrate_legacy()?;
        if let Some(marker_key) = p.nonfinite_key.clone() {
            let value = p.to_value_with_marker(&marker_key);
            collect_marker_paths(&value, &marker_key, &mut Vec::new(), &mut p.nonfinite_paths);
        }
        Ok(p)
    }
    fn migrate_legacy(&mut self) -> Result<(), ProjectError> {
        if self.bc.get("phase").and_then(Value::as_str) == Some("fir") {
            self.bc
                .as_object_mut()
                .ok_or_else(|| ProjectError::Invalid("bc must be object".into()))?
                .insert("phase".into(), Value::String("minphase".into()));
            self.extra
                .insert("_migrated_fir_phase".into(), Value::Bool(true));
        }
        if self.dsp.fir_taps_auto.is_none() || self.dsp.support_taps_auto.is_none() {
            let phase = self
                .bc
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("minphase");
            let (a, b) =
                self.dsp
                    .allocate_taps(&self.config, phase, self.bass_path_count() as i64, 256);
            if self.dsp.fir_taps_auto.is_none() {
                self.dsp.fir_taps_auto = Some(self.dsp.fir_taps == a);
            }
            if self.dsp.support_taps_auto.is_none() {
                self.dsp.support_taps_auto = Some(self.dsp.support_taps == b);
            }
        }
        if let Some(m) = self.mso.as_object_mut() {
            let focus = snap_focus_with_marker(
                m.get("focus").unwrap_or(&Value::from(50)),
                self.nonfinite_key.as_deref(),
            );
            m.insert("focus".into(), Value::from(focus));
        }
        if self.version < 2 {
            self.xo = migrate_group_keys(&Value::Object(self.xo.clone()))
                .as_object()
                .cloned()
                .unwrap();
            if let Some(c) = self
                .extra
                .get_mut("calibration")
                .and_then(Value::as_object_mut)
            {
                for key in ["xo", "xo_rec"] {
                    if let Some(v) = c.get(key).cloned() {
                        c.insert(key.into(), migrate_group_keys(&v));
                    }
                }
            }
        }
        if self.version < 3
            && let Some(c) = self.extra.get("calibration").cloned()
        {
            self.extra
                .insert("calibration".into(), migrate_calibration_keys(&c));
        }
        if self.version < PROJECT_VERSION {
            self.version = PROJECT_VERSION;
        }
        Ok(())
    }
    pub fn bass_path_count(&self) -> usize {
        self.extra
            .get("bass")
            .and_then(|b| b.get("paths"))
            .map_or(0, |paths| match paths {
                Value::Object(paths) => paths.len(),
                Value::Array(paths) => paths.len(),
                _ => 0,
            })
    }
    pub fn apply_recommended_taps(&mut self) -> bool {
        let phase = match self.bc.get("phase").and_then(Value::as_str) {
            Some("mixed") => "mixed",
            _ => "minphase",
        };
        let (a, b) =
            self.dsp
                .allocate_taps(&self.config, phase, self.bass_path_count() as i64, 256);
        let mut changed = false;
        if self.dsp.fir_taps_auto == Some(true) && self.dsp.fir_taps != a {
            self.dsp.fir_taps = a;
            changed = true;
        }
        if self.dsp.support_taps_auto == Some(true) && self.dsp.support_taps != b {
            self.dsp.support_taps = b;
            changed = true;
        }
        changed
    }
    pub fn validate(&self) -> Vec<String> {
        self.config.validate()
    }
    fn unmarked_value(&self) -> Value {
        self.to_value_with_marker("")
    }

    fn to_value_with_marker(&self, marker_key: &str) -> Value {
        let marker_key = (!marker_key.is_empty()).then_some(marker_key);
        let mut d = Map::new();
        for (k, v) in &self.extra {
            if ![
                "version",
                "name",
                "config",
                "dsp",
                "target",
                "curtain",
                "xo",
                "bc",
                "mso",
                "measurements",
                "verify",
                "tesira",
                "snapshots",
                "extra",
            ]
            .contains(&k.as_str())
            {
                d.insert(k.clone(), v.clone());
            }
        }
        let bands = |b: &Band| serde_json::json!({"band":b.band,"out":b.out,"pol":b.pol});
        let speakers:Vec<Value>=self.config.speakers.iter().map(|s|serde_json::json!({"role":s.role,"size":s.size,"bands":s.bands.iter().map(&bands).collect::<Vec<_>>()})).collect();
        d.insert("version".into(), self.version.into());
        d.insert("name".into(), self.name.clone().into());
        d.insert("config".into(),serde_json::json!({"speakers":speakers,"device":self.config.device,"preset":self.config.preset,"biamp_default":self.config.biamp_default}));
        d.insert("dsp".into(), self.dsp.to_value());
        let points: Vec<Value> = self
            .target
            .points
            .iter()
            .map(|(a, b)| {
                Value::Array(vec![
                    float_value(*a, marker_key),
                    float_value(*b, marker_key),
                ])
            })
            .collect();
        d.insert("target".into(), serde_json::json!({"points":points}));
        d.insert(
            "curtain".into(),
            Value::Array(vec![
                float_value(self.curtain.0, marker_key),
                float_value(self.curtain.1, marker_key),
            ]),
        );
        d.insert(
            "xo".into(),
            Value::Object(
                self.xo
                    .iter()
                    .map(|(key, value)| {
                        let path = vec![
                            ValuePathSegment::Key("xo".into()),
                            ValuePathSegment::Key(key.clone()),
                        ];
                        let value = self
                            .nonfinite_paths
                            .contains(&path)
                            .then(|| marker_literal(value, self.nonfinite_key.as_deref()))
                            .flatten()
                            .and_then(|literal| match literal {
                                "NaN" => Some(f64::NAN),
                                "Infinity" => Some(f64::INFINITY),
                                "-Infinity" => Some(f64::NEG_INFINITY),
                                _ => None,
                            })
                            .or_else(|| {
                                value
                                    .as_number()
                                    .and_then(|number| number.to_string().parse::<f64>().ok())
                            })
                            .map_or_else(|| value.clone(), |value| float_value(value, marker_key));
                        (key.clone(), value)
                    })
                    .collect(),
            ),
        );
        for (k, v) in [
            ("bc", &self.bc),
            ("mso", &self.mso),
            ("measurements", &self.measurements),
            ("verify", &self.verify),
            ("tesira", &self.tesira),
            ("snapshots", &self.snapshots),
        ] {
            d.insert(k.into(), v.clone());
        }
        Value::Object(d)
    }
    pub fn to_value(&self) -> Value {
        if let Some(marker_key) = self.nonfinite_key.as_deref() {
            self.to_value_with_marker(marker_key)
        } else {
            self.unmarked_value()
        }
    }
    pub fn load_text(text: &str) -> Result<Self, ProjectError> {
        reject_lone_surrogates(text)?;
        let private_key = unique_private_key(text);
        let escaped = escape_json_object_key(text, SERDE_NUMBER_KEY, &private_key);
        let marker_key = unique_marker_key(&escaped, &private_key)?;
        let rewritten = rewrite_nonfinite(&escaped, &marker_key);
        let mut value: Value = serde_json::from_str(&rewritten)?;
        rename_decoded_key(&mut value, &private_key, SERDE_NUMBER_KEY);
        Self::from_value_with_marker(value, Some(marker_key))
    }
    pub fn save_text(&self) -> String {
        let marker_key = self
            .nonfinite_key
            .clone()
            .unwrap_or_else(|| unique_marker_key_for_value(&self.unmarked_value()));
        let value = self.to_value_with_marker(&marker_key);
        let mut nonfinite_paths = self.nonfinite_paths.clone();
        for (index, number) in [self.curtain.0, self.curtain.1].into_iter().enumerate() {
            if !number.is_finite() {
                nonfinite_paths.insert(vec![
                    ValuePathSegment::Key("curtain".into()),
                    ValuePathSegment::Index(index),
                ]);
            }
        }
        for (index, (frequency, level)) in self.target.points.iter().enumerate() {
            for (part, number) in [frequency, level].into_iter().enumerate() {
                if !number.is_finite() {
                    nonfinite_paths.insert(vec![
                        ValuePathSegment::Key("target".into()),
                        ValuePathSegment::Key("points".into()),
                        ValuePathSegment::Index(index),
                        ValuePathSegment::Index(part),
                    ]);
                }
            }
        }
        for (key, number) in &self.xo {
            if number
                .as_number()
                .and_then(|number| number.to_string().parse::<f64>().ok())
                .is_some_and(|number| !number.is_finite())
            {
                nonfinite_paths.insert(vec![
                    ValuePathSegment::Key("xo".into()),
                    ValuePathSegment::Key(key.clone()),
                ]);
            }
        }
        python_json_with_marker(&value, Some(&marker_key), Some(&nonfinite_paths))
    }
}

fn unique_marker_key(text: &str, temporary_key: &str) -> Result<String, serde_json::Error> {
    let parsed: Value = serde_json::from_str(&rewrite_nonfinite(text, temporary_key))?;
    Ok(unique_marker_key_for_value(&parsed))
}

fn unique_private_key(text: &str) -> String {
    let mut index = 0_u64;
    loop {
        let key = format!("__secs_project_private_number_{index}__");
        if !contains_decoded_object_key(text, &key) {
            return key;
        }
        index += 1;
    }
}

fn contains_decoded_object_key(text: &str, needle: &str) -> bool {
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut escaped = false;
        let mut end = None;
        for (index, ch) in chars.by_ref() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                end = Some(index + ch.len_utf8());
                break;
            }
        }
        let Some(end) = end else {
            return false;
        };
        if text[end..].trim_start().starts_with(':')
            && serde_json::from_str::<String>(&text[start..end]).is_ok_and(|key| key == needle)
        {
            return true;
        }
    }
    false
}

fn unique_marker_key_for_value(value: &Value) -> String {
    fn contains_key(value: &Value, needle: &str) -> bool {
        match value {
            Value::Array(values) => values.iter().any(|value| contains_key(value, needle)),
            Value::Object(values) => {
                values.contains_key(needle)
                    || values.values().any(|value| contains_key(value, needle))
            }
            _ => false,
        }
    }

    let mut index = 0_u64;
    loop {
        let key = format!("__secs_project_nonfinite_{index}__");
        if !contains_key(value, &key) {
            return key;
        }
        index += 1;
    }
}

fn escape_json_object_key(text: &str, decoded_key: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' {
            out.push(ch);
            continue;
        }
        let mut escaped = false;
        let mut end = None;
        for (index, ch) in chars.by_ref() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                end = Some(index + ch.len_utf8());
                break;
            }
        }
        let Some(end) = end else {
            out.push_str(&text[start..]);
            break;
        };
        let token = &text[start..end];
        let is_key = text[end..].trim_start().starts_with(':');
        if is_key && serde_json::from_str::<String>(token).is_ok_and(|key| key == decoded_key) {
            out.push_str(&py_string(replacement));
        } else {
            out.push_str(token);
        }
    }
    out
}

fn rename_decoded_key(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::Array(values) => {
            for value in values {
                rename_decoded_key(value, old, new);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                rename_decoded_key(value, old, new);
            }
            if let Some(index) = values.keys().position(|key| key == old)
                && let Some(value) = values.shift_remove(old)
            {
                values.shift_insert(index, new.to_owned(), value);
            }
        }
        _ => {}
    }
}

fn reject_lone_surrogates(text: &str) -> Result<(), ProjectError> {
    fn surrogate_error() -> ProjectError {
        ProjectError::Invalid("JSON contains an unpaired UTF-16 surrogate".into())
    }

    let bytes = text.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    let mut high = false;
    while index < bytes.len() {
        if !in_string {
            if bytes[index] == b'"' {
                in_string = true;
            }
            index += 1;
            continue;
        }
        match bytes[index] {
            b'"' => {
                if high {
                    return Err(surrogate_error());
                }
                in_string = false;
                index += 1;
            }
            b'\\' if index + 1 < bytes.len() => {
                if bytes[index + 1] != b'u' {
                    if high {
                        return Err(surrogate_error());
                    }
                    index += 2;
                    continue;
                }
                if index + 5 >= bytes.len() {
                    return Ok(());
                }
                let Some(unit) = std::str::from_utf8(&bytes[index + 2..index + 6])
                    .ok()
                    .and_then(|value| u16::from_str_radix(value, 16).ok())
                else {
                    if high {
                        return Err(surrogate_error());
                    }
                    index += 2;
                    continue;
                };
                if (0xd800..=0xdbff).contains(&unit) {
                    if high {
                        return Err(surrogate_error());
                    }
                    high = true;
                } else if (0xdc00..=0xdfff).contains(&unit) {
                    if !high {
                        return Err(surrogate_error());
                    }
                    high = false;
                } else if high {
                    return Err(surrogate_error());
                }
                index += 6;
            }
            _ => {
                if high {
                    return Err(surrogate_error());
                }
                index += 1;
            }
        }
    }
    if high {
        return Err(surrogate_error());
    }
    Ok(())
}

fn rewrite_nonfinite(text: &str, marker_key: &str) -> String {
    let mut rewritten = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some((index, ch)) = chars.next() {
        if in_string {
            rewritten.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            rewritten.push(ch);
            continue;
        }
        let token = if text[index..].starts_with("-Infinity") {
            Some(("-Infinity", "-Infinity"))
        } else if text[index..].starts_with("Infinity") {
            Some(("Infinity", "Infinity"))
        } else if text[index..].starts_with("NaN") {
            Some(("NaN", "NaN"))
        } else {
            None
        };
        if let Some((literal, marker)) = token {
            rewritten.push_str(python_json(&marker_value(marker_key, marker)).trim_end());
            for _ in 1..literal.chars().count() {
                chars.next();
            }
        } else {
            rewritten.push(ch);
        }
    }
    rewritten
}

fn python_float(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == f64::INFINITY {
        return "Infinity".into();
    }
    if x == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if x == 0. && x.is_sign_negative() {
        return "-0.0".into();
    }
    let mut s = ryu::Buffer::new().format_finite(x).to_owned();
    let abs = x.abs();
    if !s.contains(['e', 'E']) && abs != 0.0 && (abs < 1e-4 || abs >= 1e16) {
        let negative = s.starts_with('-');
        let unsigned = s.trim_start_matches('-');
        let (digits, exponent) = if abs < 1.0 {
            let fractional = unsigned.trim_start_matches("0.");
            let leading_zeros = fractional.chars().take_while(|ch| *ch == '0').count();
            (
                fractional.trim_start_matches('0').to_owned(),
                -(leading_zeros as i32) - 1,
            )
        } else {
            let whole_digits = unsigned.find('.').unwrap_or(unsigned.len());
            (unsigned.replace('.', ""), whole_digits as i32 - 1)
        };
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_owned()
        } else {
            format!("{first}.{rest}")
        };
        s = format!("{}{mantissa}e{exponent:+}", if negative { "-" } else { "" });
    }
    if !s.contains(['.', 'e', 'E']) {
        s.push_str(".0");
    }
    if let Some(i) = s.find('e') {
        let exp = s[i + 1..].parse::<i32>().unwrap();
        let sign = if exp >= 0 { "+" } else { "-" };
        s = format!("{}e{}{:02}", &s[..i], sign, exp.abs());
    }
    s
}
fn py_string(s: &str) -> String {
    serde_json::to_string(s).expect("serializing a Rust string cannot fail")
}
fn write_json(
    v: &Value,
    depth: usize,
    marker_key: Option<&str>,
    nonfinite_paths: Option<&BTreeSet<ValuePath>>,
    path: &mut ValuePath,
    out: &mut String,
) {
    if nonfinite_paths.is_none_or(|paths| paths.contains(path))
        && let Some(literal @ ("NaN" | "Infinity" | "-Infinity")) = marker_literal(v, marker_key)
    {
        out.push_str(literal);
        return;
    }
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let raw = n.to_string();
            if !raw.contains(['.', 'e', 'E']) {
                out.push_str(if raw == "-0" { "0" } else { &raw });
            } else if let Some(value) = n.as_f64() {
                out.push_str(&python_float(value));
            } else {
                let negative = raw.starts_with('-');
                let magnitude = raw.trim_start_matches('-');
                let underflow = magnitude.starts_with("0e-") || magnitude.starts_with("0.0e-");
                if underflow {
                    out.push_str(if negative { "-0.0" } else { "0.0" });
                } else {
                    out.push_str(if negative { "-Infinity" } else { "Infinity" });
                }
            }
        }
        Value::String(s) => out.push_str(&py_string(s)),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&" ".repeat((depth + 1) * 2));
                path.push(ValuePathSegment::Index(i));
                write_json(x, depth + 1, marker_key, nonfinite_paths, path, out);
                path.pop();
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(depth * 2));
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (k, x)) in m.iter().enumerate() {
                out.push_str(&" ".repeat((depth + 1) * 2));
                out.push_str(&py_string(k));
                out.push_str(": ");
                path.push(ValuePathSegment::Key(k.clone()));
                write_json(x, depth + 1, marker_key, nonfinite_paths, path, out);
                path.pop();
                if i + 1 < m.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(depth * 2));
            out.push('}');
        }
    }
}
fn python_json_with_marker(
    v: &Value,
    marker_key: Option<&str>,
    nonfinite_paths: Option<&BTreeSet<ValuePath>>,
) -> String {
    let mut out = String::new();
    write_json(v, 0, marker_key, nonfinite_paths, &mut Vec::new(), &mut out);
    out.push('\n');
    out
}

pub fn python_json(v: &Value) -> String {
    python_json_with_marker(v, None, None)
}

pub fn ir_directory(project: &Path) -> PathBuf {
    project.with_extension("irs")
}
pub fn ir_file(project: &Path, role: &str, pos: i64) -> PathBuf {
    let safe = role.replace([':', '/', '\\'], "_");
    ir_directory(project).join(format!("{safe}_P{pos}.wav"))
}
/// Writes the upstream mono float32 RIFF layout.
///
/// For values that Python's `struct.pack('<I', ...)` cannot represent, the
/// legacy infallible API returns an empty vector instead of panicking. Call
/// [`try_write_wav_float32`] when the error must be reported.
pub fn write_wav_float32(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    try_write_wav_float32(samples, sample_rate).unwrap_or_default()
}

pub fn try_write_wav_float32(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, ProjectError> {
    let bytes = samples
        .len()
        .checked_mul(4)
        .ok_or_else(|| ProjectError::Wav("WAV payload size overflow".into()))?;
    let payload_size = u32::try_from(bytes)
        .map_err(|_| ProjectError::Wav("WAV payload exceeds RIFF limit".into()))?;
    let riff_size = payload_size
        .checked_add(36)
        .ok_or_else(|| ProjectError::Wav("WAV RIFF size overflow".into()))?;
    let byte_rate = sample_rate
        .checked_mul(4)
        .ok_or_else(|| ProjectError::Wav("WAV sample rate overflow".into()))?;
    let capacity = 44usize
        .checked_add(bytes)
        .ok_or_else(|| ProjectError::Wav("WAV allocation size overflow".into()))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend(b"RIFF");
    out.extend(&riff_size.to_le_bytes());
    out.extend(b"WAVEfmt \x10\0\0\0\x03\0\x01\0");
    out.extend(&sample_rate.to_le_bytes());
    out.extend(&byte_rate.to_le_bytes());
    out.extend(&4u16.to_le_bytes());
    out.extend(&32u16.to_le_bytes());
    out.extend(b"data");
    out.extend(&payload_size.to_le_bytes());
    for x in samples {
        out.extend(&x.to_le_bytes());
    }
    Ok(out)
}
const PCM_GUID: [u8; 16] = [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
const FLOAT_GUID: [u8; 16] = [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];

#[derive(Debug, Clone, Copy)]
struct WavFormat {
    tag: u16,
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    bits: u16,
    valid_bits: u16,
}
type WavChunks = (WavFormat, Vec<u8>);

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, ProjectError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ProjectError::Wav("truncated WAV field".into()))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, ProjectError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ProjectError::Wav("truncated WAV field".into()))?;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        ProjectError::Wav("truncated WAV field".into())
    })?))
}
fn parse_wav_format(bytes: &[u8]) -> Result<WavFormat, ProjectError> {
    if bytes.len() < 16 {
        return Err(ProjectError::Wav("short fmt chunk".into()));
    }
    let mut tag = le_u16(bytes, 0)?;
    let channels = le_u16(bytes, 2)?;
    let sample_rate = le_u32(bytes, 4)?;
    let block_align = le_u16(bytes, 12)?;
    let bits = le_u16(bytes, 14)?;
    let mut valid_bits = bits;
    if tag == 0xfffe {
        if bytes.len() < 40 || le_u16(bytes, 16)? < 22 {
            return Err(ProjectError::Wav("short extensible fmt chunk".into()));
        }
        valid_bits = le_u16(bytes, 18)?;
        let guid = bytes
            .get(24..40)
            .ok_or_else(|| ProjectError::Wav("missing extensible subformat".into()))?;
        tag = if guid == PCM_GUID {
            1
        } else if guid == FLOAT_GUID {
            3
        } else {
            return Err(ProjectError::Wav(
                "unsupported extensible WAV subformat".into(),
            ));
        };
    }
    Ok(WavFormat {
        tag,
        channels,
        sample_rate,
        block_align,
        bits,
        valid_bits,
    })
}
fn wav_chunks(data: &[u8], default_format: Option<WavFormat>) -> Result<WavChunks, ProjectError> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(ProjectError::Wav("not a RIFF/WAVE file".into()));
    }
    let mut pos = 12usize;
    let mut format = None;
    let mut payload = None;
    while let Some(header_end) = pos.checked_add(8) {
        if header_end > data.len() {
            break;
        }
        let size = le_u32(data, pos + 4)? as usize;
        let declared_end = header_end
            .checked_add(size)
            .ok_or_else(|| ProjectError::Wav("chunk size overflow".into()))?;
        let end = declared_end.min(data.len());
        match &data[pos..pos + 4] {
            b"fmt " => format = Some(parse_wav_format(&data[header_end..end])?),
            b"data" => payload = Some(data[header_end..end].to_vec()),
            _ => {}
        }
        if declared_end >= data.len() {
            break;
        }
        pos = declared_end
            .checked_add(size & 1)
            .ok_or_else(|| ProjectError::Wav("chunk padding overflow".into()))?;
        if pos > data.len() {
            break;
        }
    }
    Ok((
        format
            .or(default_format)
            .ok_or_else(|| ProjectError::Wav("missing fmt chunk".into()))?,
        payload.ok_or_else(|| ProjectError::Wav("missing data chunk".into()))?,
    ))
}
pub fn read_wav_float32(data: &[u8]) -> Result<(Vec<f32>, u32), ProjectError> {
    let default = WavFormat {
        tag: 3,
        channels: 1,
        sample_rate: 48000,
        block_align: 4,
        bits: 32,
        valid_bits: 32,
    };
    let (format, payload) = wav_chunks(data, Some(default))?;
    if format.tag != 3 || format.bits != 32 || payload.len() % 4 != 0 {
        return Err(ProjectError::Wav(format!(
            "not 32-bit float WAV (fmt={}, bits={})",
            format.tag, format.bits
        )));
    }
    Ok((
        payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| f32::from_le_bytes(*x))
            .collect(),
        format.sample_rate,
    ))
}
/// Reads mono PCM 16/24/32 and IEEE float 32/64 WAV, including extensible WAV.
/// PCM 8-bit and valid-bit/container combinations other than left-aligned PCM in
/// a 32-bit container are unsupported. It also does not decode compressed codecs,
/// RF64, endian-swapped RIFX, multichannel audio, or containers other than WAV.
pub fn read_mono_float_wav(data: &[u8]) -> Result<(Vec<f32>, u32), ProjectError> {
    let (format, payload) = wav_chunks(data, None)?;
    if format.channels != 1 {
        return Err(ProjectError::Wav("WAV is not mono".into()));
    }
    let bytes_per_sample = usize::from(format.bits.div_ceil(8));
    if bytes_per_sample == 0
        || usize::from(format.block_align) != bytes_per_sample
        || payload.len() % bytes_per_sample != 0
    {
        return Err(ProjectError::Wav("invalid block alignment".into()));
    }
    let out = match (format.tag, format.bits, format.valid_bits) {
        (3, 32, 32) => payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| f32::from_le_bytes(*x))
            .collect(),
        (3, 64, 64) => payload
            .as_chunks::<8>()
            .0
            .iter()
            .map(|x| f64::from_le_bytes(*x) as f32)
            .collect(),
        (1, 16, 16) => payload
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes(*x) as f32 / 32768.)
            .collect(),
        (1, 24, 24) => payload
            .as_chunks::<3>()
            .0
            .iter()
            .map(|x| {
                let n = ((x[0] as i32) | ((x[1] as i32) << 8) | ((x[2] as i32) << 16)) << 8 >> 8;
                n as f32 / 8388608.
            })
            .collect(),
        (1, 32, 32) => payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| i32::from_le_bytes(*x) as f32 / 2147483648.)
            .collect(),
        (1, 32, valid @ (1..=31)) => {
            let shift = 32 - u32::from(valid);
            let divisor = (1_u64 << (valid - 1)) as f32;
            payload
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| (i32::from_le_bytes(*x) >> shift) as f32 / divisor)
                .collect()
        }
        _ => {
            return Err(ProjectError::Wav(format!(
                "unsupported WAV format {}/{}/{}",
                format.tag, format.bits, format.valid_bits
            )));
        }
    };
    Ok((out, format.sample_rate))
}
pub fn save_ir(
    project: &Path,
    role: &str,
    pos: i64,
    samples: &[f32],
    rate: u32,
) -> Result<PathBuf, ProjectError> {
    let path = ir_file(project, role, pos);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, try_write_wav_float32(samples, rate)?)?;
    Ok(path)
}
pub fn load_ir(project: &Path, role: &str, pos: i64) -> Result<(Vec<f32>, u32), ProjectError> {
    read_mono_float_wav(&fs::read(ir_file(project, role, pos))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_numbers_match_python() {
        let value = serde_json::json!([
            1e-5,
            1.2345678901234568e-5,
            1e16,
            1.2e20,
            1.2e-20,
            5e-324,
            100000.0,
            -0.0
        ]);
        assert_eq!(
            python_json(&value),
            "[\n  1e-05,\n  1.2345678901234568e-05,\n  1e+16,\n  1.2e+20,\n  1.2e-20,\n  5e-324,\n  100000.0,\n  -0.0\n]\n"
        );
    }
    #[test]
    fn wav_round_trip() {
        let x = [0., -0., 1., -1., 1. / 3.];
        let bytes = write_wav_float32(&x, 44100);
        let (y, r) = read_wav_float32(&bytes).unwrap();
        assert_eq!(r, 44100);
        assert_eq!(x, y.as_slice());
    }

    #[test]
    fn direct_preset_uses_dataclass_dsp_default() {
        let project = Project::from_preset("2.0").unwrap();
        assert_eq!(project.dsp, DspProfile::default());
        assert_eq!(project.dsp.profile, "tesira");
        assert_eq!(project.dsp.fir_budget, 32768);
        assert_eq!(project.dsp.name, "");
        assert_eq!(project.config.roles(), ["L", "R"]);
    }

    #[test]
    fn forward_versions_and_unknown_profile_limits_are_preserved() {
        for version in [4, 5] {
            let project = Project::load_text(&format!(r#"{{"version":{version}}}"#)).unwrap();
            assert_eq!(project.version, version);
            assert!(
                project
                    .save_text()
                    .contains(&format!("\"version\": {version}"))
            );
        }
        let project = Project::load_text(
            r#"{"dsp":{"profile":"future","fir_taps":4096,"fir_budget":32768,"iir_bands":16,"support_mode":"iir","support_taps":1024,"name":""}}"#,
        )
        .unwrap();
        assert_eq!(project.dsp.fir_taps_max, 4096);
    }

    #[test]
    fn focus_coercion_matches_python() {
        for (value, expected) in [
            (Value::String("75".into()), 75),
            (Value::Bool(true), 0),
            (Value::String("NaN".into()), 0),
            (Value::String("Infinity".into()), 0),
            (Value::String("-Infinity".into()), 0),
            (Value::Null, 50),
        ] {
            assert_eq!(snap_focus(&value), expected, "{value:?}");
        }
        for (text, expected) in [
            ("NaN", 0),
            ("Infinity", 0),
            ("-Infinity", 0),
            ("1e400", 0),
            ("-1e400", 0),
        ] {
            let project = Project::load_text(&format!(r#"{{"mso":{{"focus":{text}}}}}"#)).unwrap();
            assert_eq!(project.mso["focus"], expected);
        }
    }

    #[test]
    fn target_sort_matches_python_stability() {
        let points = TargetCurve::new(vec![(0.0, 1.0), (-0.0, 2.0)]).points;
        assert!(!points[0].0.is_sign_negative());
        assert!(points[1].0.is_sign_negative());

        let points = TargetCurve::new(vec![(f64::NAN, 1.0), (2.0, 2.0), (1.0, 3.0)]).points;
        assert_eq!(points[0], (1.0, 3.0));
        assert!(points[1].0.is_nan());
        assert_eq!(points[2], (2.0, 2.0));
    }

    #[test]
    fn project_collection_edge_cases_match_upstream() {
        let project =
            Project::load_text(r#"{"curtain":[20,200,999],"bass":{"paths":[{}, {}, {}]}}"#)
                .unwrap();
        assert_eq!(project.curtain, (20.0, 200.0));
        assert_eq!(project.bass_path_count(), 3);
        assert!(Project::load_text(r#"{"curtain":[20]}"#).is_err());

        let config = SpeakerConfig {
            speakers: vec![
                Speaker {
                    role: "L".into(),
                    size: "small".into(),
                    bands: vec![Band {
                        band: "FULL".into(),
                        out: 1,
                        pol: "+".into(),
                    }],
                },
                Speaker {
                    role: "R".into(),
                    size: "small".into(),
                    bands: vec![Band {
                        band: "FULL".into(),
                        out: 1,
                        pol: "+".into(),
                    }],
                },
                Speaker {
                    role: "C".into(),
                    size: "small".into(),
                    bands: vec![Band {
                        band: "FULL".into(),
                        out: 1,
                        pol: "+".into(),
                    }],
                },
            ],
            ..SpeakerConfig::default()
        };
        assert_eq!(
            config.validate(),
            ["출력 채널 중복: 1 (L ↔ R)", "출력 채널 중복: 1 (L ↔ C)"]
        );
        assert_eq!(
            config
                .groups()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            ["프론트", "센터"]
        );
    }

    #[test]
    fn nonfinite_project_text_preserves_utf8_and_strings() {
        let text = r#"{
  "name": "한글 😀 NaN Infinity -Infinity",
  "future": {
    "nan": NaN,
    "positive": Infinity,
    "negative": -Infinity,
    "literal": "NaN",
    "old_nan_sentinel": "\u0000secs-project:NaN",
    "old_infinity_sentinel": "\u0000secs-project:Infinity",
    "old_negative_sentinel": "\u0000secs-project:-Infinity"
  }
}
"#;
        let project = Project::load_text(text).unwrap();
        let saved = project.save_text();
        assert!(saved.contains("한글 😀 NaN Infinity -Infinity"));
        assert!(saved.contains("\"nan\": NaN"));
        assert!(saved.contains("\"positive\": Infinity"));
        assert!(saved.contains("\"negative\": -Infinity"));
        assert!(saved.contains("\"literal\": \"NaN\""));
        assert!(saved.contains(r#""old_nan_sentinel": "\u0000secs-project:NaN""#));
        assert!(saved.contains(r#""old_infinity_sentinel": "\u0000secs-project:Infinity""#));
        assert!(saved.contains(r#""old_negative_sentinel": "\u0000secs-project:-Infinity""#));
        assert!(Project::load_text(&saved).is_ok());
    }

    #[test]
    fn loaded_nonfinite_provenance_does_not_capture_later_collision_objects() {
        let result = std::panic::catch_unwind(|| {
            let mut project = Project::load_text(
                r#"{"version":1,"future":NaN,"nested":{"values":[Infinity,{"negative":-Infinity}]},"xo":{"하이트":NaN},"calibration":{"art_low":Infinity}}"#,
            )?;
            project.extra.insert(
                "literal".into(),
                serde_json::json!({"__secs_project_nonfinite_0__":"Infinity"}),
            );
            project.extra.insert(
                "nested_literal".into(),
                serde_json::json!([{"inside":{"__secs_project_nonfinite_0__":"NaN"}}]),
            );
            Ok::<_, ProjectError>(project.save_text())
        });
        assert!(result.is_ok());
        let saved = result.unwrap().unwrap();
        assert!(saved.contains("\"future\": NaN"), "{saved}");
        assert!(saved.contains("\"values\": [\n      Infinity,"), "{saved}");
        assert!(saved.contains("\"negative\": -Infinity"), "{saved}");
        assert!(saved.contains("\"탑\": NaN"), "{saved}");
        assert!(saved.contains("\"mimo_low\": Infinity"), "{saved}");
        assert!(
            saved.contains(
                "\"literal\": {\n    \"__secs_project_nonfinite_0__\": \"Infinity\"\n  }"
            ),
            "{saved}"
        );
        assert!(
            saved.contains(
                "\"inside\": {\n        \"__secs_project_nonfinite_0__\": \"NaN\"\n      }"
            ),
            "{saved}"
        );
    }

    #[test]
    fn from_value_preserves_typed_overflow_as_nonfinite() {
        let value: Value = serde_json::from_str(
            r#"{"curtain":[1e400,-1e400],"target":[[1e400,-1e400]],"xo":{"L":1e400,"R":-1e400}}"#,
        )
        .unwrap();
        let result = std::panic::catch_unwind(|| Project::from_value(value));
        assert!(result.is_ok());
        let project = result.unwrap().unwrap();
        assert_eq!(project.curtain, (f64::INFINITY, f64::NEG_INFINITY));
        assert!(project.target.points[0].0.is_infinite());
        assert!(project.target.points[0].1.is_infinite());
        let saved = project.save_text();
        assert!(
            saved.contains("\"curtain\": [\n    Infinity,\n    -Infinity\n  ]"),
            "{saved}"
        );
        assert!(saved.contains("\"L\": Infinity"), "{saved}");
        assert!(saved.contains("\"R\": -Infinity"), "{saved}");
    }

    #[test]
    fn typed_arbitrary_precision_integers_coerce_without_i128_ceiling() {
        let two_to_128 = "340282366920938463463374607431768211456";
        let text = format!(
            r#"{{"curtain":[{two_to_128},300],"target":[[{two_to_128},1]],"xo":{{"L":{two_to_128}}},"future":{two_to_128}}}"#
        );
        let result = std::panic::catch_unwind(|| Project::load_text(&text));
        assert!(result.is_ok());
        let project = result.unwrap().unwrap();
        assert_eq!(project.curtain.0, two_to_128.parse::<f64>().unwrap());
        assert_eq!(
            project.target.points[0].0,
            two_to_128.parse::<f64>().unwrap()
        );
        let saved = project.save_text();
        assert!(saved.contains("\"L\": 3.402823669209385e+38"), "{saved}");
        assert!(
            saved.contains(&format!("\"future\": {two_to_128}")),
            "{saved}"
        );

        let oversized = format!(r#"{{"curtain":[1{},300]}}"#, "0".repeat(400));
        let result = std::panic::catch_unwind(|| Project::load_text(&oversized));
        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), Err(ProjectError::Invalid(message)) if message.contains("too large"))
        );
    }

    #[test]
    fn escaped_strings_and_collision_objects_round_trip_exactly() {
        for (input, expected_fragment) in [
            (r#"{"future":"\\\\/"}"#, r#""future": "\\\\/""#),
            (
                r#"{"future":{"__secs_project_nonfinite_0__":"NaN"}}"#,
                "\"future\": {\n    \"__secs_project_nonfinite_0__\": \"NaN\"\n  }",
            ),
            (
                r#"{"future":{"$serde_json::private::Number":"123"}}"#,
                "\"future\": {\n    \"$serde_json::private::Number\": \"123\"\n  }",
            ),
            (
                r#"{"future":{"__secs_project_private_number_0__":"kept","$serde_json::private::Number":"123"}}"#,
                "\"future\": {\n    \"__secs_project_private_number_0__\": \"kept\",\n    \"$serde_json::private::Number\": \"123\"\n  }",
            ),
        ] {
            let project = Project::load_text(input).unwrap();
            let saved = project.save_text();
            assert!(
                saved.contains(expected_fragment),
                "expected={expected_fragment:?} saved={saved}"
            );
            let reloaded = Project::load_text(&saved).unwrap();
            assert_eq!(reloaded.extra["future"], project.extra["future"]);
        }
    }

    #[test]
    fn normally_constructed_nonfinite_values_save_as_python_constants() {
        for mut project in [
            Project::default(),
            Project::from_preset("2.0").unwrap(),
            Project::from_value(serde_json::json!({})).unwrap(),
        ] {
            project.curtain = (f64::NAN, f64::INFINITY);
            project.target = TargetCurve::new(vec![(f64::NEG_INFINITY, f64::NAN)]);
            let saved = project.save_text();
            assert!(
                saved.contains("\"curtain\": [\n    NaN,\n    Infinity\n  ]"),
                "{saved}"
            );
            assert!(saved.contains("-Infinity"));
            assert!(Project::load_text(&saved).is_ok());
        }
    }

    #[test]
    fn typed_integer_negative_zero_coerces_to_positive_float_zero() {
        let project = Project::load_text(
            r#"{"version":3,"curtain":[-0,-0.0],"target":[[-0,-0.0]],"xo":{"탑":-0,"하이트":-0.0}}"#,
        )
        .unwrap();
        let saved = project.save_text();
        assert!(
            saved.contains("\"curtain\": [\n    0.0,\n    -0.0\n  ]"),
            "{saved}"
        );
        assert!(saved.contains("\"points\": [\n      [\n        0.0,\n        -0.0\n      ]"));
        assert!(saved.contains("\"탑\": 0.0"));
        assert!(saved.contains("\"하이트\": -0.0"), "{saved}");
    }

    #[test]
    fn arbitrary_precision_numbers_save_like_python_without_panicking() {
        for (text, needle) in [
            (r#"{"future":1e400}"#, "\"future\": Infinity"),
            (r#"{"future":-1e400}"#, "\"future\": -Infinity"),
            (r#"{"future":1e-4000}"#, "\"future\": 0.0"),
            (r#"{"future":-1e-4000}"#, "\"future\": -0.0"),
            (r#"{"future":-0}"#, "\"future\": 0"),
        ] {
            let result = std::panic::catch_unwind(|| {
                let project = Project::load_text(text)?;
                Ok::<_, ProjectError>(project.save_text())
            });
            assert!(result.is_ok(), "panicked for {text}");
            assert!(result.unwrap().unwrap().contains(needle), "{text}");
        }
    }

    #[test]
    fn lone_surrogate_is_an_explicit_error() {
        for text in [
            r#"{"future":"\ud800"}"#,
            r#"{"future":"\udc00"}"#,
            r#"{"future":"\ud800x"}"#,
        ] {
            let result = std::panic::catch_unwind(|| Project::load_text(text));
            assert!(result.is_ok());
            assert!(
                matches!(result.unwrap(), Err(ProjectError::Invalid(message)) if message.contains("surrogate"))
            );
        }
        for text in [r#"{"future":"\\ud800"}"#, r#"{"future":"paired 😀"}"#] {
            let result = std::panic::catch_unwind(|| Project::load_text(text));
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok(), "rejected {text}");
        }
    }

    #[test]
    fn malformed_projects_return_errors_without_panicking() {
        let cases = [
            "[]",
            r#"{"version":"three"}"#,
            r#"{"config":[]}"#,
            r#"{"config":{"speakers":{}}}"#,
            r#"{"config":{"speakers":[{"bands":{}}]}}"#,
            r#"{"dsp":{"fir_taps":false}}"#,
            r#"{"target":{"points":{}}}"#,
            r#"{"target":{"points":[[1]]}}"#,
            r#"{"curtain":"wide"}"#,
            r#"{"xo":[]}"#,
            r#"{"bc":[]}"#,
            r#"{"measurements":{}}"#,
            r#"{"extra":[]}"#,
            r#"{"future":NaNfoo}"#,
            "{\"이름\": NaN",
        ];
        for text in cases {
            let result = std::panic::catch_unwind(|| Project::load_text(text));
            assert!(result.is_ok(), "panicked for {text:?}");
            assert!(result.unwrap().is_err(), "accepted {text:?}");
        }
    }

    fn wav_with_format(fmt: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend(b"fmt ");
        body.extend(&(fmt.len() as u32).to_le_bytes());
        body.extend(fmt);
        if fmt.len() % 2 == 1 {
            body.push(0);
        }
        body.extend(b"data");
        body.extend(&(payload.len() as u32).to_le_bytes());
        body.extend(payload);
        if payload.len() % 2 == 1 {
            body.push(0);
        }
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(&(4 + body.len() as u32).to_le_bytes());
        wav.extend(b"WAVE");
        wav.extend(body);
        wav
    }

    fn basic_fmt(tag: u16, bits: u16) -> Vec<u8> {
        let bytes = bits.div_ceil(8);
        let mut fmt = Vec::new();
        fmt.extend(&tag.to_le_bytes());
        fmt.extend(&1u16.to_le_bytes());
        fmt.extend(&48000u32.to_le_bytes());
        fmt.extend(&(48000 * u32::from(bytes)).to_le_bytes());
        fmt.extend(&bytes.to_le_bytes());
        fmt.extend(&bits.to_le_bytes());
        fmt
    }

    #[test]
    fn wav_reader_supports_pcm_and_float_encodings() {
        let cases = [
            (basic_fmt(1, 16), vec![0, 128, 255, 127]),
            (basic_fmt(1, 24), vec![0, 0, 128, 255, 255, 127]),
            (basic_fmt(1, 32), {
                let mut bytes = i32::MIN.to_le_bytes().to_vec();
                bytes.extend(i32::MAX.to_le_bytes());
                bytes
            }),
            (basic_fmt(3, 64), {
                let mut bytes = (-1.0f64).to_le_bytes().to_vec();
                bytes.extend(1.0f64.to_le_bytes());
                bytes
            }),
        ];
        for (fmt, payload) in cases {
            let (samples, rate) = read_mono_float_wav(&wav_with_format(&fmt, &payload)).unwrap();
            assert_eq!(rate, 48000);
            assert_eq!(samples.len(), 2);
            assert_eq!(samples[0], -1.0);
            assert!(samples[1] > 0.99);
        }
    }

    #[test]
    fn wav_reader_parses_extensible_subformat_from_fmt_chunk() {
        for (guid, payload, expected) in [
            (PCM_GUID, i16::MIN.to_le_bytes().to_vec(), -1.0),
            (FLOAT_GUID, (-0.5f32).to_le_bytes().to_vec(), -0.5),
        ] {
            let bits = if guid == PCM_GUID { 16 } else { 32 };
            let bytes = bits / 8;
            let mut fmt = basic_fmt(0xfffe, bits);
            fmt.extend(&22u16.to_le_bytes());
            fmt.extend(&bits.to_le_bytes());
            fmt.extend(&0u32.to_le_bytes());
            fmt.extend(&guid);
            fmt[8..12].copy_from_slice(&(48000 * u32::from(bytes)).to_le_bytes());
            fmt[12..14].copy_from_slice(&bytes.to_le_bytes());
            let (samples, _) = read_mono_float_wav(&wav_with_format(&fmt, &payload)).unwrap();
            assert_eq!(samples, [expected]);
        }
    }

    #[test]
    fn wav_parser_matches_upstream_truncation_and_defaults() {
        let sample = 0.25f32.to_le_bytes();
        let mut no_fmt = Vec::new();
        no_fmt.extend(b"RIFF");
        no_fmt.extend(&16u32.to_le_bytes());
        no_fmt.extend(b"WAVEdata");
        no_fmt.extend(&4u32.to_le_bytes());
        no_fmt.extend(sample);
        let (samples, rate) = read_wav_float32(&no_fmt).unwrap();
        assert_eq!(samples, [0.25]);
        assert_eq!(rate, 48000);

        let mut truncated = write_wav_float32(&[0.25], 48000);
        truncated[40..44].copy_from_slice(&8u32.to_le_bytes());
        let (samples, _) = read_wav_float32(&truncated).unwrap();
        assert_eq!(samples, [0.25]);

        let mut odd_trailing = write_wav_float32(&[0.25], 48000);
        odd_trailing.extend(b"JUNK\x01\0\0\0x");
        let (samples, _) = read_wav_float32(&odd_trailing).unwrap();
        assert_eq!(samples, [0.25]);
    }

    #[test]
    fn wav_writer_overflow_never_panics() {
        let result = std::panic::catch_unwind(|| try_write_wav_float32(&[], u32::MAX));
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
        assert!(write_wav_float32(&[], u32::MAX).is_empty());
    }

    #[test]
    fn malformed_wav_returns_errors_without_panicking() {
        let malformed = [
            Vec::new(),
            b"RIFF\0\0\0\0WAVE".to_vec(),
            b"RIFF\x10\0\0\0WAVEfmt \xff\xff\xff\xff".to_vec(),
            {
                let mut bytes = write_wav_float32(&[1.0], 48000);
                bytes.truncate(30);
                bytes
            },
        ];
        for bytes in malformed {
            let result = std::panic::catch_unwind(|| read_mono_float_wav(&bytes));
            assert!(result.is_ok());
            assert!(result.unwrap().is_err());
        }
    }
}

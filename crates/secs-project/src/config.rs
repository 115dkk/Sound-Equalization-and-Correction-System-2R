use crate::ProjectError;
use secs_types::role::{RoleKind, SUB_ROLES, role_group, role_kind, role_order};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    pub fn to_json(&self) -> crate::PyJson {
        crate::PyJson::Object(vec![
            (
                "speakers".into(),
                crate::PyJson::Array(
                    self.speakers
                        .iter()
                        .map(|speaker| {
                            crate::PyJson::Object(vec![
                                ("role".into(), crate::PyJson::Str(speaker.role.clone())),
                                ("size".into(), crate::PyJson::Str(speaker.size.clone())),
                                (
                                    "bands".into(),
                                    crate::PyJson::Array(
                                        speaker
                                            .bands
                                            .iter()
                                            .map(|band| {
                                                crate::PyJson::Object(vec![
                                                    (
                                                        "band".into(),
                                                        crate::PyJson::Str(band.band.clone()),
                                                    ),
                                                    (
                                                        "out".into(),
                                                        crate::PyJson::Int(band.out.to_string()),
                                                    ),
                                                    (
                                                        "pol".into(),
                                                        crate::PyJson::Str(band.pol.clone()),
                                                    ),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("device".into(), crate::PyJson::Str(self.device.clone())),
            ("preset".into(), crate::PyJson::Str(self.preset.clone())),
            (
                "biamp_default".into(),
                crate::PyJson::Bool(self.biamp_default),
            ),
        ])
    }

    pub fn from_json(value: crate::PyJson) -> Result<Self, ProjectError> {
        let object = value
            .object()
            .ok_or_else(|| ProjectError::Invalid("config must be an object".into()))?;
        let get = |key: &str| {
            object
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        };
        let speakers = match get("speakers") {
            None | Some(crate::PyJson::Null) => Vec::new(),
            Some(crate::PyJson::Array(values)) => values
                .iter()
                .map(|value| {
                    let object = value
                        .object()
                        .ok_or_else(|| ProjectError::Invalid("speaker must be an object".into()))?;
                    let get = |key: &str| {
                        object
                            .iter()
                            .find(|(name, _)| name == key)
                            .map(|(_, value)| value)
                    };
                    let bands = match get("bands") {
                        None | Some(crate::PyJson::Null) => Vec::new(),
                        Some(crate::PyJson::Array(values)) => values
                            .iter()
                            .map(|value| {
                                let object = value.object().ok_or_else(|| {
                                    ProjectError::Invalid("band must be an object".into())
                                })?;
                                let get = |key: &str| {
                                    object
                                        .iter()
                                        .find(|(name, _)| name == key)
                                        .map(|(_, value)| value)
                                };
                                Ok(Band {
                                    band: crate::project::string(get("band"), ""),
                                    out: crate::project::integer(get("out"), 0, "band.out")?,
                                    pol: crate::project::string(get("pol"), "+"),
                                })
                            })
                            .collect::<Result<Vec<_>, ProjectError>>()?,
                        Some(_) => {
                            return Err(ProjectError::Invalid(
                                "speaker.bands must be an array".into(),
                            ));
                        }
                    };
                    Ok(Speaker {
                        role: crate::project::string(get("role"), ""),
                        size: crate::project::string(get("size"), "small"),
                        bands,
                    })
                })
                .collect::<Result<Vec<_>, ProjectError>>()?,
            Some(_) => {
                return Err(ProjectError::Invalid(
                    "config.speakers must be an array".into(),
                ));
            }
        };
        Ok(Self {
            speakers,
            device: crate::project::string(get("device"), ""),
            preset: crate::project::string(get("preset"), "custom"),
            biamp_default: get("biamp_default")
                .and_then(crate::PyJson::bool)
                .unwrap_or(true),
        })
    }
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

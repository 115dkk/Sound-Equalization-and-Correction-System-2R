//! Speaker-role vocabulary shared by project files and later service layers.

use serde::{Deserialize, Serialize};

pub const ROLES: [&str; 27] = [
    "L", "R", "C", "SW1", "SW2", "SW3", "SW4", "SW5", "SW6", "SW7", "SW8", "Ls", "Rs", "Lb", "Rb",
    "Lw", "Rw", "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr", "Hfl", "Hfr", "Hbl", "Hbr",
];
pub const SUB_ROLES: [&str; 8] = ["SW1", "SW2", "SW3", "SW4", "SW5", "SW6", "SW7", "SW8"];
pub const TOP_GROUP: &str = "탑";
pub const HEIGHT_GROUP: &str = "하이트";
pub const LEGACY_TOP_GROUP: &str = "하이트";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    Main,
    Sub,
    Top,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleInfo {
    pub label: &'static str,
    pub kind: RoleKind,
    pub group: &'static str,
}

const CUSTOM: RoleInfo = RoleInfo {
    label: "Custom",
    kind: RoleKind::Main,
    group: "기타",
};

pub fn role_info(role: &str) -> RoleInfo {
    let (label, kind, group) = match role {
        "L" => ("Left", RoleKind::Main, "프론트"),
        "R" => ("Right", RoleKind::Main, "프론트"),
        "C" => ("Center", RoleKind::Main, "센터"),
        "SW1" => ("Subwoofer 1", RoleKind::Sub, "서브"),
        "SW2" => ("Subwoofer 2", RoleKind::Sub, "서브"),
        "SW3" => ("Subwoofer 3", RoleKind::Sub, "서브"),
        "SW4" => ("Subwoofer 4", RoleKind::Sub, "서브"),
        "SW5" => ("Subwoofer 5", RoleKind::Sub, "서브"),
        "SW6" => ("Subwoofer 6", RoleKind::Sub, "서브"),
        "SW7" => ("Subwoofer 7", RoleKind::Sub, "서브"),
        "SW8" => ("Subwoofer 8", RoleKind::Sub, "서브"),
        "Ls" => ("Left Surround", RoleKind::Main, "서라운드"),
        "Rs" => ("Right Surround", RoleKind::Main, "서라운드"),
        "Lb" => ("Left Rear Surround", RoleKind::Main, "백"),
        "Rb" => ("Right Rear Surround", RoleKind::Main, "백"),
        "Lw" => ("Left Wide", RoleKind::Main, "와이드"),
        "Rw" => ("Right Wide", RoleKind::Main, "와이드"),
        "Tfl" => ("Left Top Front", RoleKind::Top, TOP_GROUP),
        "Tfr" => ("Right Top Front", RoleKind::Top, TOP_GROUP),
        "Tml" => ("Left Top Middle", RoleKind::Top, TOP_GROUP),
        "Tmr" => ("Right Top Middle", RoleKind::Top, TOP_GROUP),
        "Tbl" => ("Left Top Rear", RoleKind::Top, TOP_GROUP),
        "Tbr" => ("Right Top Rear", RoleKind::Top, TOP_GROUP),
        "Hfl" => ("Left Front Height", RoleKind::Height, HEIGHT_GROUP),
        "Hfr" => ("Right Front Height", RoleKind::Height, HEIGHT_GROUP),
        "Hbl" => ("Left Rear Height", RoleKind::Height, HEIGHT_GROUP),
        "Hbr" => ("Right Rear Height", RoleKind::Height, HEIGHT_GROUP),
        _ => return CUSTOM,
    };
    RoleInfo { label, kind, group }
}

pub fn role_label(role: &str) -> &'static str {
    role_info(role).label
}

pub fn role_kind(role: &str) -> RoleKind {
    role_info(role).kind
}

pub fn role_group(role: &str) -> &'static str {
    if role_kind(role) == RoleKind::Sub {
        "sub"
    } else {
        role_info(role).group
    }
}

pub fn role_code(role: &str) -> &str {
    match role {
        "Lb" => "Lrs",
        "Rb" => "Rrs",
        "Tfl" => "Ltf",
        "Tfr" => "Rtf",
        "Tml" => "Ltm",
        "Tmr" => "Rtm",
        "Tbl" => "Ltr",
        "Tbr" => "Rtr",
        "Hfl" => "Lfh",
        "Hfr" => "Rfh",
        "Hbl" => "Lrh",
        "Hbr" => "Rrh",
        _ => role,
    }
}

pub fn role_order(role: &str) -> usize {
    let base = role.split(':').next().unwrap_or(role);
    ROLES
        .iter()
        .position(|known| *known == base)
        .unwrap_or(ROLES.len())
}

pub fn role_position(role: &str) -> Option<(f64, f64)> {
    Some(match role {
        "L" => (0.27, 0.18),
        "R" => (0.73, 0.18),
        "C" => (0.5, 0.12),
        "SW1" => (0.16, 0.4),
        "SW2" => (0.84, 0.4),
        "SW3" => (0.16, 0.78),
        "SW4" => (0.84, 0.78),
        "SW5" => (0.5, 0.04),
        "SW6" => (0.5, 0.97),
        "SW7" => (0.04, 0.5),
        "SW8" => (0.96, 0.5),
        "Ls" => (0.1, 0.62),
        "Rs" => (0.9, 0.62),
        "Lb" => (0.22, 0.93),
        "Rb" => (0.78, 0.93),
        "Lw" => (0.06, 0.34),
        "Rw" => (0.94, 0.34),
        "Tfl" => (0.36, 0.3),
        "Tfr" => (0.64, 0.3),
        "Tml" => (0.36, 0.5),
        "Tmr" => (0.64, 0.5),
        "Tbl" => (0.36, 0.84),
        "Tbr" => (0.64, 0.84),
        "Hfl" => (0.14, 0.08),
        "Hfr" => (0.86, 0.08),
        "Hbl" => (0.07, 0.86),
        "Hbr" => (0.93, 0.86),
        _ => return None,
    })
}

fn python_isalnum(ch: char) -> bool {
    // Rust's Alphabetic derived property includes some combining marks that
    // Python's str.isalnum rejects. U+0903 is the observed project-title case.
    ch != '\u{0903}' && ch.is_alphanumeric()
}

fn python_isdigit(ch: char) -> bool {
    // Python 3.13's Unicode 15.1 Digit property: Numeric_Type=Digit or Decimal.
    matches!(
        ch,
        '\u{30}'..='\u{39}'
            | '\u{b2}'..='\u{b3}'
            | '\u{b9}'
            | '\u{660}'..='\u{669}'
            | '\u{6f0}'..='\u{6f9}'
            | '\u{7c0}'..='\u{7c9}'
            | '\u{966}'..='\u{96f}'
            | '\u{9e6}'..='\u{9ef}'
            | '\u{a66}'..='\u{a6f}'
            | '\u{ae6}'..='\u{aef}'
            | '\u{b66}'..='\u{b6f}'
            | '\u{be6}'..='\u{bef}'
            | '\u{c66}'..='\u{c6f}'
            | '\u{ce6}'..='\u{cef}'
            | '\u{d66}'..='\u{d6f}'
            | '\u{de6}'..='\u{def}'
            | '\u{e50}'..='\u{e59}'
            | '\u{ed0}'..='\u{ed9}'
            | '\u{f20}'..='\u{f29}'
            | '\u{1040}'..='\u{1049}'
            | '\u{1090}'..='\u{1099}'
            | '\u{1369}'..='\u{1371}'
            | '\u{17e0}'..='\u{17e9}'
            | '\u{1810}'..='\u{1819}'
            | '\u{1946}'..='\u{194f}'
            | '\u{19d0}'..='\u{19da}'
            | '\u{1a80}'..='\u{1a89}'
            | '\u{1a90}'..='\u{1a99}'
            | '\u{1b50}'..='\u{1b59}'
            | '\u{1bb0}'..='\u{1bb9}'
            | '\u{1c40}'..='\u{1c49}'
            | '\u{1c50}'..='\u{1c59}'
            | '\u{2070}'
            | '\u{2074}'..='\u{2079}'
            | '\u{2080}'..='\u{2089}'
            | '\u{2460}'..='\u{2468}'
            | '\u{2474}'..='\u{247c}'
            | '\u{2488}'..='\u{2490}'
            | '\u{24ea}'
            | '\u{24f5}'..='\u{24fd}'
            | '\u{24ff}'
            | '\u{2776}'..='\u{277e}'
            | '\u{2780}'..='\u{2788}'
            | '\u{278a}'..='\u{2792}'
            | '\u{a620}'..='\u{a629}'
            | '\u{a8d0}'..='\u{a8d9}'
            | '\u{a900}'..='\u{a909}'
            | '\u{a9d0}'..='\u{a9d9}'
            | '\u{a9f0}'..='\u{a9f9}'
            | '\u{aa50}'..='\u{aa59}'
            | '\u{abf0}'..='\u{abf9}'
            | '\u{ff10}'..='\u{ff19}'
            | '\u{104a0}'..='\u{104a9}'
            | '\u{10a40}'..='\u{10a43}'
            | '\u{10d30}'..='\u{10d39}'
            | '\u{10e60}'..='\u{10e68}'
            | '\u{11052}'..='\u{1105a}'
            | '\u{11066}'..='\u{1106f}'
            | '\u{110f0}'..='\u{110f9}'
            | '\u{11136}'..='\u{1113f}'
            | '\u{111d0}'..='\u{111d9}'
            | '\u{112f0}'..='\u{112f9}'
            | '\u{11450}'..='\u{11459}'
            | '\u{114d0}'..='\u{114d9}'
            | '\u{11650}'..='\u{11659}'
            | '\u{116c0}'..='\u{116c9}'
            | '\u{11730}'..='\u{11739}'
            | '\u{118e0}'..='\u{118e9}'
            | '\u{11950}'..='\u{11959}'
            | '\u{11c50}'..='\u{11c59}'
            | '\u{11d50}'..='\u{11d59}'
            | '\u{11da0}'..='\u{11da9}'
            | '\u{11f50}'..='\u{11f59}'
            | '\u{16a60}'..='\u{16a69}'
            | '\u{16ac0}'..='\u{16ac9}'
            | '\u{16b50}'..='\u{16b59}'
            | '\u{1d7ce}'..='\u{1d7ff}'
            | '\u{1e140}'..='\u{1e149}'
            | '\u{1e2f0}'..='\u{1e2f9}'
            | '\u{1e4f0}'..='\u{1e4f9}'
            | '\u{1e950}'..='\u{1e959}'
            | '\u{1f100}'..='\u{1f10a}'
            | '\u{1fbf0}'..='\u{1fbf9}'
    )
}

fn python_whitespace(ch: char) -> bool {
    ch.is_whitespace() || ('\u{001c}'..='\u{001f}').contains(&ch)
}

fn title_key(text: &str) -> String {
    text.chars()
        .flat_map(char::to_uppercase)
        .filter(|ch| python_isalnum(*ch))
        .collect()
}

fn python_words(text: &str) -> Vec<&str> {
    text.split(python_whitespace)
        .filter(|word| !word.is_empty())
        .collect()
}

fn lookup_title(key: &str) -> Option<&'static str> {
    for role in ROLES {
        if title_key(role) == key || title_key(role_code(role)) == key {
            return Some(role);
        }
    }
    let direct = match key {
        "FL" => "L",
        "FR" => "R",
        "FC" | "CEN" | "CENTER" => "C",
        "SL" | "LSS" | "SIDEL" => "Ls",
        "SR" | "RSS" | "SIDER" => "Rs",
        "SBL" | "LSB" | "BL" => "Lb",
        "SBR" | "RSB" | "BR" => "Rb",
        "FWL" => "Lw",
        "FWR" => "Rw",
        "TFL" => "Tfl",
        "TFR" => "Tfr",
        "TML" => "Tml",
        "TMR" => "Tmr",
        "TRL" | "TBL" => "Tbl",
        "TRR" | "TBR" => "Tbr",
        "FHL" => "Hfl",
        "FHR" => "Hfr",
        "RHL" | "HRL" => "Hbl",
        "RHR" | "HRR" => "Hbr",
        "SUB" | "SW" | "LFE" | "SUBWOOFER" | "서브" | "서브우퍼" => "SW1",
        _ => {
            for prefix in ["LFE", "SUB", "SUBWOOFER", "서브", "서브우퍼"] {
                if let Some(number) = key.strip_prefix(prefix) {
                    for (index, role) in SUB_ROLES.into_iter().enumerate() {
                        if number == (index + 1).to_string() {
                            return Some(role);
                        }
                    }
                }
            }
            return None;
        }
    };
    Some(direct)
}

pub fn role_from_title(title: &str, roles: Option<&[&str]>) -> Option<&'static str> {
    let mut words = python_words(title);
    if words.is_empty() {
        return None;
    }
    let mut role = lookup_title(&title_key(&words.join(" ")));
    while role.is_none()
        && words.len() > 1
        && words
            .last()
            .is_some_and(|word| !word.is_empty() && word.chars().all(python_isdigit))
    {
        words.pop();
        role = lookup_title(&title_key(&words.join(" ")));
    }
    let role = role?;
    if let Some(allowed) = roles
        && !allowed
            .iter()
            .filter_map(|item| item.split(':').next())
            .any(|base| base == role)
    {
        return None;
    }
    Some(role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vocabulary_is_complete() {
        assert_eq!(ROLES.len(), 27);
        assert_eq!(role_group("SW8"), "sub");
        assert_eq!(role_order("L:LF"), 0);
        assert_eq!(role_from_title("LFE 2", None), Some("SW2"));
        assert_eq!(role_from_title("L １", None), Some("L"));
        assert_eq!(role_from_title("L ١", None), Some("L"));
        assert_eq!(role_from_title("L १", None), Some("L"));
        assert_eq!(role_from_title("L ፩", None), Some("L"));
        assert_eq!(role_from_title("L ²", None), Some("L"));
        assert_eq!(role_from_title("L ⑴", None), Some("L"));
        assert_eq!(role_from_title("L ½", None), None);
        assert_eq!(role_from_title("L Ⅻ", None), None);
        assert_eq!(role_from_title("L\u{0903}", None), Some("L"));
        assert_eq!(role_from_title("L\u{001f}1", None), Some("L"));
        assert_eq!(role_from_title("SUB01", None), None);
        assert_eq!(role_from_title("서브우퍼02", None), None);
        assert_eq!(role_from_title("DBA L", None), None);
    }
}

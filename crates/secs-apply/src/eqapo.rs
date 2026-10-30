pub const MARKER: &str = "# SECS2R: managed by SECS 2R (this line and the next)";
pub const INCLUDE: &str = r"Include: SECS2R\SECS.txt";
pub const FOLDER: &str = "SECS2R";
pub const BACKUP: &str = "config.txt.secs2r-backup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeState {
    Absent,
    Present,
    Damaged,
}

#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    raw: &'a str,
    content: &'a str,
}

fn split_bom(text: &str) -> (&str, &str) {
    text.strip_prefix('\u{feff}')
        .map_or(("", text), |rest| ("\u{feff}", rest))
}

fn lines(text: &str) -> Vec<Line<'_>> {
    text.split_inclusive('\n')
        .map(|raw| {
            let content = raw
                .strip_suffix("\r\n")
                .or_else(|| raw.strip_suffix('\n'))
                .unwrap_or(raw);
            Line { raw, content }
        })
        .collect()
}

fn is_marker(line: &Line<'_>) -> bool {
    line.content.trim_end() == MARKER
}

fn is_include(line: &Line<'_>) -> bool {
    line.content == INCLUDE
}

fn without_managed_lines(text: &str) -> Option<String> {
    let (bom, body) = split_bom(text);
    let lines = lines(body);
    let mut changed = false;
    let mut result = String::with_capacity(text.len());
    result.push_str(bom);

    let mut index = 0;
    while index < lines.len() {
        if is_marker(&lines[index]) {
            changed = true;
            index += 1;
            if let Some(include) = lines.get(index).filter(|line| is_include(line)) {
                index += 1;
                // An unterminated include ends the file: `with_include` ended
                // the original's last line for it, so that ending goes too.
                if include.raw == include.content && index == lines.len() {
                    let end = result
                        .strip_suffix("\r\n")
                        .or_else(|| result.strip_suffix('\n'));
                    if let Some(end) = end.filter(|end| end.len() > bom.len()) {
                        result.truncate(end.len());
                    }
                }
            }
        } else {
            result.push_str(lines[index].raw);
            index += 1;
        }
    }

    changed.then_some(result)
}

fn line_ending(text: &str) -> &'static str {
    text.find('\n').map_or("\n", |index| {
        if text.as_bytes().get(index.wrapping_sub(1)) == Some(&b'\r') {
            "\r\n"
        } else {
            "\n"
        }
    })
}

pub fn include_state(config_txt: &str) -> IncludeState {
    let (_, body) = split_bom(config_txt);
    let lines = lines(body);
    let marker_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| is_marker(line).then_some(index))
        .collect();

    match marker_indices.as_slice() {
        [] => IncludeState::Absent,
        [index] if lines.get(index + 1).is_some_and(is_include) => IncludeState::Present,
        _ => IncludeState::Damaged,
    }
}

/// Adds or repairs the managed marker and include lines.
pub fn with_include(config_txt: &str) -> Option<String> {
    if include_state(config_txt) == IncludeState::Present {
        return None;
    }

    let ending = line_ending(config_txt);
    let mut result = without_managed_lines(config_txt).unwrap_or_else(|| config_txt.to_owned());
    let body_is_empty = split_bom(&result).1.is_empty();
    // A file whose last line has no ending (Equalizer APO's own default) gets
    // the ending before the pair and none after it, so removing the pair
    // restores every original byte.
    let unterminated = !body_is_empty && !result.ends_with('\n');
    if unterminated {
        result.push_str(ending);
    }
    result.push_str(MARKER);
    result.push_str(ending);
    result.push_str(INCLUDE);
    if !unterminated {
        result.push_str(ending);
    }
    Some(result)
}

/// Removes marker lines and the managed include immediately following each one.
pub fn without_include(config_txt: &str) -> Option<String> {
    without_managed_lines(config_txt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(ending: &str) -> String {
        format!("{MARKER}{ending}{INCLUDE}{ending}")
    }

    #[test]
    fn eqapo_config() {
        assert_eq!(include_state("Preamp: -3 dB\n"), IncludeState::Absent);
        assert_eq!(
            with_include("Preamp: -3 dB\n"),
            Some(format!("Preamp: -3 dB\n{}", pair("\n")))
        );
        assert_eq!(
            with_include("Preamp: -3 dB\r\n"),
            Some(format!("Preamp: -3 dB\r\n{}", pair("\r\n")))
        );
        assert_eq!(
            with_include("\u{feff}Preamp: -3 dB\n"),
            Some(format!("\u{feff}Preamp: -3 dB\n{}", pair("\n")))
        );
        assert_eq!(
            with_include("Preamp: -3 dB"),
            Some(format!("Preamp: -3 dB\n{MARKER}\n{INCLUDE}"))
        );
        assert_eq!(
            with_include("Preamp: -3 dB\r\nInclude: example.txt"),
            Some(format!(
                "Preamp: -3 dB\r\nInclude: example.txt\r\n{MARKER}\r\n{INCLUDE}"
            ))
        );
    }

    #[test]
    fn intact_pair_is_present_and_unchanged() {
        let config = format!("Preamp: -3 dB\n{}", pair("\n"));
        assert_eq!(include_state(&config), IncludeState::Present);
        assert_eq!(with_include(&config), None);

        let trailing_marker_space = format!("{MARKER}  \t\n{INCLUDE}\n");
        assert_eq!(include_state(&trailing_marker_space), IncludeState::Present);
    }

    #[test]
    fn damaged_pairs_are_repaired_once() {
        for damaged in [
            format!("alpha\n{MARKER}\nomega\n"),
            format!("alpha\n{MARKER}\n{INCLUDE} \n"),
            format!("{MARKER}\n{INCLUDE}\n{MARKER}\n{INCLUDE}\n"),
        ] {
            assert_eq!(include_state(&damaged), IncludeState::Damaged);
            let repaired = with_include(&damaged).expect("damaged text must be repaired");
            assert_eq!(include_state(&repaired), IncludeState::Present);
            assert_eq!(repaired.matches(MARKER).count(), 1);
        }
    }

    #[test]
    fn removal_preserves_other_bytes_and_handwritten_include() {
        let handwritten = format!("Preamp: -3 dB  \r\n{INCLUDE}\r\n");
        assert_eq!(without_include(&handwritten), None);

        let managed = format!("\u{feff}{handwritten}{MARKER}\r\n{INCLUDE}\r\n");
        assert_eq!(
            without_include(&managed),
            Some(format!("\u{feff}{handwritten}"))
        );

        let damaged = format!("one\n{MARKER}\nkeep me\n");
        assert_eq!(without_include(&damaged), Some("one\nkeep me\n".into()));
    }

    #[test]
    fn adding_then_removing_round_trips_every_byte() {
        for original in [
            "",
            "one\n",
            "one\r\n",
            "\u{feff}one\n",
            "one",
            "one\r\ntwo",
            "\u{feff}one",
            "\u{feff}",
        ] {
            let added = with_include(original).expect("pair is initially absent");
            assert_eq!(include_state(&added), IncludeState::Present);
            assert_eq!(without_include(&added).as_deref(), Some(original));
        }
    }
}

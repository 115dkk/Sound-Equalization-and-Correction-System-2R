#![forbid(unsafe_code)]

//! Speaker presets SECS 2R adds to upstream's `model.PRESETS` (docs/DEVIATIONS.md U13).
//! Upstream has no golden for them; they follow its 5.1 preset and `default_bands`.

use secs_project::{Project, SpeakerConfig, loads};

/// A speaker as (role, size, [(band, output)]).
type Row = (String, String, Vec<(String, i64)>);

fn layout(config: &SpeakerConfig) -> Vec<Row> {
    config
        .speakers
        .iter()
        .map(|speaker| {
            (
                speaker.role.clone(),
                speaker.size.clone(),
                speaker
                    .bands
                    .iter()
                    .map(|band| (band.band.clone(), band.out))
                    .collect(),
            )
        })
        .collect()
}

fn row(role: &str, size: &str, bands: &[(&str, i64)]) -> Row {
    (
        role.into(),
        size.into(),
        bands
            .iter()
            .map(|(band, out)| ((*band).into(), *out))
            .collect(),
    )
}

#[test]
fn quad_presets_are_5_1_without_the_centre() {
    let surround = SpeakerConfig::from_preset("5.1", "", false).unwrap();
    for (name, subs) in [("4.0", 0), ("4.1", 1), ("4.2", 2)] {
        let quad = SpeakerConfig::from_preset(name, "dev", false).unwrap();
        assert_eq!(quad.preset, name);
        assert_eq!(quad.device, "dev");
        assert_eq!(quad.sub_count(), subs, "{name}");
        let mut expected: Vec<&str> = surround
            .roles()
            .into_iter()
            .filter(|role| *role != "C" && *role != "SW1")
            .collect();
        let sw: Vec<String> = (1..=subs).map(|n| format!("SW{n}")).collect();
        for (i, role) in sw.iter().enumerate() {
            expected.insert(2 + i, role);
        }
        assert_eq!(quad.roles(), expected, "{name}");
    }

    assert_eq!(
        layout(&SpeakerConfig::from_preset("4.0", "", false).unwrap()),
        [
            row("L", "small", &[("FULL", 1)]),
            row("R", "small", &[("FULL", 2)]),
            row("Ls", "small", &[("FULL", 3)]),
            row("Rs", "small", &[("FULL", 4)]),
        ]
    );
    assert_eq!(
        layout(&SpeakerConfig::from_preset("4.2", "", true).unwrap()),
        [
            row("L", "small", &[("LF", 1), ("HF", 2)]),
            row("R", "small", &[("LF", 3), ("HF", 4)]),
            row("SW1", "sub", &[("SUB", 5)]),
            row("SW2", "sub", &[("SUB", 6)]),
            row("Ls", "small", &[("LF", 7), ("HF", 8)]),
            row("Rs", "small", &[("LF", 9), ("HF", 10)]),
        ]
    );
}

#[test]
fn quad_presets_survive_save_and_reopen() {
    for name in ["4.0", "4.1", "4.2"] {
        let project = Project::from_preset(name).unwrap();
        assert!(project.config.validate().is_empty(), "{name}");
        let reopened = Project::from_json(loads(&project.save_text()).unwrap()).unwrap();
        assert_eq!(reopened.config, project.config, "{name}");
        assert_eq!(reopened.save_text(), project.save_text(), "{name}");
    }
    assert!(SpeakerConfig::from_preset("4.3", "", false).is_err());
}

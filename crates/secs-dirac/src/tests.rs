use super::*;
use test_support::{
    SyntheticChannel, SyntheticProject, bytes, envelope, fixed32, header, integer, write,
};

const SEED: &[u8] = include_bytes!("../tests/data/minimal.liveproject");

fn parse_default(bytes: &[u8]) -> Result<LiveProject, DiracError> {
    parse(bytes, &Limits::default())
}

fn session(file: &[u8]) -> &[u8] {
    let mut cursor = Cursor::new(file);
    cursor.u32("version").unwrap();
    for _ in 0..6 {
        cursor.skip_string(&Limits::default()).unwrap();
    }
    cursor.u32("flags").unwrap();
    assert_eq!(cursor.byte("presence").unwrap(), 1);
    let length = cursor.u32("length").unwrap() as usize;
    cursor.take(length, "session").unwrap()
}

fn one_entry(scale: Option<f32>, codes: Option<&[u8]>) -> Vec<u8> {
    let mut entry = Vec::new();
    if let Some(scale) = scale {
        fixed32(&mut entry, 5, scale);
    }
    if let Some(codes) = codes {
        bytes(&mut entry, 6, codes);
    }
    entry
}

fn set_with(dimensions: [Option<u64>; 3], entries: &[Vec<u8>]) -> Vec<u8> {
    let mut set = Vec::new();
    for (field, value) in (1..=3).zip(dimensions) {
        if let Some(value) = value {
            integer(&mut set, field, value);
        }
    }
    for entry in entries {
        bytes(&mut set, 4, entry);
    }
    set
}

fn single_set(entry: Vec<u8>) -> Vec<u8> {
    set_with([Some(48000), Some(1), Some(1)], &[entry])
}

fn wrap_set(set: &[u8]) -> Vec<u8> {
    let mut session = Vec::new();
    bytes(&mut session, 12, set);
    envelope(&SyntheticProject::minimal(), &session)
}

fn valid_entry() -> Vec<u8> {
    one_entry(Some(0.001), Some(&[0x7f, 0xff]))
}

fn assert_same_sample(actual: f64, expected: f64) {
    if cfg!(miri) {
        // Miri deliberately perturbs powf, even for identical arguments.
        // Keep exact reproducibility assertions on native runs; in Miri check
        // the result within the injected error (including cancellation at 0).
        assert!((actual - expected).abs() <= 1e-12 * (1.0 + expected.abs()));
    } else {
        assert_eq!(actual, expected);
    }
}

#[test]
fn decodes_mu_law_codes_symmetrically_and_monotonically() {
    let scale = 0.125f32;
    assert_eq!(decode_code(0, scale), 0.0);
    assert_eq!(
        decode_code(32767, scale),
        f64::from(scale) * CODE_FULL_SCALE
    );
    for code in 1..=i16::MAX {
        assert_same_sample(decode_code(-code, scale), -decode_code(code, scale));
    }
    let mut previous = decode_code(i16::MIN, scale);
    assert!(previous.is_finite());
    assert!(previous < -f64::from(scale) * CODE_FULL_SCALE);
    for code in (i32::from(i16::MIN) + 1)..=i32::from(i16::MAX) {
        let value = decode_code(code as i16, scale);
        assert!(value > previous);
        previous = value;
    }
}

#[test]
fn synthetic_project_round_trips_within_code_precision() {
    let mut source = SyntheticProject::minimal();
    source.format_version = u32::MAX;
    source.name = "합성 방 𐐀".into();
    source.microphone = "측정 마이크".into();
    source.sample_rate = 96000;
    source.channels.push(SyntheticChannel {
        name: "Auxiliary".into(),
        kind: 7,
    });
    source.positions = 3;
    source.responses = (0..9)
        .map(|slot| match slot {
            4 => None,
            8 => Some(vec![0.0; 65]),
            _ => Some(
                (0..65)
                    .map(|i| {
                        let t = f64::from(i);
                        (t * 0.37).sin() * (-t / 20.0).exp() * f64::from(slot + 1)
                    })
                    .collect(),
            ),
        })
        .collect();
    let encoded = write(&source);
    let parsed = parse_default(&encoded).unwrap();
    let repeated = parse_default(&encoded).unwrap();
    if !cfg!(miri) {
        assert_eq!(repeated, parsed);
    }
    for (first, second) in parsed.measured().zip(repeated.measured()) {
        for (&a, &b) in first.samples.iter().zip(second.samples) {
            assert_same_sample(a, b);
        }
    }
    assert_eq!(parsed.format_version, source.format_version);
    assert_eq!(parsed.name, source.name);
    assert_eq!(parsed.application, "Live Desktop");
    assert_eq!(parsed.microphone, source.microphone);
    assert_eq!(parsed.sample_rate, source.sample_rate);
    assert_eq!(parsed.positions, 3);
    assert_eq!(
        parsed
            .channels
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["Left", "Subwoofer", "Auxiliary"]
    );
    assert_eq!(
        parsed.channels.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ChannelKind::Main,
            ChannelKind::Subwoofer,
            ChannelKind::Other(7)
        ]
    );
    for (index, response) in source.responses.iter().enumerate() {
        let actual = parsed.response(index / 3, index % 3);
        match response {
            None => assert_eq!(actual, None),
            Some(expected) => {
                let actual = actual.unwrap();
                assert_eq!(actual.len(), expected.len());
                let peak = expected.iter().map(|x| x.abs()).fold(0.0, f64::max);
                for (&a, &b) in actual.iter().zip(expected) {
                    assert!((a - b).abs() <= 1e-4 * b.abs() + 4e-7 * peak, "{a} != {b}");
                }
            }
        }
    }
    let order: Vec<_> = parsed
        .measured()
        .map(|m| {
            assert_eq!(Some(m.samples), parsed.response(m.position, m.channel));
            (m.position, m.channel)
        })
        .collect();
    assert_eq!(
        order,
        [
            (0, 0),
            (0, 1),
            (0, 2),
            (1, 0),
            (1, 2),
            (2, 0),
            (2, 1),
            (2, 2)
        ]
    );
    for (position, channel) in [(3, 0), (0, 3), (usize::MAX, 0), (0, usize::MAX)] {
        assert_eq!(parsed.response(position, channel), None);
    }
    let mut changed = parsed;
    changed.channels.clear();
    changed.positions = usize::MAX;
    assert_eq!(changed.measured().count(), 0);
    assert_eq!(changed.response(usize::MAX - 1, 0), None);
}

#[test]
fn committed_seed_matches_the_writer() {
    assert_eq!(SEED, write(&SyntheticProject::minimal()));
}

#[test]
fn every_truncation_of_the_seed_is_an_error() {
    let body = session(SEED);
    let end = header(&SyntheticProject::minimal()).len() + 5 + body.len();
    for length in 0..end {
        assert!(parse_default(&SEED[..length]).is_err(), "prefix {length}");
    }
    assert!(parse_default(SEED).is_ok());
    for length in end..=SEED.len() {
        assert!(
            parse_default(&SEED[..length]).is_ok(),
            "ignored tail {length}"
        );
    }
}

#[test]
fn malformed_protobuf_is_rejected() {
    let project = SyntheticProject::minimal();
    for bad in [
        vec![0, 0],
        vec![
            8, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0,
        ],
        vec![8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2],
    ] {
        assert!(matches!(
            parse_default(&envelope(&project, &bad)),
            Err(DiracError::Invalid(_))
        ));
    }
    for wire in [3, 4, 6, 7] {
        assert!(matches!(
            parse_default(&envelope(&project, &[8 | wire])),
            Err(DiracError::Invalid(_))
        ));
    }
    for bad in [
        vec![10, 5, 1],
        vec![0x62, 2, 0x22, 5],
        vec![8, 0x80],
        vec![9, 0],
        vec![13, 0],
    ] {
        assert!(matches!(
            parse_default(&envelope(&project, &bad)),
            Err(DiracError::Truncated(_))
        ));
    }
    assert!(matches!(
        parse_default(&wrap_set(&single_set(vec![0x32, 4]))),
        Err(DiracError::Truncated(_))
    ));
    // Correct wire encoding but wrong wire type for known fields.
    for bad in [vec![0x60, 1], vec![0xf0, 1, 1]] {
        assert!(matches!(
            parse_default(&envelope(&project, &bad)),
            Err(DiracError::Invalid(_))
        ));
    }
    assert!(matches!(
        parse_default(&wrap_set(&[0x0a, 0])),
        Err(DiracError::Invalid(_))
    ));
    assert!(matches!(
        parse_default(&wrap_set(&single_set(vec![0x28, 1]))),
        Err(DiracError::Invalid(_))
    ));
}

#[test]
fn ir_set_shape_is_validated() {
    for index in 0..3 {
        for invalid in [None, Some(0)] {
            let mut shape = [Some(48000), Some(1), Some(1)];
            shape[index] = invalid;
            assert!(matches!(
                parse_default(&wrap_set(&set_with(shape, &[valid_entry()]))),
                Err(DiracError::Invalid(_))
            ));
        }
    }
    assert!(matches!(
        parse_default(&wrap_set(&set_with(
            [Some(1_000_001), Some(1), Some(1)],
            &[valid_entry()]
        ))),
        Err(DiracError::Invalid(_))
    ));
    for entries in [vec![], vec![valid_entry(), valid_entry()]] {
        assert!(matches!(
            parse_default(&wrap_set(&set_with(
                [Some(48000), Some(1), Some(1)],
                &entries
            ))),
            Err(DiracError::Invalid(_))
        ));
    }
    let mut mismatched = valid_entry();
    integer(&mut mismatched, 1, 44100);
    let mut invalid_entries = vec![
        mismatched,
        one_entry(Some(1.0), Some(&[1])),
        one_entry(None, Some(&[0, 0])),
        one_entry(None, Some(&[])),
    ];
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        invalid_entries.push(one_entry(Some(scale), Some(&[0, 0])));
        invalid_entries.push(one_entry(Some(scale), None));
    }
    for entry in invalid_entries {
        assert!(matches!(
            parse_default(&wrap_set(&single_set(entry))),
            Err(DiracError::Invalid(_))
        ));
    }
    let mut duplicate = Vec::new();
    bytes(&mut duplicate, 12, &single_set(valid_entry()));
    bytes(&mut duplicate, 12, &single_set(valid_entry()));
    assert!(matches!(
        parse_default(&envelope(&SyntheticProject::minimal(), &duplicate)),
        Err(DiracError::Invalid(_))
    ));
    let huge = set_with([Some(48000), Some(u64::MAX), Some(u64::MAX)], &[]);
    let limits = Limits {
        max_channels: usize::MAX,
        max_positions: usize::MAX,
        ..Limits::default()
    };
    assert!(matches!(
        parse(&wrap_set(&huge), &limits),
        Err(DiracError::Limit(_))
    ));
}

#[test]
fn limits_are_enforced() {
    let defaults = Limits::default();
    assert_eq!(
        defaults,
        Limits {
            max_file_bytes: 256 * 1024 * 1024,
            max_string_bytes: 1024 * 1024,
            max_channels: 64,
            max_positions: 64,
            max_ir_samples: 1 << 20,
            max_total_samples: 1 << 24
        }
    );
    for limits in [
        Limits {
            max_file_bytes: SEED.len() - 1,
            ..defaults
        },
        Limits {
            max_string_bytes: 8,
            ..defaults
        },
        Limits {
            max_channels: 1,
            ..defaults
        },
        Limits {
            max_positions: 1,
            ..defaults
        },
        Limits {
            max_ir_samples: 63,
            ..defaults
        },
        Limits {
            max_total_samples: 191,
            ..defaults
        },
    ] {
        assert!(
            matches!(parse(SEED, &limits), Err(DiracError::Limit(_))),
            "{limits:?}"
        );
    }
    let exact = Limits {
        max_file_bytes: SEED.len(),
        max_string_bytes: 72,
        max_channels: 2,
        max_positions: 2,
        max_ir_samples: 64,
        max_total_samples: 192,
    };
    assert!(parse(SEED, &exact).is_ok());
    let mut source = SyntheticProject::minimal();
    source.channels[0].name = "n".repeat(73);
    assert!(matches!(
        parse(
            &write(&source),
            &Limits {
                max_string_bytes: 72,
                ..defaults
            }
        ),
        Err(DiracError::Limit(_))
    ));
    // The UUID is ignored, but its declared string length is still bounded.
    let mut source = SyntheticProject::minimal();
    source.name.clear();
    source.microphone.clear();
    assert!(matches!(
        parse(
            &write(&source),
            &Limits {
                max_string_bytes: 71,
                ..defaults
            }
        ),
        Err(DiracError::Limit(_))
    ));
}

#[test]
fn no_measurements_is_its_own_error() {
    let mut project = SyntheticProject::minimal();
    let mut absent = header(&project);
    absent.push(0);
    assert_eq!(parse_default(&absent), Err(DiracError::NoMeasurements));
    let mut null = header(&project);
    null.push(1);
    null.extend_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(parse_default(&null), Err(DiracError::NoMeasurements));
    assert_eq!(
        parse_default(&envelope(&project, &[])),
        Err(DiracError::NoMeasurements)
    );
    project.responses.fill(None);
    assert_eq!(
        parse_default(&write(&project)),
        Err(DiracError::NoMeasurements)
    );
    assert_eq!(
        parse_default(&wrap_set(&single_set(one_entry(Some(0.0), Some(&[]))))),
        Err(DiracError::NoMeasurements)
    );
    // Zero-valued samples are measurements, unlike an absent or empty array.
    assert_eq!(
        parse_default(&wrap_set(&single_set(one_entry(Some(0.0), Some(&[0, 0])))))
            .unwrap()
            .response(0, 0),
        Some(&[0.0][..])
    );
}

#[test]
fn channel_names_fall_back_when_the_list_does_not_match() {
    let set = single_set(valid_entry());
    assert_eq!(
        parse_default(&wrap_set(&set)).unwrap().channels,
        [Channel {
            name: "Channel 1".into(),
            kind: ChannelKind::Other(0)
        }]
    );
    let mut project = SyntheticProject::minimal();
    project.channels[0].name.clear();
    let parsed = parse_default(&write(&project)).unwrap();
    assert_eq!(
        parsed.channels[0],
        Channel {
            name: "Channel 1".into(),
            kind: ChannelKind::Main
        }
    );
    project.channels[0].name = "Repeated".into();
    project.channels[1].name = "Repeated".into();
    let encoded = write(&project);
    let parsed = parse_default(&encoded).unwrap();
    assert_eq!(parsed.channels[0].name, parsed.channels[1].name);
    let mut extra = session(&encoded).to_vec();
    bytes(&mut extra, 30, &[]);
    let parsed = parse_default(&envelope(&project, &extra)).unwrap();
    assert_eq!(
        parsed.channels,
        [
            Channel {
                name: "Channel 1".into(),
                kind: ChannelKind::Other(0)
            },
            Channel {
                name: "Channel 2".into(),
                kind: ChannelKind::Other(0)
            }
        ]
    );
}

#[test]
fn header_strings_and_presence_are_validated() {
    let project = SyntheticProject::minimal();
    let mut bad = header(&project);
    bad.push(2);
    assert!(matches!(parse_default(&bad), Err(DiracError::Invalid(_))));
    let mut odd = SEED.to_vec();
    odd[4..8].copy_from_slice(&1u32.to_be_bytes());
    assert!(matches!(parse_default(&odd), Err(DiracError::Invalid(_))));
    let mut lossy = SEED.to_vec();
    lossy[8..10].copy_from_slice(&0xd800u16.to_be_bytes());
    assert!(parse_default(&lossy).unwrap().name.starts_with('\u{fffd}'));
    let mut empty = project;
    empty.name.clear();
    let mut null = write(&empty);
    null[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(parse_default(&null).unwrap().name, "");
}

#[test]
fn unknown_fields_are_skipped_at_every_level_and_singular_fields_use_last_value() {
    let unknown = [
        0x80, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x89, 0x01, 0, 1,
        2, 3, 4, 5, 6, 7, 0x92, 0x01, 2, 0xff, 0xff, 0x9d, 0x01, 0, 0, 0, 0,
    ];
    let mut entry = unknown.to_vec();
    entry.extend(valid_entry());
    fixed32(&mut entry, 5, 0.0);
    let mut set = unknown.to_vec();
    integer(&mut set, 1, 44100);
    set.extend(single_set(entry));
    let mut body = unknown.to_vec();
    bytes(&mut body, 12, &set);
    let mut channel = unknown.to_vec();
    bytes(&mut channel, 1, b"old");
    bytes(&mut channel, 1, &[0xff]);
    bytes(&mut body, 30, &channel);
    let parsed = parse_default(&envelope(&SyntheticProject::minimal(), &body)).unwrap();
    assert_eq!(parsed.sample_rate, 48000);
    assert_eq!(parsed.response(0, 0), Some(&[0.0][..]));
    assert_eq!(parsed.channels[0].name, "\u{fffd}");
    assert_eq!(parsed.channels[0].kind, ChannelKind::Other(0));
}

#[test]
#[ignore = "requires private projects in SECS_DIRAC_SAMPLES; never commit these files"]
fn parses_dirac_live_sample_projects() {
    let Some(root) = std::env::var_os("SECS_DIRAC_SAMPLES") else {
        eprintln!("SECS_DIRAC_SAMPLES is unset; skipping private projects");
        return;
    };
    let mut directories = vec![std::path::PathBuf::from(root)];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() && entry.path().extension().is_some_and(|x| x == "liveproject")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no liveproject files found");
    for path in &files {
        let file = std::fs::read(path).unwrap();
        let project = parse_default(&file).unwrap();
        assert_eq!(project.sample_rate, 48000);
        let mut session_reader = Reader::new(session(&file));
        let mut scales = Vec::new();
        while let Some((field, value)) = session_reader.next().unwrap() {
            if field != 12 {
                continue;
            }
            let mut set_reader = Reader::new(value.bytes().unwrap());
            while let Some((field, value)) = set_reader.next().unwrap() {
                if field != 4 {
                    continue;
                }
                let mut entry_reader = Reader::new(value.bytes().unwrap());
                let mut scale = None;
                let mut measured = false;
                while let Some((field, value)) = entry_reader.next().unwrap() {
                    match field {
                        5 => scale = Some(value.float().unwrap()),
                        6 => measured = !value.bytes().unwrap().is_empty(),
                        _ => {}
                    }
                }
                if measured {
                    scales.push(scale.unwrap());
                }
            }
        }
        assert_eq!(scales.len(), project.measured().count());
        for (ir, scale) in project.measured().zip(scales) {
            assert_eq!(ir.samples.len(), 32769);
            assert!(ir.samples.iter().all(|sample| sample.is_finite()));
            let (index, peak) = ir
                .samples
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap();
            let expected = decode_code(i16::MAX, scale);
            assert_eq!(expected, f64::from(scale) * CODE_FULL_SCALE);
            assert!((peak.abs() - expected).abs() <= f64::EPSILON * expected.abs());
            if ir.channel == 0 {
                assert!((10_000..13_000).contains(&index));
            }
        }
        eprintln!(
            "{:?}: {} measured IRs",
            path.file_name().unwrap(),
            project.measured().count()
        );
    }
    eprintln!("Parsed {} private projects", files.len());
}

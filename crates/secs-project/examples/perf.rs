//! Release project open/save benchmark; requires SECS_PERF_PROJECT.
#![forbid(unsafe_code)]

use secs_project::{Band, Project, PyJson, load_ir};
use std::{hint::black_box, path::PathBuf, time::Instant};

fn single_amp(project: &mut Project) {
    project.config.biamp_default = false;
    for speaker in &mut project.config.speakers {
        if speaker.bands.len() <= 1 {
            continue;
        }
        let keep = speaker
            .bands
            .iter()
            .find(|band| band.band == "LF")
            .unwrap_or(&speaker.bands[0]);
        speaker.bands = vec![Band {
            band: if speaker.role.starts_with("SW") {
                "SUB"
            } else {
                "FULL"
            }
            .into(),
            out: keep.out,
            pol: keep.pol.clone(),
        }];
    }
}

fn records(project: &Project) -> Vec<(i64, String, f64, Option<f64>)> {
    let PyJson::Array(rows) = &project.measurements else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in rows {
        let PyJson::Object(fields) = row else {
            continue;
        };
        let get = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        };
        let pos = match get("pos") {
            Some(PyJson::Int(value)) => value.parse().unwrap(),
            Some(PyJson::Float(value)) => *value as i64,
            _ => continue,
        };
        let origins: &[(String, PyJson)] = match get("t0") {
            Some(PyJson::Object(value)) => value,
            _ => &[],
        };
        let offsets: &[(String, PyJson)] = match get("spl_offset") {
            Some(PyJson::Object(value)) => value,
            _ => &[],
        };
        let PyJson::Object(irs) = get("ir").unwrap_or(&PyJson::Null) else {
            continue;
        };
        for (role, stored) in irs {
            if !matches!(stored, PyJson::Str(value) if !value.is_empty()) {
                continue;
            }
            let number = |values: &[(String, PyJson)], role: &str| {
                values.iter().find_map(|(key, value)| {
                    (key == role)
                        .then(|| match value {
                            PyJson::Float(v) => Some(*v),
                            PyJson::Int(v) => v.parse().ok(),
                            _ => None,
                        })
                        .flatten()
                })
            };
            out.push((
                pos,
                role.clone(),
                number(origins, role).unwrap_or(0.0),
                number(offsets, role),
            ));
        }
    }
    out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    out
}

fn load(path: &PathBuf) -> (Project, Vec<(Vec<f32>, u32)>) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut project = Project::load_text(&text).unwrap();
    single_amp(&mut project);
    let irs = records(&project)
        .into_iter()
        .map(|(pos, role, _, _)| load_ir(path, &role, pos).unwrap())
        .collect();
    (project, irs)
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let args: Vec<_> = std::env::args().collect();
    let op = args.get(1).map(String::as_str).unwrap_or("project_open");
    let root = PathBuf::from(std::env::var("SECS_PERF_PROJECT").expect("demo project directory"));
    let source = root.join("demo.secs");
    let target = root.join("rust-saved.secs");
    let save_project = if op == "project_save" {
        let text = std::fs::read_to_string(&source).unwrap();
        let mut project = Project::load_text(&text).unwrap();
        single_amp(&mut project);
        Some(project)
    } else {
        None
    };
    let call = || match op {
        "project_open" => {
            black_box(load(&source));
        }
        "project_save" => {
            let project = save_project.as_ref().unwrap();
            std::fs::write(&target, project.save_text()).unwrap();
            // The side .mdat SECS 2 writes on save (`_write_side_mdat`): every
            // IR read back, one measurement each, built and written in one go.
            let items: Vec<_> = records(project)
                .into_iter()
                .map(|(pos, role, t0, offset)| {
                    let (samples, rate) = load_ir(&source, &role, pos).unwrap();
                    secs_mdat::MeasurementInput {
                        title: format!("{role}_P{pos}"),
                        notes: format!("SECS · 위치 {pos} · 원 측정"),
                        sample_rate_hz: rate,
                        samples: samples.into_iter().map(f64::from).collect(),
                        start_time_s: t0,
                        spl_offset_db: offset,
                    }
                })
                .collect();
            let mut next = 0_u128;
            let bytes = secs_mdat::build(&items, secs_mdat::TEMPLATE, None, &mut || {
                next += 1;
                next
            })
            .unwrap();
            std::fs::write(target.with_extension("mdat"), bytes).unwrap();
        }
        _ => panic!("unknown operation: {op}"),
    };
    call();
    let runs = std::env::var("SECS_PERF_RUNS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(5);
    let mut samples = Vec::new();
    for _ in 0..runs {
        let start = Instant::now();
        call();
        samples.push(start.elapsed().as_secs_f64());
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"implementation":"rust","op":op,"case":"demo","samples_s":samples,"median_s":sorted[sorted.len()/2]})
    );
}

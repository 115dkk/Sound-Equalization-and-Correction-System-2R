use std::fs::File;
use std::io::Read;
use std::path::Path;

use secs_audio::{CalibrationError, CalibrationIssue, MicCalibration};
use secs_project::{MeasurementStore, Project, PyJson};
use secs_types::ipc::{
    ErrorCode, IpcError, MicCalibrationFile, MicCalibrationSetParams, MicCalibrationStatus,
};

use crate::measure::json::JsonExt;
use crate::measure::store_devices;
use crate::{Service, lock};

const MAX_CALIBRATION_BYTES: u64 = 4 * 1024 * 1024;

struct SelectedCalibration {
    text: String,
    name: String,
    path: String,
    curve: MicCalibration,
}

fn device_value<'a>(project: &'a Project, key: &str) -> Option<&'a str> {
    project
        .extra
        .iter()
        .find(|(name, _)| name == "devices")
        .and_then(|(_, value)| value.get(key))
        .and_then(PyJson::str)
}

fn selected_name(project: &Project, path: &str) -> String {
    device_value(project, "mic_calibration_name")
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "mic calibration".into())
}

fn issue_name(issue: CalibrationIssue) -> &'static str {
    match issue {
        CalibrationIssue::TooLarge => "too_large",
        CalibrationIssue::Header => "header",
        CalibrationIssue::Row => "row",
        CalibrationIssue::Order => "order",
        CalibrationIssue::Range => "range",
        CalibrationIssue::TooFewPoints => "too_few_points",
        CalibrationIssue::BothConventions => "both_conventions",
    }
}

fn input_error(
    file: &str,
    issue: &str,
    detail: impl Into<String>,
    line: Option<usize>,
) -> IpcError {
    let mut error = IpcError::new(ErrorCode::MeasurementInput, detail)
        .with_param("reason", "mic_calibration")
        .with_param("issue", issue)
        .with_param("file", file);
    if let Some(line) = line {
        error.params.insert("line".into(), line.to_string());
    }
    error
}

fn parse_error(file: &str, error: CalibrationError) -> IpcError {
    input_error(file, issue_name(error.issue), error.to_string(), error.line)
}

fn selected_calibration(project: &Project) -> Result<Option<SelectedCalibration>, IpcError> {
    let Some(text) = device_value(project, "mic_calibration") else {
        return Ok(None);
    };
    let path = device_value(project, "mic_calibration_path")
        .filter(|path| !path.is_empty())
        .or_else(|| device_value(project, "mic_calibration_name").filter(|name| !name.is_empty()))
        .unwrap_or("mic calibration")
        .to_owned();
    let name = selected_name(project, &path);
    let curve = MicCalibration::parse_checked(text).map_err(|error| parse_error(&name, error))?;
    Ok(Some(SelectedCalibration {
        text: text.to_owned(),
        name,
        path,
        curve,
    }))
}

fn file_status(selected: &SelectedCalibration) -> MicCalibrationFile {
    let points = selected.curve.points();
    MicCalibrationFile {
        name: selected.name.clone(),
        path: selected.path.clone(),
        serial: selected.curve.serial().map(str::to_owned),
        sens_factor_db: selected.curve.usb_sens_factor_db(),
        analog_gain_db: selected.curve.analog_gain_db(),
        sensitivity_dbfs: selected.curve.sensitivity_dbfs_at_94(),
        notes: selected.curve.notes().to_vec(),
        points: u32::try_from(points.len()).unwrap_or(u32::MAX),
        min_hz: points[0].frequency,
        max_hz: points[points.len() - 1].frequency,
        has_phase: selected.curve.has_phase(),
    }
}

fn status(
    store: &mut MeasurementStore,
    selected: Option<&SelectedCalibration>,
) -> MicCalibrationStatus {
    let mut attached = 0_u32;
    let mut attachable = 0_u32;
    for record in store.records() {
        for (role, path) in &record.ir {
            if path.is_empty() {
                continue;
            }
            let has_meter = store
                .source_of(role, record.pos)
                .as_ref()
                .is_some_and(|info| secs_project::attached_calibration(info).meter.is_some());
            if has_meter {
                attached = attached.saturating_add(1);
            } else if record.source.get(role).is_none_or(String::is_empty) {
                attachable = attachable.saturating_add(1);
            }
        }
    }
    MicCalibrationStatus {
        file: selected.map(file_status),
        attached,
        attachable,
    }
}

fn path_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_owned())
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    let text = if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let (pairs, remainder) = bytes.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("UTF-16 LE calibration has an odd byte count".into());
        }
        let units = pairs
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|error| error.to_string())?
    } else if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let (pairs, remainder) = bytes.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("UTF-16 BE calibration has an odd byte count".into());
        }
        let units = pairs
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|error| error.to_string())?
    } else {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        if bytes.contains(&0) {
            return Err("calibration contains a NUL byte".into());
        }
        std::str::from_utf8(bytes)
            .map_err(|error| error.to_string())?
            .to_owned()
    };
    if text.contains('\0') {
        Err("calibration contains a NUL character".into())
    } else {
        Ok(text)
    }
}

fn read_selected(path: &str) -> Result<SelectedCalibration, IpcError> {
    let name = path_file_name(path);
    let mut file = File::open(path)
        .map_err(|error| input_error(&name, "unreadable", error.to_string(), None))?;
    let length = file
        .metadata()
        .map_err(|error| input_error(&name, "unreadable", error.to_string(), None))?
        .len();
    if length > MAX_CALIBRATION_BYTES {
        return Err(input_error(
            &name,
            "too_large",
            format!("calibration file is {length} bytes"),
            None,
        ));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.by_ref()
        .take(MAX_CALIBRATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| input_error(&name, "unreadable", error.to_string(), None))?;
    if bytes.len() as u64 > MAX_CALIBRATION_BYTES {
        return Err(input_error(
            &name,
            "too_large",
            "calibration file exceeds 4 MiB",
            None,
        ));
    }
    let text =
        decode_text(&bytes).map_err(|detail| input_error(&name, "not_text", detail, None))?;
    let curve = MicCalibration::parse_checked(&text).map_err(|error| parse_error(&name, error))?;
    Ok(SelectedCalibration {
        text,
        name,
        path: path.to_owned(),
        curve,
    })
}

fn remove_selected_file(project: &mut Project) {
    let Some(devices) = project
        .extra
        .iter_mut()
        .find(|(name, _)| name == "devices")
        .and_then(|(_, value)| value.object_mut())
    else {
        return;
    };
    devices.retain(|(name, _)| {
        !matches!(
            name.as_str(),
            "mic_calibration" | "mic_calibration_name" | "mic_calibration_path"
        )
    });
}

impl Service {
    pub(crate) fn mic_calibration_get(&self) -> Result<MicCalibrationStatus, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let selected = selected_calibration(open.store.project())?;
        Ok(status(&mut open.store, selected.as_ref()))
    }

    pub(crate) fn mic_calibration_set(
        &self,
        params: MicCalibrationSetParams,
    ) -> Result<MicCalibrationStatus, IpcError> {
        {
            let session = lock(&self.session);
            session.require()?;
        }
        self.refuse_while_measuring()?;
        let selected = read_selected(&params.path)?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        store_devices(
            open.store.project_mut(),
            vec![
                ("mic_calibration".into(), PyJson::Str(selected.text.clone())),
                (
                    "mic_calibration_name".into(),
                    PyJson::Str(selected.name.clone()),
                ),
                (
                    "mic_calibration_path".into(),
                    PyJson::Str(selected.path.clone()),
                ),
            ],
        );
        open.dirty = true;
        Ok(status(&mut open.store, Some(&selected)))
    }

    pub(crate) fn mic_calibration_clear(&self) -> Result<MicCalibrationStatus, IpcError> {
        self.refuse_while_measuring()?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        remove_selected_file(open.store.project_mut());
        open.dirty = true;
        Ok(status(&mut open.store, None))
    }

    pub(crate) fn mic_calibration_attach(&self) -> Result<MicCalibrationStatus, IpcError> {
        self.refuse_while_measuring()?;
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        let Some(selected) = selected_calibration(open.store.project())? else {
            return Err(input_error(
                "",
                "no_file",
                "no microphone calibration file is selected",
                None,
            ));
        };
        let cal_file = secs_mdat::CalFile {
            path: selected.path.clone(),
            text: Some(selected.text.as_bytes().to_vec()),
        };
        let built = secs_mdat::from_rew_measurement(Some(&cal_file), "", "", "", 0)
            .map_err(|error| input_error(&selected.name, "row", error.to_string(), None))?
            .ok_or_else(|| {
                input_error(
                    &selected.name,
                    "row",
                    "calibration did not produce source information",
                    None,
                )
            })?;
        let meter = built
            .cal
            .into_iter()
            .find(|(name, _)| name == "meterCal")
            .map(|(_, cal)| cal)
            .ok_or_else(|| {
                input_error(
                    &selected.name,
                    "row",
                    "calibration did not produce meterCal",
                    None,
                )
            })?;

        let mut candidates = Vec::new();
        for record in open.store.records() {
            for (role, path) in &record.ir {
                if !path.is_empty()
                    && record.source.get(role).is_none_or(String::is_empty)
                    && !open
                        .store
                        .source_of(role, record.pos)
                        .as_ref()
                        .is_some_and(|info| {
                            secs_project::attached_calibration(info).meter.is_some()
                        })
                {
                    candidates.push((record.pos, role.clone()));
                }
            }
        }
        for (position, role) in candidates {
            let mut info = open.store.source_of(&role, position).unwrap_or_default();
            info.cal.retain(|(name, _)| name != "meterCal");
            info.cal.insert(0, ("meterCal".into(), meter.clone()));
            info.boxed.retain(|(name, _)| name != "usesEmbeddedCalData");
            info.boxed.push((
                "usesEmbeddedCalData".into(),
                secs_mdat::Boxed::Boolean(true),
            ));
            open.store
                .replace_source(&role, position, Some(&info))
                .map_err(crate::ServiceError::from)
                .map_err(IpcError::from)?;
        }
        Ok(status(&mut open.store, Some(&selected)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_mdat::{CalInfo, SourceInfo};
    use secs_project::MeasurementResult;
    use secs_types::ipc::{ProjectNewParams, Request};
    use std::path::PathBuf;
    use std::sync::Arc;

    const UMIK_90: &str = "\"Sens Factor =-.7312dB, SERNO: 7000001\"\r\n\
\"Auto-generated 90-degree calibration file for use with nanoAVR DL\"\r\n\
10.000000 -3.6\r\n1000.000000 0.1\r\n20000.000000 -5.5\r\n";

    fn root(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "secs-mic-calibration-{}-{name}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    fn service_with_project(name: &str) -> (Service, PathBuf) {
        let root = root(name);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("room.secs");
        let service = Service::new(crate::Deps::default());
        let response = service.call(
            Request::ProjectNew(ProjectNewParams {
                path: path.to_string_lossy().into_owned(),
                preset: "2.0".into(),
                sub_count: 0,
                sample: false,
                tutorial: false,
            }),
            Arc::new(crate::NoEvents),
        );
        assert!(matches!(response, secs_types::ipc::Envelope::Ok(_)));
        (service, root)
    }

    fn expect_issue(error: IpcError, issue: &str, file: &str, line: Option<&str>) {
        assert_eq!(error.code, ErrorCode::MeasurementInput);
        assert_eq!(
            error.params.get("reason").map(String::as_str),
            Some("mic_calibration")
        );
        assert_eq!(error.params.get("issue").map(String::as_str), Some(issue));
        assert_eq!(error.params.get("file").map(String::as_str), Some(file));
        assert_eq!(error.params.get("line").map(String::as_str), line);
        assert!(!error.detail.is_empty());
    }

    fn result(
        role: &str,
        position: i64,
        source: Option<&str>,
        source_info: Option<SourceInfo>,
    ) -> MeasurementResult {
        MeasurementResult {
            role: role.into(),
            pos: position,
            ir: vec![1.0, 0.0, -0.25, 0.0],
            sample_rate: 48_000,
            t0_s: 0.0,
            spl_db: None,
            spl_offset_db: None,
            snr_db: None,
            uuid: None,
            source: source.map(str::to_owned),
            source_info,
        }
    }

    fn meter(name: &str, gain: f32) -> SourceInfo {
        SourceInfo {
            cal: vec![(
                "meterCal".into(),
                CalInfo {
                    inverse_c_selected: false,
                    source_name: name.into(),
                    source_path: name.into(),
                    freq: Some(vec![20.0, 20_000.0]),
                    gain: Some(vec![gain, gain]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..SourceInfo::default()
        }
    }

    #[test]
    fn mic_calibration_set_get_clear_and_errors() {
        let (service, root) = service_with_project("set-get");
        let file = root.join("umik-1-90.txt");
        std::fs::write(&file, UMIK_90.as_bytes()).unwrap();
        let status = service
            .mic_calibration_set(MicCalibrationSetParams {
                path: file.to_string_lossy().into_owned(),
            })
            .unwrap();
        let selected = status.file.unwrap();
        assert_eq!(selected.name, "umik-1-90.txt");
        assert_eq!(selected.path, file.to_string_lossy());
        assert_eq!(selected.serial.as_deref(), Some("7000001"));
        assert_eq!(selected.sens_factor_db, Some(-0.7312));
        assert_eq!(selected.analog_gain_db, None);
        assert_eq!(selected.sensitivity_dbfs, None);
        assert_eq!(
            selected.notes,
            ["Auto-generated 90-degree calibration file for use with nanoAVR DL"]
        );
        assert_eq!(selected.points, 3);
        assert_eq!(selected.min_hz, 10.0);
        assert_eq!(selected.max_hz, 20_000.0);
        assert!(!selected.has_phase);
        assert!(lock(&service.session).require().unwrap().dirty);
        assert_eq!(service.mic_calibration_get().unwrap().file, Some(selected));

        lock(&service.session).require_mut().unwrap().dirty = false;
        let cleared = service.mic_calibration_clear().unwrap();
        assert!(cleared.file.is_none());
        assert!(lock(&service.session).require().unwrap().dirty);

        let missing = root.join("missing.cal");
        expect_issue(
            service
                .mic_calibration_set(MicCalibrationSetParams {
                    path: missing.to_string_lossy().into_owned(),
                })
                .unwrap_err(),
            "unreadable",
            "missing.cal",
            None,
        );

        let large = root.join("large.cal");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(MAX_CALIBRATION_BYTES + 1)
            .unwrap();
        expect_issue(
            service
                .mic_calibration_set(MicCalibrationSetParams {
                    path: large.to_string_lossy().into_owned(),
                })
                .unwrap_err(),
            "too_large",
            "large.cal",
            None,
        );

        let binary = root.join("binary.cal");
        std::fs::write(&binary, [0xff, 0x80]).unwrap();
        expect_issue(
            service
                .mic_calibration_set(MicCalibrationSetParams {
                    path: binary.to_string_lossy().into_owned(),
                })
                .unwrap_err(),
            "not_text",
            "binary.cal",
            None,
        );

        let bad_row = root.join("bad-row.cal");
        std::fs::write(&bad_row, b"20 0\n1000 nope\n").unwrap();
        expect_issue(
            service
                .mic_calibration_set(MicCalibrationSetParams {
                    path: bad_row.to_string_lossy().into_owned(),
                })
                .unwrap_err(),
            "row",
            "bad-row.cal",
            Some("2"),
        );

        let utf16 = root.join("utf16-le.cal");
        let mut bytes = vec![0xff, 0xfe];
        for unit in UMIK_90.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&utf16, bytes).unwrap();
        let decoded = service
            .mic_calibration_set(MicCalibrationSetParams {
                path: utf16.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert_eq!(decoded.file.unwrap().serial.as_deref(), Some("7000001"));
        let stored = device_value(
            lock(&service.session).require().unwrap().store.project(),
            "mic_calibration",
        )
        .unwrap()
        .to_owned();
        assert_eq!(stored, UMIK_90);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mic_calibration_attach_gives_the_file_to_sweeps_only() {
        let (service, root) = service_with_project("attach");
        let imported = SourceInfo {
            strings: vec![("sourceFileName".into(), "import.mdat".into())],
            ..SourceInfo::default()
        };
        {
            let mut session = lock(&service.session);
            let open = session.require_mut().unwrap();
            open.store.record(result("L", 1, None, None), None).unwrap();
            open.store
                .record(result("R", 1, None, Some(meter("rew-own.cal", 3.0))), None)
                .unwrap();
            open.store
                .record(
                    result("L", 2, Some("import.mdat"), Some(imported.clone())),
                    None,
                )
                .unwrap();
        }
        let file = root.join("project-mic.cal");
        std::fs::write(&file, UMIK_90.as_bytes()).unwrap();
        let initial = service
            .mic_calibration_set(MicCalibrationSetParams {
                path: file.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert_eq!((initial.attached, initial.attachable), (1, 1));
        let old_signature = {
            let mut session = lock(&service.session);
            let open = session.require_mut().unwrap();
            open.dirty = false;
            crate::calibration::signature_key(open.store.project())
        };

        let attached = service.mic_calibration_attach().unwrap();
        assert_eq!((attached.attached, attached.attachable), (2, 0));
        let mut session = lock(&service.session);
        let open = session.require_mut().unwrap();
        assert!(!open.dirty);
        assert_ne!(
            crate::calibration::signature_key(open.store.project()),
            old_signature
        );
        let bare = open.store.source_of("L", 1).unwrap();
        let bare_meter = secs_project::attached_calibration(&bare).meter.unwrap();
        assert_eq!(bare_meter.source_name, "project-mic.cal");
        assert_eq!(bare_meter.freq, Some(vec![10.0, 1000.0, 20_000.0]));
        assert_eq!(bare_meter.gain, Some(vec![-3.6, 0.1, -5.5]));
        assert_eq!(
            bare.boxed
                .iter()
                .filter(|(name, _)| name == "usesEmbeddedCalData")
                .collect::<Vec<_>>(),
            [&(
                "usesEmbeddedCalData".into(),
                secs_mdat::Boxed::Boolean(true)
            )]
        );
        let own = open.store.source_of("R", 1).unwrap();
        assert_eq!(
            secs_project::attached_calibration(&own)
                .meter
                .unwrap()
                .source_name,
            "rew-own.cal"
        );
        assert_eq!(open.store.source_of("L", 2), Some(imported));
        drop(session);

        let (without_file, empty_root) = service_with_project("no-file");
        expect_issue(
            without_file.mic_calibration_attach().unwrap_err(),
            "no_file",
            "",
            None,
        );
        std::fs::remove_dir_all(empty_root).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

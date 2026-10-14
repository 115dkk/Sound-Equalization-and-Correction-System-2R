use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use secs_project::{DspProfile, MeasurementStore, Project, PyJson, measured_positions};
use secs_types::ipc::{MigrationWarning, ProjectInfo};

use crate::{ServiceError, UuidSource, lock};

#[derive(Debug, Default)]
pub(crate) struct Session {
    open: Option<OpenProject>,
}

#[derive(Debug)]
pub(crate) struct OpenProject {
    pub path: PathBuf,
    pub store: MeasurementStore,
    pub dirty: bool,
    pub cached: Option<crate::cache::CachedResults>,
    pub identity: u64,
    pub undo: std::collections::VecDeque<(String, Vec<(usize, PyJson)>)>,
}

impl Session {
    pub fn create(
        &mut self,
        path: PathBuf,
        preset: &str,
        sub_count: u8,
        sample: bool,
    ) -> Result<ProjectInfo, ServiceError> {
        let selected_preset = if sample { "7.1.4" } else { preset };
        let mut project = Project::from_preset(selected_preset)?;
        project.name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("프로젝트")
            .to_owned();
        project.dsp = DspProfile::presets()
            .remove("camilla")
            .expect("the built-in CamillaDSP profile exists");
        let requested_subs = if sample { 3 } else { sub_count };
        if project.config.sub_count() != usize::from(requested_subs) {
            project.config.set_sub_count(i64::from(requested_subs));
        }
        if sample {
            let (subs, mut rest): (Vec<_>, Vec<_>) = project
                .config
                .speakers
                .drain(..)
                .partition(|speaker| speaker.role.starts_with("SW"));
            let at = rest
                .iter()
                .position(|speaker| matches!(speaker.role.as_str(), "Ls" | "Lb" | "Tfl"))
                .unwrap_or(rest.len());
            rest.splice(at..at, subs);
            project.config.speakers = rest;
            project.config.renumber();
            project.config.preset = "7.1.4".into();
            project
                .extra
                .push(("array".into(), PyJson::Int("1".into())));
        }
        project.curtain = (20.0, 20_000.0);
        if project.dsp.fir_taps_max > 0
            && let PyJson::Object(settings) = &mut project.bc
            && let Some((_, phase)) = settings.iter_mut().find(|(key, _)| key == "phase")
        {
            *phase = PyJson::Str("mixed".into());
        }
        let mut store = MeasurementStore::new(&path, project);
        store.save()?;
        self.open = Some(OpenProject {
            path,
            store,
            dirty: false,
            cached: None,
            identity: next_identity(),
            undo: Default::default(),
        });
        self.info()
    }

    pub fn open(&mut self, path: PathBuf) -> Result<ProjectInfo, ServiceError> {
        let store = MeasurementStore::open(&path)?;
        let cached = crate::cache::load(&path);
        self.open = Some(OpenProject {
            path,
            store,
            dirty: false,
            cached,
            identity: next_identity(),
            undo: Default::default(),
        });
        self.info()
    }

    pub fn save(
        &mut self,
        path: Option<PathBuf>,
        uuid: &UuidSource,
    ) -> Result<ProjectInfo, ServiceError> {
        let open = self.require_mut()?;
        if let Some(path) = path
            && path != open.path
        {
            carry_irs(&open.path, &path)?;
            rewrite_ir_directory(open.store.project_mut(), &open.path, &path);
            open.path = path.clone();
            open.store = MeasurementStore::new(path, open.store.project().clone());
            open.cached = None;
        }
        open.store.save()?;
        write_side_mdat(open, uuid)?;
        open.dirty = false;
        self.info()
    }

    pub fn info(&self) -> Result<ProjectInfo, ServiceError> {
        let open = self.require()?;
        Ok(info(open))
    }

    pub fn require(&self) -> Result<&OpenProject, ServiceError> {
        self.open.as_ref().ok_or(ServiceError::ProjectNotOpen)
    }
    pub fn require_mut(&mut self) -> Result<&mut OpenProject, ServiceError> {
        self.open.as_mut().ok_or(ServiceError::ProjectNotOpen)
    }
}

fn next_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn info(open: &OpenProject) -> ProjectInfo {
    let project = open.store.project();
    ProjectInfo {
        path: open.path.to_string_lossy().into_owned(),
        name: project.name.clone(),
        preset: project.config.preset.clone(),
        sub_count: project.config.sub_count(),
        positions: measured_positions(project),
        dirty: open.dirty,
        migration_warnings: open
            .store
            .warnings()
            .iter()
            .map(|warning| MigrationWarning {
                code: warning.code.into(),
                entry: warning.entry.clone(),
            })
            .collect(),
    }
}

fn carry_irs(old: &Path, new: &Path) -> Result<(), std::io::Error> {
    let source = old.with_extension("irs");
    let destination = new.with_extension("irs");
    if source == destination || !source.is_dir() {
        return Ok(());
    }
    copy_tree(&source, &destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn write_side_mdat(open: &mut OpenProject, uuid: &UuidSource) -> Result<(), ServiceError> {
    let records = open.store.records();
    let mut inputs = Vec::new();
    for record in records {
        for role in record.ir.keys() {
            let (samples, sample_rate) = open.store.ir_of(role, record.pos)?;
            inputs.push(secs_mdat::MeasurementInput {
                title: format!("{role} P{}", record.pos),
                notes: String::new(),
                sample_rate_hz: sample_rate,
                samples: samples.into_iter().map(f64::from).collect(),
                start_time_s: record.t0.get(role).copied().unwrap_or(0.0),
                spl_offset_db: record.spl_offset.get(role).copied(),
            });
        }
    }
    let destination = open.path.with_extension("mdat");
    if inputs.is_empty() {
        match fs::remove_file(destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    }
    let uuids = {
        let mut uuid = lock(uuid);
        (0..inputs.len())
            .map(|_| uuid.as_mut()())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ServiceError::Project(error.to_string()))?
    };
    let mut uuids = uuids.into_iter();
    // `secs_mdat::build` consumes exactly one UUID for each validated input.
    let bytes = secs_mdat::build(&inputs, secs_mdat::TEMPLATE, None, &mut || {
        uuids.next().unwrap_or_default()
    })
    .map_err(|error| ServiceError::Project(error.to_string()))?;
    atomic_replace(&destination, &bytes)?;
    Ok(())
}

fn atomic_replace(destination: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stem = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project.mdat");
    static TEMP: AtomicU64 = AtomicU64::new(0);
    let mut temporary = None;
    for _ in 0..100 {
        let id = TEMP.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{stem}.{}-{id}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                temporary = Some((path, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some((path, mut file)) = temporary else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create side mdat temporary file",
        ));
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&path, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

fn rewrite_ir_directory(project: &mut Project, old: &Path, new: &Path) {
    let old_irs = old.with_extension("irs");
    let new_irs = new.with_extension("irs");
    let Some(old_name) = old_irs.file_name().and_then(|v| v.to_str()) else {
        return;
    };
    let Some(new_name) = new_irs.file_name().and_then(|v| v.to_str()) else {
        return;
    };
    let PyJson::Array(records) = &mut project.measurements else {
        return;
    };
    for record in records {
        let PyJson::Object(fields) = record else {
            continue;
        };
        let Some((_, PyJson::Object(irs))) = fields.iter_mut().find(|(key, _)| key == "ir") else {
            continue;
        };
        for (_, value) in irs {
            let PyJson::Str(path) = value else { continue };
            if let Some(rest) = path
                .strip_prefix(old_name)
                .and_then(|value| value.strip_prefix(['/', '\\']))
            {
                *path = format!("{new_name}/{rest}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_project::MeasurementResult;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("secs-service-{name}-{}", std::process::id()))
    }

    #[test]
    fn project_session_create_open_save_and_save_as() {
        let root = root("session");
        let source = root.join("source.secs");
        let target = root.join("saved-as.secs");
        fs::create_dir_all(&root).unwrap();
        let mut session = Session::default();
        let created = session.create(source.clone(), "2.1", 1, false).unwrap();
        assert_eq!(created.path, source.to_string_lossy());
        assert_eq!(created.sub_count, 1);

        let roles: Vec<_> = session
            .require()
            .unwrap()
            .store
            .project()
            .config
            .speakers
            .iter()
            .map(|speaker| speaker.role.clone())
            .collect();
        let open = session.require_mut().unwrap();
        open.store.start_position(1);
        for role in &roles {
            open.store
                .record(
                    MeasurementResult {
                        role: role.clone(),
                        pos: 1,
                        ir: vec![0.0, 1.0, 0.0],
                        sample_rate: 48_000,
                        t0_s: 0.0,
                        spl_db: None,
                        spl_offset_db: None,
                        snr_db: Some(30.0),
                        uuid: Some(format!("{role}-1")),
                        source: Some("test".into()),
                    },
                    Some(&roles),
                )
                .unwrap();
        }
        open.dirty = true;
        let uuid = crate::Deps::default().uuid;
        let saved = session.save(Some(target.clone()), &uuid).unwrap();
        assert_eq!(saved.path, target.to_string_lossy());
        assert!(!saved.dirty);
        assert!(target.exists());
        assert!(target.with_extension("irs").is_dir());
        assert!(target.with_extension("mdat").exists());

        let reopened = session.open(target.clone()).unwrap();
        assert_eq!(reopened.positions, vec![1]);

        session.require_mut().unwrap().store.clear_position(1);
        session.save(None, &uuid).unwrap();
        assert!(!target.with_extension("mdat").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use secs_project::{
    DspProfile, MeasurementStore, Project, PyJson, load_project_sources, measured_positions,
    plan_from_project,
};
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
    pub tutorial: bool,
    pub cached: Option<crate::cache::CachedResults>,
    pub identity: u64,
    pub undo: std::collections::VecDeque<UndoEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct UndoEntry {
    pub token: String,
    pub payload: UndoPayload,
}

#[derive(Debug, Clone)]
pub(crate) enum UndoPayload {
    Position(Vec<(usize, PyJson)>),
    Measurements(PyJson),
}

impl Session {
    pub fn create(
        &mut self,
        path: PathBuf,
        preset: &str,
        sub_count: u8,
        sample: bool,
        tutorial: bool,
    ) -> Result<ProjectInfo, ServiceError> {
        let sample = sample || tutorial;
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
        crate::measure::store_devices(
            &mut project,
            vec![("method".into(), PyJson::Str("none".into()))],
        );
        let mut store = MeasurementStore::new(&path, project);
        if !tutorial {
            store.save()?;
        }
        self.open = Some(OpenProject {
            path,
            store,
            dirty: false,
            tutorial,
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
            tutorial: false,
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
        now_ms: i64,
    ) -> Result<ProjectInfo, ServiceError> {
        let open = self.require_mut()?;
        if open.tutorial {
            return Err(ServiceError::TutorialBlocked);
        }
        if let Some(path) = path
            && path != open.path
        {
            let previous_cached = open.cached.take();
            let next = prepare_save_as(open, &path);
            let next = match next {
                Ok(next) => next,
                Err(error) => {
                    open.cached = previous_cached;
                    return Err(error);
                }
            };
            open.path = path;
            open.store = next.finish();
            open.cached = crate::cache::load(&open.path).or(previous_cached);
        } else {
            open.store.save()?;
        }
        if let Err(error) = write_side_mdat(open, uuid, now_ms) {
            eprintln!("곁 mdat을 저장하지 못했습니다: {error}");
        }
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

pub(crate) fn push_position_undo(open: &mut OpenProject, removed: Vec<(usize, PyJson)>) -> String {
    push_undo(open, UndoPayload::Position(removed))
}

pub(crate) fn push_measurements_undo(open: &mut OpenProject, measurements: PyJson) -> String {
    push_undo(open, UndoPayload::Measurements(measurements))
}

fn push_undo(open: &mut OpenProject, payload: UndoPayload) -> String {
    static TOKEN: AtomicU64 = AtomicU64::new(1);
    let token = format!(
        "{}-{}",
        open.identity,
        TOKEN.fetch_add(1, Ordering::Relaxed)
    );
    open.undo.push_back(UndoEntry {
        token: token.clone(),
        payload,
    });
    if open.undo.len() > 16 {
        open.undo.pop_front();
    }
    token
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
        tutorial: open.tutorial,
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

struct PreparedSaveAs {
    store: MeasurementStore,
    old_project: Option<PathBuf>,
    old_irs: Option<PathBuf>,
}

impl PreparedSaveAs {
    fn finish(self) -> MeasurementStore {
        if let Some(path) = self.old_project {
            let _ = remove_path(&path);
        }
        if let Some(path) = self.old_irs {
            let _ = remove_path(&path);
        }
        self.store
    }
}

fn prepare_save_as(open: &OpenProject, destination: &Path) -> Result<PreparedSaveAs, ServiceError> {
    let source_irs = open.path.with_extension("irs");
    let destination_irs = destination.with_extension("irs");
    reject_nested_irs(&source_irs, &destination_irs)?;
    if destination.exists() && !destination.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::IsADirectory,
            "project destination is not a file",
        )
        .into());
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary_irs = unique_sibling(&destination_irs, "tmp")?;
    fs::create_dir(&temporary_irs)?;
    let temporary_project = unique_sibling(destination, "tmp")?;
    let result = (|| {
        if source_irs.is_dir() {
            copy_tree_contents(&source_irs, &temporary_irs)?;
        }
        let mut project = open.store.project().clone();
        rewrite_ir_directory(&mut project, &open.path, destination);
        let mut staged = MeasurementStore::new(&temporary_project, project.clone());
        staged.save()?;

        let old_project = swap_aside(destination, "old")?;
        let old_irs = match swap_aside(&destination_irs, "old") {
            Ok(path) => path,
            Err(error) => {
                restore_aside(destination, old_project.as_deref());
                return Err(error);
            }
        };
        if let Err(error) = fs::rename(&temporary_irs, &destination_irs) {
            restore_aside(&destination_irs, old_irs.as_deref());
            restore_aside(destination, old_project.as_deref());
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&temporary_project, destination) {
            let _ = fs::rename(&destination_irs, &temporary_irs);
            restore_aside(&destination_irs, old_irs.as_deref());
            restore_aside(destination, old_project.as_deref());
            return Err(error.into());
        }
        Ok(PreparedSaveAs {
            store: MeasurementStore::new(destination, project),
            old_project,
            old_irs,
        })
    })();
    if result.is_err() {
        let _ = remove_path(&temporary_project);
        let _ = remove_path(&temporary_irs);
    }
    result
}

fn reject_nested_irs(source: &Path, destination: &Path) -> Result<(), ServiceError> {
    if !source.is_dir() {
        return Ok(());
    }
    let source = source.canonicalize()?;
    // The destination folder may not exist yet (it is created after this
    // check): resolve its deepest existing ancestor and keep the rest as written.
    let mut existing = destination;
    let mut rest = Vec::new();
    let resolved =
        loop {
            match existing.canonicalize() {
                Ok(path) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    rest.push(existing.file_name().ok_or_else(|| {
                        ServiceError::Project("invalid measurement folder".into())
                    })?);
                    existing = existing
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(Path::new("."));
                }
                Err(error) => return Err(error.into()),
            }
        };
    let destination = rest
        .iter()
        .rev()
        .fold(resolved, |path, part| path.join(part));
    if destination == source || destination.starts_with(&source) {
        return Err(ServiceError::InvalidRequest(
            "cannot save a project inside its own measurement folder".into(),
        ));
    }
    Ok(())
}

fn unique_sibling(path: &Path, kind: &str) -> Result<PathBuf, std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    static TEMP: AtomicU64 = AtomicU64::new(0);
    for _ in 0..100 {
        let id = TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.{kind}-{}-{id}", std::process::id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a project staging path",
    ))
}

fn swap_aside(path: &Path, kind: &str) -> Result<Option<PathBuf>, ServiceError> {
    if !path.exists() {
        return Ok(None);
    }
    let aside = unique_sibling(path, kind)?;
    fs::rename(path, &aside)?;
    Ok(Some(aside))
}

fn restore_aside(path: &Path, aside: Option<&Path>) {
    let _ = remove_path(path);
    if let Some(aside) = aside {
        let _ = fs::rename(aside, path);
    }
}

fn remove_path(path: &Path) -> Result<(), std::io::Error> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn copy_tree_contents(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir(&target)?;
            copy_tree_contents(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn write_side_mdat(
    open: &mut OpenProject,
    uuid: &UuidSource,
    now_ms: i64,
) -> Result<(), ServiceError> {
    let mut records = open.store.records();
    records.sort_by_key(|record| record.pos);
    let project = open.store.project().clone();
    let plan_roles = plan_from_project(&project, std::iter::empty()).roles;
    let sources = load_project_sources(&project, &open.path);
    let mut inputs = Vec::new();
    for record in records {
        let mut roles = record.ir.keys().cloned().collect::<Vec<_>>();
        roles.sort_by_key(|role| {
            plan_roles
                .iter()
                .position(|planned| planned == role)
                .unwrap_or(plan_roles.len())
        });
        for role in roles {
            let (samples, sample_rate) = open.store.ir_of(&role, record.pos)?;
            let source_info = u32::try_from(record.pos)
                .ok()
                .and_then(|pos| sources.get(&pos))
                .and_then(|by_role| by_role.get(&role))
                .cloned();
            inputs.push(secs_mdat::MeasurementInput {
                title: format!("{role}_P{}", record.pos),
                notes: format!("SECS · 위치 {} · 원 측정", record.pos),
                sample_rate_hz: sample_rate,
                samples: samples.into_iter().map(f64::from).collect(),
                start_time_s: record.t0.get(&role).copied().unwrap_or(0.0),
                spl_offset_db: record.spl_offset.get(&role).copied(),
                source_info,
                computed: false,
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
    let bytes = secs_mdat::build(&inputs, secs_mdat::TEMPLATE, None, now_ms, &mut || {
        uuids.next().unwrap_or_default()
    })
    .map_err(|error| ServiceError::Project(error.to_string()))?;
    atomic_replace(&destination, &bytes)?;
    Ok(())
}

fn atomic_replace(destination: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    secs_types::fs::write_atomic_bytes(destination, bytes)
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
    use secs_mdat::{Boxed, SourceDate, SourceInfo};
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
        let created = session
            .create(source.clone(), "2.1", 1, false, false)
            .unwrap();
        assert_eq!(created.path, source.to_string_lossy());
        assert_eq!(created.sub_count, 1);

        let mut roles: Vec<_> = session
            .require()
            .unwrap()
            .store
            .project()
            .config
            .speakers
            .iter()
            .map(|speaker| speaker.role.clone())
            .collect();
        roles.push("Z".into());
        let open = session.require_mut().unwrap();
        open.store.start_position(1);
        for pos in [2, 1] {
            open.store.start_position(pos);
            for role in roles.iter().rev() {
                open.store
                    .record(
                        MeasurementResult {
                            role: role.clone(),
                            pos,
                            ir: vec![0.0, 1.0, 0.0],
                            sample_rate: 48_000,
                            t0_s: -(pos as f64) / 1000.0,
                            spl_db: None,
                            spl_offset_db: (role == "L").then_some(116.25),
                            snr_db: Some(30.0),
                            uuid: Some(format!("{role}-{pos}")),
                            source: Some("test".into()),
                            source_info: (pos == 1 && role == "L").then(|| SourceInfo {
                                strings: vec![("outputName".into(), "장치, out 1".into())],
                                boxed: vec![("usesEmbeddedCalData".into(), Boxed::Boolean(true))],
                                date: Some(SourceDate::Int(1_704_164_645_000)),
                                ..SourceInfo::default()
                            }),
                        },
                        Some(&roles),
                    )
                    .unwrap();
            }
        }
        open.dirty = true;
        let uuid = crate::Deps::default().uuid;
        let saved = session.save(Some(target.clone()), &uuid, 0).unwrap();
        assert_eq!(saved.path, target.to_string_lossy());
        assert!(!saved.dirty);
        assert!(target.exists());
        assert!(target.with_extension("irs").is_dir());
        assert!(target.with_extension("mdat").exists());
        assert!(target.with_extension("irs").join("L_P1.src.json").exists());
        assert!(!target.with_extension("irs").join("R_P1.src.json").exists());
        let side = secs_mdat::load_mdat_project(
            &fs::read(target.with_extension("mdat")).unwrap(),
            &secs_jos::Limits::default(),
        )
        .unwrap();
        let titles = side
            .measurements
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            [
                "L_P1", "R_P1", "SW1_P1", "Z_P1", "L_P2", "R_P2", "SW1_P2", "Z_P2"
            ]
        );
        assert_eq!(side.measurements[0].start_time_s, -0.001);
        assert_eq!(
            side.mdat.measurements[0].impulse_response.spl_offset_db,
            116.25
        );
        assert_eq!(
            side.measurements[0]
                .source_info
                .strings
                .iter()
                .find(|(key, _)| key == "outputName"),
            Some(&("outputName".into(), "장치, out 1".into()))
        );
        assert_eq!(
            side.measurements[1]
                .source_info
                .strings
                .iter()
                .find(|(key, _)| key == "outputName")
                .map(|(_, value)| value.as_str()),
            Some("")
        );

        let reopened = session.open(target.clone()).unwrap();
        assert_eq!(reopened.positions, vec![1, 2]);

        session.require_mut().unwrap().store.clear_position(1);
        session.require_mut().unwrap().store.clear_position(2);
        session.save(None, &uuid, 0).unwrap();
        assert!(!target.with_extension("mdat").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

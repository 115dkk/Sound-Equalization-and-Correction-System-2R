use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use secs_project::{
    DspProfile, MeasurementStore, Project, PyJson, RemovedPosition, measured_positions,
    plan_from_project,
};
use secs_types::ipc::{ErrorCode, IpcError, MigrationWarning, Notice, ProjectInfo};

use crate::jobs::JobContext;
use crate::results::CacheOwner;
use crate::{EventSink, Service, ServiceError, lock};

#[derive(Debug, Default)]
pub(crate) struct Session {
    open: Option<OpenProject>,
    last_export: Option<secs_types::local_api::LastExport>,
}

#[derive(Debug)]
pub(crate) struct OpenProject {
    pub path: PathBuf,
    store: MeasurementStore,
    dirty: bool,
    pub tutorial: bool,
    /// A tutorial's private folder, removed with the session (TD-90).
    pub tutorial_folder: Option<TutorialFolder>,
    pub results: crate::results::ResultBook,
    /// Read only through [`Session::ticket`] and compared only by
    /// [`commit`] (ADR 0009).
    identity: u64,
    undo: std::collections::VecDeque<UndoEntry>,
}

impl OpenProject {
    pub(crate) fn project(&self) -> &Project {
        self.store.project()
    }

    pub(crate) fn store(&self) -> &MeasurementStore {
        &self.store
    }

    pub(crate) fn edit<T>(&mut self, change: impl FnOnce(&mut Project) -> T) -> T {
        let value = change(self.store.project_mut());
        self.dirty = true;
        value
    }

    pub(crate) fn edit_if<T>(&mut self, change: impl FnOnce(&mut Project) -> (T, bool)) -> T {
        let (value, changed) = change(self.store.project_mut());
        if changed {
            self.dirty = true;
        }
        value
    }

    pub(crate) fn store_mut(&mut self) -> StoreEdit<'_> {
        StoreEdit {
            store: &mut self.store,
            dirty: &mut self.dirty,
            wrote: false,
        }
    }

    pub(crate) fn record_undo(&mut self, payload: UndoPayload) -> String {
        static TOKEN: AtomicU64 = AtomicU64::new(1);
        let token = format!(
            "{}-{}",
            self.identity,
            TOKEN.fetch_add(1, Ordering::Relaxed)
        );
        self.undo.push_back(UndoEntry {
            token: token.clone(),
            payload,
        });
        if self.undo.len() > 16 {
            self.undo.pop_front();
        }
        token
    }

    pub(crate) fn take_undo(&mut self, token: &str) -> Option<UndoPayload> {
        let index = self.undo.iter().position(|entry| entry.token == token)?;
        self.undo.remove(index).map(|entry| entry.payload)
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

pub(crate) struct StoreEdit<'a> {
    store: &'a mut MeasurementStore,
    dirty: &'a mut bool,
    wrote: bool,
}

impl StoreEdit<'_> {
    pub(crate) fn start_position(&mut self, pos: i64) -> secs_project::MeasurementRecord {
        self.wrote = true;
        self.store.start_position(pos)
    }

    pub(crate) fn clear_position(&mut self, pos: i64) -> bool {
        let changed = self.store.clear_position(pos);
        self.wrote |= changed;
        changed
    }

    pub(crate) fn record(
        &mut self,
        result: secs_project::MeasurementResult,
        plan_roles: Option<&[String]>,
    ) -> Result<secs_project::MeasurementRecord, secs_project::ProjectError> {
        let result = self.store.record(result, plan_roles);
        self.wrote |= result.is_ok();
        result
    }

    pub(crate) fn remove_position(&mut self, pos: i64) -> Option<RemovedPosition> {
        let removed = self.store.remove_position(pos);
        self.wrote |= removed.is_some();
        removed
    }

    pub(crate) fn restore_position(
        &mut self,
        removed: RemovedPosition,
    ) -> Result<(), secs_project::ProjectError> {
        let result = self.store.restore_position(removed);
        self.wrote |= result.is_ok();
        result
    }

    pub(crate) fn replace_source(
        &mut self,
        role: &str,
        pos: i64,
        info: Option<&secs_mdat::SourceInfo>,
    ) -> Result<(), secs_project::ProjectError> {
        let result = self.store.replace_source(role, pos, info);
        self.wrote |= result.is_ok();
        result
    }

    pub(crate) fn write_result<T, E>(
        &mut self,
        change: impl FnOnce(&mut MeasurementStore) -> Result<T, E>,
    ) -> Result<T, E> {
        let result = change(self.store);
        self.wrote |= result.is_ok();
        result
    }

    pub(crate) fn source_of(&self, role: &str, pos: i64) -> Option<Arc<secs_mdat::SourceInfo>> {
        self.store.source_of(role, pos)
    }

    pub(crate) fn measured_ir(
        &mut self,
        role: &str,
        pos: i64,
    ) -> Result<(Vec<f32>, u32), crate::ServiceError> {
        crate::calibration::measured_ir(self.store, role, pos)
    }

    pub(crate) fn save(&mut self) -> Result<(), secs_project::ProjectError> {
        self.store.save()
    }
}

impl std::ops::Deref for StoreEdit<'_> {
    type Target = MeasurementStore;

    fn deref(&self) -> &Self::Target {
        self.store
    }
}

impl Drop for StoreEdit<'_> {
    fn drop(&mut self) {
        if self.wrote {
            *self.dirty = true;
        }
    }
}

/// The private folder a tutorial session writes its measurements and
/// results into instead of the given path (under the user's app-data folder).
/// Dropping it removes the folder, best effort, so closing, replacing or
/// opening another project cleans up (TD-90).
#[derive(Debug)]
pub(crate) struct TutorialFolder {
    folder: PathBuf,
    /// The path the screen gave, reported back unchanged.
    requested: PathBuf,
}

impl TutorialFolder {
    fn create(requested: PathBuf, app_data: &Path) -> Result<Self, ServiceError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = app_data.join("tutorial");
        fs::create_dir_all(&base)?;
        loop {
            let folder = base.join(format!(
                "secs2r-tutorial-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&folder) {
                Ok(()) => return Ok(Self { folder, requested }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn project(&self) -> PathBuf {
        self.folder.join("tutorial.secs")
    }
}

impl Drop for TutorialFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.folder);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UndoEntry {
    pub token: String,
    pub payload: UndoPayload,
}

#[derive(Debug, Clone)]
pub(crate) enum UndoPayload {
    Position(RemovedPosition),
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
        app_data: &Path,
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
        let tutorial_folder = if tutorial {
            Some(TutorialFolder::create(path.clone(), app_data)?)
        } else {
            None
        };
        let path = tutorial_folder
            .as_ref()
            .map_or(path, TutorialFolder::project);
        let mut store = MeasurementStore::new(&path, project);
        if !tutorial {
            store.save()?;
        }
        self.open = Some(OpenProject {
            path,
            store,
            dirty: false,
            tutorial,
            tutorial_folder,
            results: crate::results::ResultBook::from_cache(None),
            identity: next_identity(),
            undo: Default::default(),
        });
        self.info()
    }

    pub fn open(&mut self, path: PathBuf) -> Result<ProjectInfo, ServiceError> {
        let store = MeasurementStore::open(&path).map_err(|source| ServiceError::ProjectOpen {
            path: path.clone(),
            source,
        })?;
        let cached = crate::cache::load(&path);
        self.open = Some(OpenProject {
            path,
            store,
            dirty: false,
            tutorial: false,
            tutorial_folder: None,
            results: crate::results::ResultBook::from_cache(cached),
            identity: next_identity(),
            undo: Default::default(),
        });
        self.info()
    }

    pub fn save(&mut self, path: Option<PathBuf>) -> Result<ProjectInfo, ServiceError> {
        let open = self.require_mut()?;
        if open.tutorial {
            return Err(ServiceError::TutorialBlocked);
        }
        if let Some(path) = path
            && path != open.path
        {
            let next = prepare_save_as(open, &path)?;
            open.path = path;
            open.store = next.finish();
        } else {
            open.store.save()?;
        }
        open.mark_saved();
        self.info()
    }

    pub fn info(&self) -> Result<ProjectInfo, ServiceError> {
        let open = self.require()?;
        Ok(info(open))
    }

    pub(crate) fn last_export(&self) -> Option<secs_types::local_api::LastExport> {
        self.last_export.clone()
    }

    pub(crate) fn record_export(&mut self, export: secs_types::local_api::LastExport) {
        self.last_export = Some(export);
    }

    pub fn require(&self) -> Result<&OpenProject, ServiceError> {
        self.open.as_ref().ok_or(ServiceError::ProjectNotOpen)
    }
    pub fn require_mut(&mut self) -> Result<&mut OpenProject, ServiceError> {
        self.open.as_mut().ok_or(ServiceError::ProjectNotOpen)
    }

    pub(crate) fn snapshot(&self) -> Result<ProjectSnapshot, ServiceError> {
        let open = self.require()?;
        Ok(ProjectSnapshot::of(ProjectTicket(open.identity), open))
    }

    /// The open project's ticket; `ProjectNotOpen` when none is open.
    pub(crate) fn ticket(&self) -> Result<ProjectTicket, IpcError> {
        Ok(ProjectTicket(self.require()?.identity))
    }

    /// The open project if it is still the one `ticket` was taken from.
    fn holding(&mut self, ticket: ProjectTicket) -> Result<&mut OpenProject, IpcError> {
        let open = self.require_mut()?;
        if open.identity != ticket.0 {
            return Err(IpcError::new(
                ErrorCode::StaleInputs,
                "project changed while computing",
            ));
        }
        Ok(open)
    }
}

impl Service {
    pub(crate) fn with_open<T>(
        &self,
        change: impl FnOnce(&mut OpenProject, &crate::compute::ComputeIdentity) -> Result<T, IpcError>,
    ) -> Result<T, IpcError> {
        let compute = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let mut session = lock(&self.session);
        let open = session.require_mut().map_err(IpcError::from)?;
        change(open, &compute)
    }

    pub(crate) fn with_open_ticket<T>(
        &self,
        ticket: ProjectTicket,
        change: impl FnOnce(&mut OpenProject, &crate::compute::ComputeIdentity) -> Result<T, IpcError>,
    ) -> Result<T, IpcError> {
        let compute = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let mut session = lock(&self.session);
        let open = session.holding(ticket)?;
        change(open, &compute)
    }

    pub(crate) fn read_open<T>(
        &self,
        read: impl FnOnce(&OpenProject, &crate::compute::ComputeIdentity) -> T,
    ) -> Result<T, IpcError> {
        let compute = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        Ok(read(open, &compute))
    }

    pub(crate) fn project_save(
        &self,
        params: secs_types::ipc::ProjectSaveParams,
        events: Arc<dyn EventSink>,
    ) -> Result<ProjectInfo, IpcError> {
        let mut session = lock(&self.session);
        let info = session
            .save(params.path.map(Into::into))
            .map_err(IpcError::from)?;
        let originals = session.snapshot().map_err(IpcError::from)?;
        let destination = originals.path.with_extension("mdat");
        let work = SideMdatWork {
            originals,
            destination: destination.clone(),
            uuid: self.deps.uuid.clone(),
            now_ms: (self.deps.unix_ms)(),
        };
        drop(session);

        let queue = self.side_mdat_queue.clone();
        let start = {
            let mut queue = lock(&queue);
            queue.next_generation = queue.next_generation.saturating_add(1);
            let generation = queue.next_generation;
            queue.pending.insert(destination, (generation, work));
            if queue.running {
                false
            } else {
                queue.running = true;
                true
            }
        };
        if start {
            let worker_queue = queue.clone();
            if let Err(error) = self
                .start_side_mdat_job(events, move |job| write_side_mdat_queue(worker_queue, job))
            {
                lock(&queue).running = false;
                return Err(error);
            }
        }
        Ok(info)
    }
}

/// The open project a request started from (ADR 0009). A job takes one when
/// its handler starts and changes the project only through [`commit`], so a
/// late result of an earlier project never lands in a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectTicket(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Finality {
    Final,
    Phase,
    Streaming,
}

/// Commits a job change after checking the project ticket and the selected
/// cancellation policy. Only a final commit makes the job non-cancellable.
pub(crate) fn commit<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    job: &JobContext,
    finality: Finality,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let mut session = lock(session);
    let open = session.holding(ticket)?;
    match finality {
        Finality::Final => job.begin_commit()?,
        Finality::Phase => job
            .cancel()
            .check()
            .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?,
        Finality::Streaming => {}
    }
    run_change(open, change)
}

const CACHE_SAVE_FAILED: &str = "cache_save_failed";

pub(crate) fn commit_results<T>(
    session: &Arc<Mutex<Session>>,
    ticket: ProjectTicket,
    job: &JobContext,
    finality: Finality,
    owner: CacheOwner,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let (value, path, cached) = commit(session, ticket, job, finality, |open| {
        let (value, changed) = change(open)?;
        Ok((
            (value, open.path.clone(), open.results.cache_snapshot()),
            changed,
        ))
    })?;
    let Some(cached) = cached else {
        return Ok(value);
    };

    let saved = crate::cache::save(&path, &cached);
    let _ = commit(session, ticket, job, Finality::Streaming, |open| {
        if open.path != path {
            return Ok(((), false));
        }
        match saved {
            Ok(()) => {
                open.results.replace_cache(cached);
                for notes in open.results.all_notes_mut() {
                    notes.retain(|notice| notice.code != CACHE_SAVE_FAILED);
                }
            }
            Err(error) => {
                if let Some(notes) = open.results.owner_notes_mut(owner) {
                    notes.retain(|notice| notice.code != CACHE_SAVE_FAILED);
                    notes.push(Notice {
                        code: CACHE_SAVE_FAILED.into(),
                        params: [("error".into(), error.to_string())].into(),
                    });
                }
            }
        }
        Ok(((), false))
    });
    Ok(value)
}

fn run_change<T>(
    open: &mut OpenProject,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let (value, changed) = change(open)?;
    Ok(open.edit_if(|_| (value, changed)))
}

fn next_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn push_position_undo(open: &mut OpenProject, removed: RemovedPosition) -> String {
    open.record_undo(UndoPayload::Position(removed))
}

pub(crate) fn push_measurements_undo(open: &mut OpenProject, measurements: PyJson) -> String {
    open.record_undo(UndoPayload::Measurements(measurements))
}

pub(crate) fn info(open: &OpenProject) -> ProjectInfo {
    let project = open.project();
    let path = open
        .tutorial_folder
        .as_ref()
        .map_or(&open.path, |folder| &folder.requested);
    ProjectInfo {
        path: path.to_string_lossy().into_owned(),
        name: project.name.clone(),
        preset: project.config.preset.clone(),
        sub_count: project.config.sub_count(),
        positions: measured_positions(project),
        dirty: open.is_dirty(),
        tutorial: open.tutorial,
        migration_warnings: open
            .store()
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
    reject_nested_irs(&open.path, &source_irs, &destination_irs)?;
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
        let mut project = open.project().clone();
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

fn reject_nested_irs(
    project: &Path,
    source: &Path,
    destination: &Path,
) -> Result<(), ServiceError> {
    let source_exists = source.is_dir();
    let project = normalised_path(project)?;
    let source = normalised_path(source)?;
    let destination = normalised_path(destination)?;
    if (destination == source && source_exists)
        || destination.starts_with(&source) && destination != source
        || source.starts_with(&destination) && source != destination
        || project.starts_with(&destination)
    {
        return Err(ServiceError::InvalidRequest(
            "cannot save a project inside its own measurement folder".into(),
        ));
    }
    Ok(())
}

fn normalised_path(path: &Path) -> Result<PathBuf, ServiceError> {
    // A save destination may not exist yet. Resolve its deepest existing
    // ancestor, then append the missing components without following links.
    let mut existing = path;
    let mut rest = Vec::new();
    let resolved = loop {
        match existing.canonicalize() {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                rest.push(existing.file_name().ok_or_else(|| {
                    ServiceError::ProjectMessage("invalid measurement folder".into())
                })?);
                existing = existing
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
            }
            Err(error) => return Err(error.into()),
        }
    };
    Ok(rest
        .iter()
        .rev()
        .fold(resolved, |resolved, part| resolved.join(part)))
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

#[derive(Clone, Debug)]
pub(crate) struct ProjectSnapshot {
    pub ticket: ProjectTicket,
    pub path: PathBuf,
    pub project: Project,
    pub sources: BTreeMap<u32, BTreeMap<String, Arc<secs_mdat::SourceInfo>>>,
    source_warnings: Vec<Notice>,
}

impl ProjectSnapshot {
    pub(crate) fn of(ticket: ProjectTicket, open: &OpenProject) -> Self {
        Self {
            ticket,
            path: open.path.clone(),
            project: open.project().clone(),
            sources: open.store().sources_by_position(),
            source_warnings: open
                .store()
                .warnings()
                .iter()
                .filter(|warning| warning.code.starts_with("measurement.source."))
                .map(|warning| Notice {
                    code: warning.code.into(),
                    params: [("entry".into(), warning.entry.clone())].into(),
                })
                .collect(),
        }
    }
}

pub(crate) struct LoadedOriginals<'a> {
    pub values: Vec<secs_export::Original>,
    _permit: secs_compute::MemoryPermit<'a>,
}

pub(crate) struct LoadedMeasurements {
    pub project: Project,
    pub values: secs_engine::MeasurementSet,
    pub notices: Vec<Notice>,
}

fn measurement_bytes(path: &Path, project: &Project) -> Result<u64, IpcError> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut bytes = 0_u64;
    let store = MeasurementStore::new(path, project.clone());
    for record in store.records() {
        for raw in record.ir.values().filter(|raw| !raw.is_empty()) {
            let wav = PathBuf::from(raw);
            let wav = if wav.is_absolute() {
                wav
            } else {
                base.join(wav)
            };
            let file_bytes = fs::metadata(&wav)
                .map_err(ServiceError::from)
                .map_err(IpcError::from)?
                .len()
                .saturating_mul(12);
            bytes = bytes.saturating_add(file_bytes);
        }
    }
    Ok(bytes)
}

fn reserve_measurements<'a>(
    path: &Path,
    project: &Project,
    job: &JobContext,
    budget: &'a secs_compute::MemoryBudget,
) -> Result<secs_compute::MemoryPermit<'a>, IpcError> {
    budget
        .acquire(measurement_bytes(path, project)?, job.cancel())
        .map_err(|error| match error {
            secs_compute::MemoryError::Cancelled => {
                IpcError::new(ErrorCode::Cancelled, "cancelled")
            }
            secs_compute::MemoryError::TooLarge { .. } => {
                IpcError::new(ErrorCode::EngineInput, error.to_string())
                    .with_param("reason", "memory_budget")
            }
        })
}

impl ProjectSnapshot {
    pub(crate) fn log_source_warnings(&self, job: &JobContext) {
        for warning in &self.source_warnings {
            job.log(warning.clone());
        }
    }

    pub(crate) fn load_measurements(
        &self,
        job: &JobContext,
        budget: &secs_compute::MemoryBudget,
    ) -> Result<LoadedMeasurements, IpcError> {
        let permit = reserve_measurements(&self.path, &self.project, job, budget)?;
        let mut store = MeasurementStore::new(&self.path, self.project.clone());
        let loaded = super::calibration::load_measurements_with_notices(
            &mut store,
            &self.sources,
            job.cancel(),
        )?;
        drop(permit);
        Ok(LoadedMeasurements {
            project: self.project.clone(),
            values: loaded.measurements,
            notices: loaded.notices,
        })
    }

    pub(crate) fn load_originals<'a>(
        &self,
        job: &JobContext,
        budget: &'a secs_compute::MemoryBudget,
    ) -> Result<LoadedOriginals<'a>, IpcError> {
        self.log_source_warnings(job);
        let mut store = MeasurementStore::new(&self.path, self.project.clone());
        let mut records = store.records();
        records.sort_by_key(|record| record.pos);
        let project = store.project().clone();
        let plan_roles = plan_from_project(&project, std::iter::empty()).roles;
        let total = records
            .iter()
            .map(|record| record.ir.values().filter(|path| !path.is_empty()).count())
            .sum::<usize>();
        let permit = reserve_measurements(&self.path, &project, job, budget)?;
        let mut originals = Vec::with_capacity(total);
        for record in records {
            let position = u32::try_from(record.pos).map_err(|_| {
                IpcError::new(
                    ErrorCode::ProjectIo,
                    format!("invalid measurement position {}", record.pos),
                )
            })?;
            let mut roles = record.ir.keys().cloned().collect::<Vec<_>>();
            roles.sort_by_key(|role| {
                plan_roles
                    .iter()
                    .position(|planned| planned == role)
                    .unwrap_or(plan_roles.len())
            });
            for role in roles {
                job.cancel()
                    .check()
                    .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
                let raw = record.ir.get(&role).cloned().unwrap_or_default();
                if raw.is_empty() {
                    continue;
                }
                let (samples, sample_rate) = store
                    .ir_of(&role, record.pos)
                    .map_err(ServiceError::from)
                    .map_err(IpcError::from)?;
                // The bounded WAV reader consumes one whole file. Check on both
                // sides of every file, so cancellation is delayed by at most one
                // MAX_IR_BYTES read rather than by the whole measurement set.
                job.report_progress(
                    "read_measurements",
                    originals.len() as u64 + 1,
                    total as u64,
                );
                job.cancel()
                    .check()
                    .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
                let source_info = self
                    .sources
                    .get(&position)
                    .and_then(|by_role| by_role.get(&role))
                    .map(|source| source.as_ref().clone());
                originals.push(secs_export::Original {
                    position,
                    role: role.clone(),
                    item: secs_mdat::MeasurementInput {
                        title: format!("{role}_P{}", record.pos),
                        notes: format!("SECS · 위치 {} · 원 측정", record.pos),
                        sample_rate_hz: sample_rate,
                        samples: samples.into_iter().map(f64::from).collect(),
                        start_time_s: record.t0.get(&role).copied().unwrap_or(0.0),
                        spl_offset_db: record.spl_offset.get(&role).copied(),
                        source_info,
                        computed: false,
                    },
                });
            }
        }
        Ok(LoadedOriginals {
            values: originals,
            _permit: permit,
        })
    }
}

#[derive(Clone)]
struct SideMdatWork {
    originals: ProjectSnapshot,
    destination: PathBuf,
    uuid: crate::UuidSource,
    now_ms: i64,
}

#[derive(Default)]
pub(crate) struct SideMdatQueue {
    pending: BTreeMap<PathBuf, (u64, SideMdatWork)>,
    next_generation: u64,
    running: bool,
}

impl std::fmt::Debug for SideMdatQueue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SideMdatQueue")
            .field("pending", &self.pending.keys().collect::<Vec<_>>())
            .field("next_generation", &self.next_generation)
            .field("running", &self.running)
            .finish()
    }
}

struct SideMdatRunningGuard {
    queue: Arc<Mutex<SideMdatQueue>>,
    armed: bool,
}

impl SideMdatRunningGuard {
    fn new(queue: Arc<Mutex<SideMdatQueue>>) -> Self {
        Self { queue, armed: true }
    }

    fn stop(&mut self) {
        if self.armed {
            lock(&self.queue).running = false;
            self.armed = false;
        }
    }

    fn requeue_and_stop(&mut self, destination: PathBuf, generation: u64, work: SideMdatWork) {
        let mut queue = lock(&self.queue);
        let replace = queue
            .pending
            .get(&destination)
            .is_none_or(|(pending_generation, _)| *pending_generation < generation);
        if replace {
            queue.pending.insert(destination, (generation, work));
        }
        queue.running = false;
        self.armed = false;
    }
}

impl Drop for SideMdatRunningGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

fn write_side_mdat_queue(
    queue: Arc<Mutex<SideMdatQueue>>,
    job: JobContext,
) -> Result<secs_types::ipc::JobResult, IpcError> {
    let mut running = SideMdatRunningGuard::new(queue.clone());
    let mut completed = None;
    loop {
        let next = {
            let mut pending = lock(&queue);
            if job.cancel().is_cancelled() {
                pending.running = false;
                running.armed = false;
                return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
            }
            match pending.pending.pop_first() {
                Some((destination, (generation, work))) => Some((destination, generation, work)),
                None => {
                    pending.running = false;
                    running.armed = false;
                    None
                }
            }
        };
        let Some((destination, generation, work)) = next else {
            return completed.ok_or_else(|| {
                IpcError::new(ErrorCode::Internal, "side mdat queue finished without work")
            });
        };
        match write_side_mdat_entry(&queue, generation, &work, &job) {
            Ok(Some(result)) => completed = Some(result),
            Ok(None) => {}
            Err(error) => {
                running.requeue_and_stop(destination, generation, work);
                return Err(error);
            }
        }
    }
}

fn write_side_mdat_entry(
    queue: &Mutex<SideMdatQueue>,
    generation: u64,
    work: &SideMdatWork,
    job: &JobContext,
) -> Result<Option<secs_types::ipc::JobResult>, IpcError> {
    let context = super::calibration::compute_context(job, false);
    let originals = work.originals.load_originals(job, context.budget())?;
    let LoadedOriginals { values, _permit } = originals;
    let inputs = values
        .into_iter()
        .map(|original| original.item)
        .collect::<Vec<_>>();
    let bytes = if inputs.is_empty() {
        None
    } else {
        let uuids = {
            let mut uuid = lock(&work.uuid);
            (0..inputs.len())
                .map(|_| uuid.as_mut()())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| side_mdat_error(&work.destination, error))?
        };
        let mut uuids = uuids.into_iter();
        // `secs_mdat::build` consumes exactly one UUID for each validated input.
        Some(
            secs_mdat::build(&inputs, secs_mdat::TEMPLATE, None, work.now_ms, &mut || {
                uuids.next().unwrap_or_default()
            })
            .map_err(|error| side_mdat_error(&work.destination, error))?,
        )
    };
    let queue = lock(queue);
    if queue
        .pending
        .get(&work.destination)
        .is_some_and(|(pending_generation, _)| *pending_generation > generation)
    {
        return Ok(None);
    }
    job.begin_commit()?;
    let files = match bytes {
        Some(bytes) => {
            let parent = work.destination.parent().unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent)
                .map_err(|error| side_mdat_error(&work.destination, error))?;
            secs_types::fs::write_atomic_bytes(&work.destination, &bytes)
                .map_err(|error| side_mdat_error(&work.destination, error))?;
            vec![work.destination.to_string_lossy().into_owned()]
        }
        None => {
            match fs::remove_file(&work.destination) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(side_mdat_error(&work.destination, error)),
            }
            Vec::new()
        }
    };
    drop(queue);
    Ok(Some(side_mdat_result(&work.destination, files)))
}

fn side_mdat_result(destination: &Path, files: Vec<String>) -> secs_types::ipc::JobResult {
    secs_types::ipc::JobResult::Export(secs_types::ipc::ExportResult {
        files,
        folder: destination
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .into_owned(),
        stale: false,
        output_rate: 48_000,
        notes: Vec::new(),
    })
}

fn side_mdat_error(destination: &Path, error: impl std::fmt::Display) -> IpcError {
    IpcError::new(
        ErrorCode::ProjectIo,
        format!(
            "could not save side mdat {}: {error}",
            destination.display()
        ),
    )
    .with_param("path", destination.to_string_lossy())
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
            .create(source.clone(), "2.1", 1, false, false, &root)
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
        let saved = session.save(Some(target.clone())).unwrap();
        assert_eq!(saved.path, target.to_string_lossy());
        assert!(!saved.dirty);
        assert!(target.exists());
        assert!(target.with_extension("irs").is_dir());
        assert!(target.with_extension("irs").join("L_P1.src.json").exists());
        assert!(!target.with_extension("irs").join("R_P1.src.json").exists());

        let reopened = session.open(target.clone()).unwrap();
        assert_eq!(reopened.positions, vec![1, 2]);

        session.require_mut().unwrap().store.clear_position(1);
        session.require_mut().unwrap().store.clear_position(2);
        session.save(None).unwrap();
        assert!(!target.with_extension("mdat").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod commit_tests {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::mpsc;
    use std::sync::{Arc, Condvar};
    use std::time::{Duration, Instant};

    use secs_project::MeasurementResult;
    use secs_types::ipc::{JobEvent, JobId, JobKind, JobResult, JobStatus};

    use super::*;
    use crate::EventSink;
    use crate::jobs::JobRegistry;

    #[derive(Debug, Default)]
    struct Finished {
        values: Mutex<Vec<(JobId, JobStatus, Option<IpcError>)>>,
        changed: Condvar,
    }
    impl EventSink for Finished {
        fn send(&self, event: JobEvent) {
            if let JobEvent::Finished {
                job, status, error, ..
            } = event
            {
                lock(&self.values).push((job, status, error));
                self.changed.notify_all();
            }
        }
    }
    #[derive(Debug)]
    struct CancelOnFirstRead {
        registry: JobRegistry,
        finished: Finished,
        reads: AtomicU64,
    }

    impl EventSink for CancelOnFirstRead {
        fn send(&self, event: JobEvent) {
            if let JobEvent::Progress {
                job, stage, done, ..
            } = &event
                && stage == "read_measurements"
            {
                self.reads.store(*done, Ordering::SeqCst);
                if *done == 1 {
                    self.registry.cancel(*job).unwrap();
                }
            }
            self.finished.send(event);
        }
    }

    impl Finished {
        fn wait(&self, job: JobId) -> (JobStatus, Option<IpcError>) {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut values = lock(&self.values);
            loop {
                if let Some((_, status, error)) = values.iter().find(|(id, ..)| *id == job) {
                    return (*status, error.clone());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "job {job:?} did not finish");
                values = self.changed.wait_timeout(values, remaining).unwrap().0;
            }
        }
    }

    /// An open, unsaved project with its own identity; nothing touches the disk.
    fn project() -> OpenProject {
        let path = PathBuf::from("commit-test.secs");
        OpenProject {
            store: MeasurementStore::new(&path, Project::default()),
            path,
            dirty: false,
            tutorial: false,
            tutorial_folder: None,
            results: crate::results::ResultBook::default(),
            identity: next_identity(),
            undo: Default::default(),
        }
    }

    fn opened() -> Arc<Mutex<Session>> {
        Arc::new(Mutex::new(Session {
            open: Some(project()),
            last_export: None,
        }))
    }

    #[test]
    fn with_open_may_call_acceleration_again_without_deadlock() {
        let service = Service::new(crate::Deps::default());
        lock(&service.session).open = Some(project());
        service
            .with_open(|_, identity| {
                let again = crate::compute::ComputeIdentity::from(&service.acceleration().status);
                assert_eq!(*identity, again);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn edit_marks_dirty_and_edit_if_marks_only_when_told() {
        let mut open = project();
        open.edit(|project| project.name = "edited".into());
        assert!(open.is_dirty());
        assert_eq!(open.project().name, "edited");

        open.mark_saved();
        assert!(!open.is_dirty());
        open.edit_if(|_| ((), false));
        assert!(!open.is_dirty());
    }

    #[test]
    fn store_mut_marks_dirty_only_when_a_write_method_ran() {
        let root =
            std::env::temp_dir().join(format!("secs-service-store-edit-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("room.secs");
        let mut open = OpenProject {
            store: MeasurementStore::new(&path, Project::default()),
            path,
            dirty: false,
            tutorial: false,
            tutorial_folder: None,
            results: crate::results::ResultBook::default(),
            identity: next_identity(),
            undo: Default::default(),
        };

        drop(open.store_mut());
        assert!(!open.is_dirty());
        open.store_mut()
            .record(
                MeasurementResult {
                    role: "L".into(),
                    pos: 1,
                    ir: vec![0.0, 1.0, 0.0],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: None,
                    source: None,
                    source_info: None,
                },
                None,
            )
            .unwrap();
        assert!(open.is_dirty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_snapshot_load_originals_checks_cancel_between_files_and_keeps_plan_order() {
        let root = std::env::temp_dir().join(format!(
            "secs-service-project-snapshot-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("room.secs");
        let mut store = MeasurementStore::new(&path, Project::from_preset("2.0").unwrap());
        for role in ["A", "R", "L"] {
            store
                .record(
                    MeasurementResult {
                        role: role.into(),
                        pos: 1,
                        ir: vec![0.0, 1.0, 0.0],
                        sample_rate: 48_000,
                        t0_s: 0.0,
                        spl_db: None,
                        spl_offset_db: None,
                        snr_db: None,
                        uuid: None,
                        source: None,
                        source_info: None,
                    },
                    None,
                )
                .unwrap();
        }
        let session = Arc::new(Mutex::new(Session {
            open: Some(OpenProject {
                path,
                store,
                dirty: false,
                tutorial: false,
                tutorial_folder: None,
                results: crate::results::ResultBook::default(),
                identity: next_identity(),
                undo: Default::default(),
            }),
            last_export: None,
        }));
        let snapshot = lock(&session).snapshot().unwrap();
        let roles = Arc::new(Mutex::new(Vec::new()));
        let captured = roles.clone();
        let registry = JobRegistry::default();
        let finished = Arc::new(Finished::default());
        let started = registry
            .start(
                JobKind::Export,
                finished.clone(),
                Arc::new(secs_compute::FixedMemory(u64::MAX)),
                move |job| {
                    let budget = secs_compute::MemoryBudget::new(u64::MAX);
                    let loaded = snapshot.load_originals(&job, &budget)?;
                    *lock(&captured) = loaded.values.into_iter().map(|value| value.role).collect();
                    Ok(JobResult::Stopped)
                },
            )
            .unwrap();
        assert_eq!(finished.wait(started.job), (JobStatus::Succeeded, None));
        assert_eq!(&*lock(&roles), &["L", "R", "A"]);

        let snapshot = lock(&session).snapshot().unwrap();
        let registry = JobRegistry::default();
        let events = Arc::new(CancelOnFirstRead {
            registry: registry.clone(),
            finished: Finished::default(),
            reads: AtomicU64::new(0),
        });
        let started = registry
            .start(
                JobKind::Export,
                events.clone(),
                Arc::new(secs_compute::FixedMemory(u64::MAX)),
                move |job| {
                    let budget = secs_compute::MemoryBudget::new(u64::MAX);
                    snapshot.load_originals(&job, &budget)?;
                    Ok(JobResult::Stopped)
                },
            )
            .unwrap();
        let (status, error) = events.finished.wait(started.job);
        assert_eq!(status, JobStatus::Cancelled);
        assert_eq!(error.unwrap().code, ErrorCode::Cancelled);
        assert_eq!(events.reads.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn record_undo_hands_the_payload_back_once_and_caps_at_sixteen() {
        let mut open = project();
        let first = open.record_undo(UndoPayload::Measurements(PyJson::Int("0".into())));
        assert!(first.starts_with(&format!("{}-", open.identity)));
        for value in 1..=16 {
            open.record_undo(UndoPayload::Measurements(PyJson::Int(value.to_string())));
        }
        assert!(open.take_undo(&first).is_none());

        let token = open.record_undo(UndoPayload::Measurements(PyJson::Int("once".into())));
        assert!(matches!(
            open.take_undo(&token),
            Some(UndoPayload::Measurements(PyJson::Int(value))) if value == "once"
        ));
        assert!(open.take_undo(&token).is_none());
    }

    /// Opens another project, as `project_new` or `project_open` would.
    fn replace(session: &Mutex<Session>) {
        lock(session).open = Some(project());
    }

    fn dirty(session: &Mutex<Session>) -> bool {
        lock(session).require().unwrap().dirty
    }

    fn cancellable(registry: &JobRegistry, job: JobId) -> bool {
        registry
            .snapshots()
            .into_iter()
            .find(|snapshot| snapshot.job == job)
            .expect("the job is listed")
            .cancellable
    }

    /// Starts `body` as a real job, lets the test act before it commits,
    /// and waits for its finish.
    fn run(
        registry: &JobRegistry,
        before_commit: impl FnOnce(JobId),
        body: impl FnOnce(JobContext) -> Result<JobResult, IpcError> + Send + 'static,
    ) -> (JobStatus, Option<IpcError>) {
        let events = Arc::new(Finished::default());
        let (release, gate) = mpsc::channel::<()>();
        let started = registry
            .start(
                JobKind::Calibrate,
                events.clone(),
                Arc::new(secs_compute::FixedMemory(u64::MAX)),
                move |job| {
                    gate.recv().expect("the test releases the job");
                    body(job)
                },
            )
            .unwrap();
        before_commit(started.job);
        release.send(()).unwrap();
        events.wait(started.job)
    }

    #[test]
    fn ticket_needs_an_open_project() {
        let error = Session::default().ticket().unwrap_err();
        assert_eq!(error.code, ErrorCode::ProjectNotOpen);
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        assert_eq!(lock(&session).ticket().unwrap(), ticket);
        replace(&session);
        assert_ne!(lock(&session).ticket().unwrap(), ticket);
    }

    #[test]
    fn final_commit_refuses_a_stale_ticket_before_begin_commit() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let still_cancellable = Arc::new(AtomicBool::new(false));
        let (job_session, job_ran, job_cancellable, job_registry) = (
            session.clone(),
            ran.clone(),
            still_cancellable.clone(),
            registry.clone(),
        );
        let (status, error) = run(
            &registry,
            |_| replace(&session),
            move |job| {
                let outcome = commit(&job_session, ticket, &job, Finality::Final, |_| {
                    job_ran.store(true, Ordering::SeqCst);
                    Ok(((), true))
                });
                job_cancellable.store(cancellable(&job_registry, job.id()), Ordering::SeqCst);
                outcome.map(|()| JobResult::Stopped)
            },
        );
        let error = error.expect("a stale commit fails");
        assert_eq!(status, JobStatus::Failed);
        assert_eq!(error.code, ErrorCode::StaleInputs);
        assert_eq!(error.detail, "project changed while computing");
        assert!(!ran.load(Ordering::SeqCst));
        assert!(still_cancellable.load(Ordering::SeqCst));
        assert!(!dirty(&session));
    }

    #[test]
    fn final_commit_reports_cancelled_before_running_the_change() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let (job_session, job_ran) = (session.clone(), ran.clone());
        let (status, error) = run(
            &registry,
            |job| {
                registry.cancel(job).unwrap();
            },
            move |job| {
                commit(&job_session, ticket, &job, Finality::Final, |_| {
                    job_ran.store(true, Ordering::SeqCst);
                    Ok(((), true))
                })
                .map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(status, JobStatus::Cancelled);
        assert_eq!(
            error.expect("a cancelled commit fails").code,
            ErrorCode::Cancelled
        );
        assert!(!ran.load(Ordering::SeqCst));
        assert!(!dirty(&session));
    }

    #[test]
    fn final_commit_runs_the_change_after_begin_commit_and_dirty_follows_the_return_value() {
        for changed in [false, true] {
            let registry = JobRegistry::default();
            let session = opened();
            let ticket = lock(&session).ticket().unwrap();
            let seen = Arc::new(Mutex::new(None));
            let (job_session, job_seen, job_registry) =
                (session.clone(), seen.clone(), registry.clone());
            let (status, error) = run(
                &registry,
                |_| {},
                move |job| {
                    let id = job.id();
                    let value = commit(&job_session, ticket, &job, Finality::Final, |open| {
                        // `begin_commit` already made the job non-cancellable.
                        let cancellable = cancellable(&job_registry, id);
                        Ok(((open.path.clone(), cancellable), changed))
                    })?;
                    *lock(&job_seen) = Some(value);
                    Ok(JobResult::Stopped)
                },
            );
            assert_eq!((status, error), (JobStatus::Succeeded, None));
            assert_eq!(
                lock(&seen).take(),
                Some((PathBuf::from("commit-test.secs"), false))
            );
            assert_eq!(dirty(&session), changed);
        }
    }

    #[test]
    fn phase_commit_after_cancel_is_cancelled_and_leaves_the_job_cancellable() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let still_cancellable = Arc::new(AtomicBool::new(false));
        let (job_session, job_cancellable, job_registry) =
            (session.clone(), still_cancellable.clone(), registry.clone());
        let (status, error) = run(
            &registry,
            |job| {
                registry.cancel(job).unwrap();
            },
            move |job| {
                let outcome = commit(&job_session, ticket, &job, Finality::Phase, |_| {
                    Ok(((), true))
                });
                job_cancellable.store(cancellable(&job_registry, job.id()), Ordering::SeqCst);
                outcome.map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(status, JobStatus::Cancelled);
        assert_eq!(error.unwrap().code, ErrorCode::Cancelled);
        assert!(still_cancellable.load(Ordering::SeqCst));
        assert!(!dirty(&session));
    }

    #[test]
    fn phase_commit_survives_a_later_cancel_and_the_next_phase_fails() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let job_session = session.clone();
        let (status, error) = run(
            &registry,
            |_| {},
            move |job| {
                commit(&job_session, ticket, &job, Finality::Phase, |open| {
                    open.edit(|project| project.name = "phase one".into());
                    Ok(((), true))
                })?;
                job.cancel().cancel();
                commit(&job_session, ticket, &job, Finality::Phase, |_| {
                    Ok(((), true))
                })?;
                Ok(JobResult::Stopped)
            },
        );
        assert_eq!(status, JobStatus::Cancelled);
        assert_eq!(error.unwrap().code, ErrorCode::Cancelled);
        assert_eq!(
            lock(&session).require().unwrap().project().name,
            "phase one"
        );
        assert!(dirty(&session));
    }

    #[test]
    fn streaming_commit_checks_only_the_ticket_even_when_cancelled() {
        let registry = JobRegistry::default();
        let stale_session = opened();
        let stale = lock(&stale_session).ticket().unwrap();
        let stale_ran = Arc::new(AtomicBool::new(false));
        let (job_session, job_ran) = (stale_session.clone(), stale_ran.clone());
        let (_, error) = run(
            &registry,
            |_| replace(&stale_session),
            move |job| {
                commit(&job_session, stale, &job, Finality::Streaming, |_| {
                    job_ran.store(true, Ordering::SeqCst);
                    Ok(((), true))
                })
                .map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(error.unwrap().code, ErrorCode::StaleInputs);
        assert!(!stale_ran.load(Ordering::SeqCst));

        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let (job_session, job_ran) = (session.clone(), ran.clone());
        let (status, error) = run(
            &registry,
            |job| {
                registry.cancel(job).unwrap();
            },
            move |job| {
                commit(&job_session, ticket, &job, Finality::Streaming, |_| {
                    job_ran.store(true, Ordering::SeqCst);
                    Ok(((), true))
                })
                .map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(status, JobStatus::Cancelled);
        assert_eq!(error.unwrap().code, ErrorCode::Cancelled);
        assert!(ran.load(Ordering::SeqCst));
        assert!(dirty(&session));
    }
}

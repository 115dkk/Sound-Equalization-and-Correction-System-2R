use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use secs_project::{
    DspProfile, MeasurementStore, Project, PyJson, RemovedPosition, load_project_sources,
    measured_positions, plan_from_project,
};
use secs_types::ipc::{ErrorCode, IpcError, MigrationWarning, ProjectInfo};

use crate::jobs::JobContext;
use crate::{ServiceError, UuidSource, lock};

#[derive(Debug, Default)]
pub(crate) struct Session {
    open: Option<OpenProject>,
    last_export: Option<secs_types::local_api::LastExport>,
}

#[derive(Debug)]
pub(crate) struct OpenProject {
    pub path: PathBuf,
    pub store: MeasurementStore,
    pub dirty: bool,
    pub tutorial: bool,
    /// A tutorial's private folder, removed with the session (TD-90).
    pub tutorial_folder: Option<TutorialFolder>,
    pub results: crate::results::ResultBook,
    /// Read only through [`Session::ticket`] and compared only by
    /// [`commit`] and [`apply`] (ADR 0009).
    identity: u64,
    pub undo: std::collections::VecDeque<UndoEntry>,
}

/// The private folder a tutorial session writes its measurements and
/// results into instead of the given path (`temp_dir()/secs2r-tutorial-<pid>-<n>`).
/// Dropping it removes the folder, best effort, so closing, replacing or
/// opening another project cleans up (TD-90).
#[derive(Debug)]
pub(crate) struct TutorialFolder {
    folder: PathBuf,
    /// The path the screen gave, reported back unchanged.
    requested: PathBuf,
}

impl TutorialFolder {
    fn create(requested: PathBuf) -> Result<Self, ServiceError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let folder = std::env::temp_dir().join(format!(
            "secs2r-tutorial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A folder of that name can only be left by an earlier process with this id.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder)?;
        Ok(Self { folder, requested })
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
            Some(TutorialFolder::create(path.clone())?)
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
        let store = MeasurementStore::open(&path)?;
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
            let next = prepare_save_as(open, &path)?;
            open.path = path;
            open.store = next.finish();
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

/// The open project a request started from (ADR 0009). A job takes one when
/// its handler starts and changes the project only through [`commit`] or
/// [`apply`], so a late result of an earlier project never lands in a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectTicket(u64);

/// Commits a finishing job's change (ADR 0009). Under the session lock: a
/// different open project fails with `StaleInputs` ("project changed while
/// computing"), then `job.begin_commit()` fails with `Cancelled` if the job
/// was cancelled; only then `change` runs. `change` returns its value and
/// whether it changed the project; `commit` sets `dirty` when it did.
pub(crate) fn commit<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    job: &JobContext,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let mut session = lock(session);
    let open = session.holding(ticket)?;
    job.begin_commit()?;
    run_change(open, change)
}

/// Commits one completed phase while leaving a multi-phase job cancellable.
/// The project ticket and cancellation token are checked under the session
/// lock before `change` runs. A cancellation after this commit stops the next
/// phase without undoing this one.
pub(crate) fn commit_phase<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    job: &JobContext,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let mut session = lock(session);
    let open = session.holding(ticket)?;
    if job.cancel().is_cancelled() {
        return Err(IpcError::new(ErrorCode::Cancelled, "cancelled"));
    }
    run_change(open, change)
}

/// The same check for a job that keeps running and writes as it goes (the
/// level monitor): the ticket check only, no `begin_commit`, so the job
/// stays cancellable.
pub(crate) fn apply<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let mut session = lock(session);
    let open = session.holding(ticket)?;
    run_change(open, change)
}

fn run_change<T>(
    open: &mut OpenProject,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError> {
    let (value, changed) = change(open)?;
    if changed {
        open.dirty = true;
    }
    Ok(value)
}

fn next_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn push_position_undo(open: &mut OpenProject, removed: RemovedPosition) -> String {
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

#[derive(Debug)]
pub(crate) struct OriginalSnapshot {
    path: PathBuf,
    project: Project,
}

pub(crate) struct LoadedOriginals<'a> {
    pub values: Vec<secs_export::Original>,
    _permit: secs_compute::MemoryPermit<'a>,
}

#[derive(Debug)]
pub(crate) struct MeasurementSnapshot {
    path: PathBuf,
    project: Project,
}

pub(crate) struct LoadedMeasurements {
    pub project: Project,
    pub values: secs_engine::MeasurementSet,
}

pub(crate) fn original_snapshot(open: &OpenProject) -> OriginalSnapshot {
    OriginalSnapshot {
        path: open.path.clone(),
        project: open.store.project().clone(),
    }
}

pub(crate) fn measurement_snapshot(open: &OpenProject) -> MeasurementSnapshot {
    MeasurementSnapshot {
        path: open.path.clone(),
        project: open.store.project().clone(),
    }
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

impl MeasurementSnapshot {
    pub(crate) fn load(
        self,
        job: &JobContext,
        budget: &secs_compute::MemoryBudget,
    ) -> Result<LoadedMeasurements, IpcError> {
        let permit = reserve_measurements(&self.path, &self.project, job, budget)?;
        let mut store = MeasurementStore::new(&self.path, self.project.clone());
        let values = super::calibration::load_measurements_cancellable(&mut store, job.cancel())?;
        drop(permit);
        Ok(LoadedMeasurements {
            project: self.project,
            values,
        })
    }
}

impl OriginalSnapshot {
    pub(crate) fn load<'a>(
        self,
        job: &JobContext,
        budget: &'a secs_compute::MemoryBudget,
    ) -> Result<LoadedOriginals<'a>, IpcError> {
        let mut store = MeasurementStore::new(&self.path, self.project);
        let mut records = store.records();
        records.sort_by_key(|record| record.pos);
        let project = store.project().clone();
        let plan_roles = plan_from_project(&project, std::iter::empty()).roles;
        let sources = load_project_sources(&project, &self.path);
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
                let source_info = sources
                    .get(&position)
                    .and_then(|by_role| by_role.get(&role))
                    .cloned();
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

pub(crate) fn original_measurements(
    open: &mut OpenProject,
) -> Result<Vec<secs_export::Original>, ServiceError> {
    let mut records = open.store.records();
    records.sort_by_key(|record| record.pos);
    let project = open.store.project().clone();
    let plan_roles = plan_from_project(&project, std::iter::empty()).roles;
    let sources = load_project_sources(&project, &open.path);
    let mut originals = Vec::new();
    for record in records {
        let position = u32::try_from(record.pos).map_err(|_| {
            ServiceError::Project(format!("invalid measurement position {}", record.pos))
        })?;
        let mut roles = record.ir.keys().cloned().collect::<Vec<_>>();
        roles.sort_by_key(|role| {
            plan_roles
                .iter()
                .position(|planned| planned == role)
                .unwrap_or(plan_roles.len())
        });
        for role in roles {
            let (samples, sample_rate) = open.store.ir_of(&role, record.pos)?;
            let source_info = sources
                .get(&position)
                .and_then(|by_role| by_role.get(&role))
                .cloned();
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
    Ok(originals)
}

fn write_side_mdat(
    open: &mut OpenProject,
    uuid: &UuidSource,
    now_ms: i64,
) -> Result<(), ServiceError> {
    let originals = original_measurements(open)?;
    let inputs = originals
        .iter()
        .map(|original| original.item.clone())
        .collect::<Vec<_>>();
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

#[cfg(test)]
mod commit_tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::{Arc, Condvar};
    use std::time::{Duration, Instant};

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
            .start(JobKind::Calibrate, events.clone(), move |job| {
                gate.recv().expect("the test releases the job");
                body(job)
            })
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
    fn commit_with_a_stale_ticket_fails_before_begin_commit_and_change() {
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
                let outcome = commit(&job_session, ticket, &job, |_| {
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
    fn commit_of_a_cancelled_job_fails_with_cancelled_before_change() {
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
                commit(&job_session, ticket, &job, |_| {
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
    fn a_stale_ticket_is_reported_before_cancellation() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let job_session = session.clone();
        let (_, error) = run(
            &registry,
            |job| {
                registry.cancel(job).unwrap();
                replace(&session);
            },
            move |job| {
                commit(
                    &job_session,
                    ticket,
                    &job,
                    |_| -> Result<((), bool), IpcError> {
                        unreachable!("change runs only for the current project")
                    },
                )
                .map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(
            error.expect("the commit fails").code,
            ErrorCode::StaleInputs
        );
    }

    #[test]
    fn commit_with_a_valid_ticket_runs_change_after_begin_commit_and_dirty_follows_it() {
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
                    let value = commit(&job_session, ticket, &job, |open| {
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
    fn a_failing_change_leaves_dirty_alone() {
        let registry = JobRegistry::default();
        let session = opened();
        let ticket = lock(&session).ticket().unwrap();
        let job_session = session.clone();
        let (status, error) = run(
            &registry,
            |_| {},
            move |job| {
                commit(
                    &job_session,
                    ticket,
                    &job,
                    |_| -> Result<((), bool), IpcError> {
                        Err(IpcError::new(ErrorCode::ProjectIo, "disk full"))
                    },
                )
                .map(|()| JobResult::Stopped)
            },
        );
        assert_eq!(status, JobStatus::Failed);
        assert_eq!(error.unwrap().code, ErrorCode::ProjectIo);
        assert!(!dirty(&session));
    }

    #[test]
    fn apply_checks_the_ticket_without_begin_commit() {
        let session = opened();
        let stale = lock(&session).ticket().unwrap();
        replace(&session);
        let mut ran = false;
        let error = apply(&session, stale, |_| {
            ran = true;
            Ok(((), true))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleInputs);
        assert_eq!(error.detail, "project changed while computing");
        assert!(!ran);
        assert!(!dirty(&session));

        let ticket = lock(&session).ticket().unwrap();
        assert_eq!(apply(&session, ticket, |_| Ok((7, false))).unwrap(), 7);
        assert!(!dirty(&session));
        apply(&session, ticket, |_| Ok(((), true))).unwrap();
        assert!(dirty(&session));

        // A job that writes as it goes stays cancellable.
        let registry = JobRegistry::default();
        let still_cancellable = Arc::new(AtomicBool::new(false));
        let (job_session, job_cancellable, job_registry) =
            (session.clone(), still_cancellable.clone(), registry.clone());
        let (status, error) = run(
            &registry,
            |_| {},
            move |job| {
                apply(&job_session, ticket, |_| Ok(((), false)))?;
                job_cancellable.store(cancellable(&job_registry, job.id()), Ordering::SeqCst);
                Ok(JobResult::Stopped)
            },
        );
        assert_eq!((status, error), (JobStatus::Succeeded, None));
        assert!(still_cancellable.load(Ordering::SeqCst));
        assert!(dirty(&session));
    }
}

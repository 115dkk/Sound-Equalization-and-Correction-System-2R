use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use secs_service::{EventSink, LocalTime};
use secs_types::ipc::{JobEvent, JobId, JobKind};

const LOG_LIMIT: u64 = 2 * 1024 * 1024;
const ROTATED_FILES: usize = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug)]
struct StartRecord {
    at: Instant,
    kind: JobKind,
    logged: bool,
}

#[derive(Debug)]
struct State {
    file: Option<File>,
    rotate_after: u64,
    started: HashMap<JobId, StartRecord>,
}

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    limit: u64,
    rotate: fn(&Path) -> bool,
    state: Mutex<State>,
}

/// Best-effort application log. Every failure is deliberately ignored.
#[derive(Debug, Clone)]
pub(crate) struct AppLog {
    inner: Arc<Inner>,
}

impl AppLog {
    pub(crate) fn open(dir: &Path) -> AppLog {
        Self::open_with_limit(dir, LOG_LIMIT)
    }

    fn open_with_limit(dir: &Path, limit: u64) -> AppLog {
        Self::open_with_limit_and_rotate(dir, limit, rotate)
    }

    fn open_with_limit_and_rotate(dir: &Path, limit: u64, rotate: fn(&Path) -> bool) -> AppLog {
        let _ = fs::create_dir_all(dir);
        let path = dir.join("secs2r.log");
        let file = append_file(&path);
        AppLog {
            inner: Arc::new(Inner {
                path,
                limit,
                rotate,
                state: Mutex::new(State {
                    file,
                    rotate_after: limit,
                    started: HashMap::new(),
                }),
            }),
        }
    }

    pub(crate) fn line(&self, level: Level, area: &str, text: &str) {
        let now = SystemTime::now();
        let offset = secs_sys::clock::utc_offset_seconds(now);
        let local = LocalTime::from_system(now, offset);
        let millis = now
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.subsec_millis());
        let sign = if offset < 0 { '-' } else { '+' };
        let offset = offset.unsigned_abs();
        let stamp = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}{sign}{:02}:{:02}",
            local.year,
            local.month,
            local.day,
            local.hour,
            local.minute,
            local.second,
            millis,
            offset / 3600,
            offset % 3600 / 60,
        );
        let text = redact(&text.replace('\r', "\\r").replace('\n', "\\n"));
        self.write(&format!("{stamp} {} {area} {text}\n", level.name()));
    }

    fn write(&self, bytes: &str) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        let length = state
            .file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map(|metadata| metadata.len());
        if let Some(length) = length
            && length > state.rotate_after
        {
            state.file.take();
            let rotated = (self.inner.rotate)(&self.inner.path);
            state.rotate_after = if rotated {
                self.inner.limit
            } else {
                length.saturating_add(self.inner.limit)
            };
            state.file = append_file(&self.inner.path);
        }
        if state.file.is_none() {
            state.file = append_file(&self.inner.path);
        }
        if let Some(file) = state.file.as_mut() {
            let _ = file.write_all(bytes.as_bytes()).and_then(|()| file.flush());
        }
    }
}

impl EventSink for AppLog {
    fn send(&self, event: JobEvent) {
        match event {
            JobEvent::Started { job, kind } => {
                let deferred = matches!(
                    kind,
                    JobKind::Calibrate | JobKind::RecommendXo | JobKind::Mimo
                );
                if let Ok(mut state) = self.inner.state.lock() {
                    state.started.insert(
                        job,
                        StartRecord {
                            at: Instant::now(),
                            kind,
                            logged: !deferred,
                        },
                    );
                }
                if !deferred {
                    self.line(Level::Info, "job", &format!("{} {:?} started", job.0, kind));
                }
            }
            JobEvent::Log { job, code, params } => {
                let pending_kind = self.inner.state.lock().ok().and_then(|mut state| {
                    let record = state.started.get_mut(&job)?;
                    if record.logged {
                        None
                    } else {
                        record.logged = true;
                        Some(record.kind)
                    }
                });
                if let Some(kind) = pending_kind {
                    let background = params.get("background").map_or("false", String::as_str);
                    self.line(
                        Level::Info,
                        "job",
                        &format!("{} {:?} started background={background}", job.0, kind),
                    );
                }
                let params = params
                    .into_iter()
                    .map(|(key, value)| format!("{key}={}", log_value(&value)))
                    .collect::<Vec<_>>()
                    .join(" ");
                let suffix = if params.is_empty() {
                    String::new()
                } else {
                    format!(" {params}")
                };
                self.line(Level::Info, "job", &format!("{} {code}{suffix}", job.0));
            }
            JobEvent::Finished {
                job, status, error, ..
            } => {
                let started = self
                    .inner
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| state.started.remove(&job));
                if let Some(record) = &started
                    && !record.logged
                {
                    self.line(
                        Level::Info,
                        "job",
                        &format!("{} {:?} started background=false", job.0, record.kind),
                    );
                }
                let elapsed = started.map_or(0, |record| record.at.elapsed().as_millis());
                let mut text = format!("{} finished status={status:?} elapsed_ms={elapsed}", job.0);
                let level = if let Some(error) = error {
                    text.push_str(&format!(
                        " error_code={:?} message={}",
                        error.code,
                        log_value(&error.detail)
                    ));
                    Level::Error
                } else if status == secs_types::ipc::JobStatus::Cancelled {
                    Level::Warn
                } else {
                    Level::Info
                };
                self.line(level, "job", &text);
            }
            JobEvent::Progress { .. } | JobEvent::Meter { .. } | JobEvent::MeasureStatus { .. } => {
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct FanOut {
    pub(crate) first: Arc<dyn EventSink>,
    pub(crate) second: Arc<dyn EventSink>,
}

impl EventSink for FanOut {
    fn send(&self, event: JobEvent) {
        self.first.send(event.clone());
        self.second.send(event);
    }
}

fn append_file(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn rotate(path: &Path) -> bool {
    let rotated = |index: usize| path.with_file_name(format!("secs2r.{index}.log"));
    let _ = fs::remove_file(rotated(ROTATED_FILES));
    for index in (1..ROTATED_FILES).rev() {
        let _ = fs::rename(rotated(index), rotated(index + 1));
    }
    fs::rename(path, rotated(1)).is_ok()
}

pub(crate) fn log_value(value: &str) -> String {
    if value.chars().any(char::is_whitespace) || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

pub(crate) fn redact(text: &str) -> String {
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    preferred_home(user_profile, home, cfg!(windows))
        .as_deref()
        .map_or_else(
            || text.to_owned(),
            |home| redact_home(text, &home.to_string_lossy(), cfg!(windows)),
        )
}

fn preferred_home(
    user_profile: Option<PathBuf>,
    home: Option<PathBuf>,
    windows: bool,
) -> Option<PathBuf> {
    if windows {
        user_profile.or(home)
    } else {
        home.or(user_profile)
    }
}

fn redact_home(text: &str, home: &str, insensitive: bool) -> String {
    if home.is_empty() {
        return text.to_owned();
    }
    let backslashes = home.replace('/', "\\");
    let mut variants = vec![
        backslashes.replace('\\', "\\\\"),
        backslashes,
        home.replace('\\', "/"),
        home.to_owned(),
    ];
    variants.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    variants.dedup();
    variants.into_iter().fold(text.to_owned(), |text, home| {
        redact_one_home(&text, &home, insensitive)
    })
}

fn redact_one_home(text: &str, home: &str, insensitive: bool) -> String {
    if !insensitive {
        return text.replace(home, "~");
    }
    let folded_text = text.to_ascii_lowercase();
    let folded_home = home.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    while let Some(relative) = folded_text[start..].find(&folded_home) {
        let found = start + relative;
        out.push_str(&text[start..found]);
        out.push('~');
        start = found + home.len();
    }
    out.push_str(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use secs_types::ipc::JobStatus;

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);
    static ROTATE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

    fn fail_rotation(_path: &Path) -> bool {
        ROTATE_ATTEMPTS.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "secs2r-applog-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rotation_keeps_five_files_and_newest_line_in_current() {
        let root = root("rotation");
        let log = AppLog::open_with_limit(&root, 64);
        for index in 0..20 {
            log.line(
                Level::Info,
                "test",
                &format!("line={index} padding=abcdefghij"),
            );
        }
        let files = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("secs2r"))
            .count();
        assert!(files <= 5, "found {files} log files");
        let current = fs::read_to_string(root.join("secs2r.log")).unwrap();
        assert!(current.contains("line=19"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_rotation_waits_for_another_limit_of_growth_before_retrying() {
        ROTATE_ATTEMPTS.store(0, Ordering::SeqCst);
        let root = root("rotation-failure");
        let log = AppLog::open_with_limit_and_rotate(&root, 10, fail_rotation);
        log.write("12345678901");
        log.write("a");
        assert_eq!(ROTATE_ATTEMPTS.load(Ordering::SeqCst), 1);
        log.write("1234567890");
        assert_eq!(ROTATE_ATTEMPTS.load(Ordering::SeqCst), 1);
        log.write("b");
        assert_eq!(ROTATE_ATTEMPTS.load(Ordering::SeqCst), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn redaction_replaces_the_home_directory() {
        let home = r"c:\users\person";
        assert_eq!(
            redact_home(r"C:\Users\Person\room.secs", home, true),
            r"~\room.secs"
        );
        assert_eq!(
            redact_home("C:/Users/Person/room.secs", home, true),
            "~/room.secs"
        );
        assert_eq!(
            redact_home(r"C:\\Users\\Person\\room.secs", home, true),
            r"~\\room.secs"
        );
        assert_eq!(
            redact_home("/home/person/room.secs", "/home/person", false),
            "~/room.secs"
        );
        assert_eq!(
            preferred_home(
                Some(PathBuf::from(r"C:\Users\profile")),
                Some(PathBuf::from(r"C:\Users\home")),
                true,
            ),
            Some(PathBuf::from(r"C:\Users\profile"))
        );
    }

    #[test]
    fn log_values_with_spaces_are_quoted() {
        assert_eq!(log_value("project is not open"), r#""project is not open""#);
    }

    #[test]
    fn event_sink_writes_started_log_finished_and_skips_progress() {
        let root = root("events");
        let log = AppLog::open(&root);
        let job = JobId(7);
        log.send(JobEvent::Started {
            job,
            kind: JobKind::Calibrate,
        });
        log.send(JobEvent::Progress {
            job,
            stage: "engine.test".into(),
            done: 1,
            total: 2,
        });
        log.send(JobEvent::Log {
            job,
            code: "calibration_settings".into(),
            params: BTreeMap::from([
                ("engine_key".into(), "key with space".into()),
                ("background".into(), "true".into()),
            ]),
        });
        log.send(JobEvent::Finished {
            job,
            status: JobStatus::Succeeded,
            result: None,
            error: None,
        });
        let text = fs::read_to_string(root.join("secs2r.log")).unwrap();
        assert!(text.contains("7 Calibrate started background=true"));
        assert!(
            text.contains("calibration_settings background=true engine_key=\"key with space\"")
        );
        assert!(text.contains("finished status=Succeeded elapsed_ms="));
        assert!(!text.contains("engine.test"));
        fs::remove_dir_all(root).unwrap();
    }
}

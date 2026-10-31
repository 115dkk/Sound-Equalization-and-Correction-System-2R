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
    started: HashMap<JobId, StartRecord>,
}

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    limit: u64,
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
        let _ = fs::create_dir_all(dir);
        let path = dir.join("secs2r.log");
        let file = append_file(&path);
        AppLog {
            inner: Arc::new(Inner {
                path,
                limit,
                state: Mutex::new(State {
                    file,
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
        if state
            .file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .is_some_and(|metadata| metadata.len() > self.inner.limit)
        {
            state.file.take();
            rotate(&self.inner.path);
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

fn rotate(path: &Path) {
    let rotated = |index: usize| path.with_file_name(format!("secs2r.{index}.log"));
    let _ = fs::remove_file(rotated(ROTATED_FILES));
    for index in (1..ROTATED_FILES).rev() {
        let _ = fs::rename(rotated(index), rotated(index + 1));
    }
    let _ = fs::rename(path, rotated(1));
}

fn log_value(value: &str) -> String {
    if value.chars().any(char::is_whitespace) || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

pub(crate) fn redact(text: &str) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    home.as_deref().map_or_else(
        || text.to_owned(),
        |home| redact_home(text, &home.to_string_lossy(), cfg!(windows)),
    )
}

fn redact_home(text: &str, home: &str, insensitive: bool) -> String {
    if home.is_empty() {
        return text.to_owned();
    }
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
    fn redaction_replaces_the_home_directory() {
        assert_eq!(
            redact_home(r"C:\Users\Person\room.secs", r"c:\users\person", true),
            r"~\room.secs"
        );
        assert_eq!(
            redact_home("/home/person/room.secs", "/home/person", false),
            "~/room.secs"
        );
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

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use secs_types::chain::MimoPaths;
use secs_types::ipc::{CalibrationSummary, MimoSummary};
use serde::{Deserialize, Serialize};

const VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedRunLabels {
    pub mode: String,
    pub ultra_ms: f64,
    pub latency_ms: f64,
    pub rc_hi_hz: f64,
    pub mimo_max_supports: usize,
}
const MAX_CACHE_BYTES: u64 = 8 * 1024 * 1024;
const CACHE_NAME: &str = "secs_run_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedResults {
    pub calibration: CalibrationSummary,
    pub mimo: Option<MimoSummary>,
    pub measurement_signature: String,
    pub target_key: String,
    pub engine_key: String,
    pub mimo_key: Option<String>,
    pub mimo_calibration_key: Option<String>,
    pub labels: CachedRunLabels,
    pub mimo_paths: Option<MimoPaths>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    results: CachedResults,
}

pub(crate) fn load(project_path: &Path) -> Option<CachedResults> {
    let path = path(project_path);
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > MAX_CACHE_BYTES {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if value.get("version")?.as_u64()? != u64::from(VERSION) {
        return None;
    }
    // Version 2 already stored the run's mode and budget in labels. Restore
    // missing summary fields from that source, never from today's settings.
    let results = value.get_mut("results")?;
    let mode = results.get("labels")?.get("mode")?.clone();
    let ultra_ms = results.get("labels")?.get("ultra_ms")?.clone();
    let summary = results.get_mut("calibration")?.as_object_mut()?;
    summary.entry("mode").or_insert(mode);
    summary.entry("ultra_ms").or_insert(ultra_ms);
    summary
        .entry("stale_reason")
        .or_insert(serde_json::Value::Null);
    let cache: CacheFile = serde_json::from_value(value).ok()?;
    Some(cache.results)
}

pub(crate) fn save(project_path: &Path, results: &CachedResults) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec(&CacheFile {
        version: VERSION,
        results: results.clone(),
    })
    .map_err(std::io::Error::other)?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(std::io::Error::other("run cache exceeds size limit"));
    }
    let destination = path(project_path);
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    static TEMP: AtomicU64 = AtomicU64::new(0);
    let mut temporary = None;
    for _ in 0..100 {
        let candidate = parent.join(format!(
            ".secs-run-cache-{}-{}.tmp",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some((temporary, mut file)) = temporary else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create run cache temporary file",
        ));
    };
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn path(project_path: &Path) -> PathBuf {
    project_path.with_extension("irs").join(CACHE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_types::chain::SignalChain;
    use secs_types::ipc::{ChannelScore, VerdictSummary};

    fn sample() -> CachedResults {
        CachedResults {
            calibration: CalibrationSummary {
                stale: false,
                stale_reason: None,
                mode: secs_types::calibration::Mode::Mixed,
                ultra_ms: 100.0,
                verdict: VerdictSummary {
                    within_1db_before_pct: 0.0,
                    within_1db_after_pct: 1.0,
                    low_band_time_spread_before_ms: 2.0,
                    low_band_time_spread_after_ms: 1.0,
                    added_delay_ms: 4.0,
                    position_spread_db: 0.0,
                },
                scorecard: Vec::<ChannelScore>::new(),
                latency_ms: 4.0,
                notes: vec![],
                chain: SignalChain {
                    sample_rate: 48_000,
                    mains: vec![],
                    subs: vec![],
                    sub_bus_peq: vec![],
                    main_extra_ms: 0.0,
                    lift_ms: 0.0,
                    latency_ms: 4.0,
                },
            },
            mimo: None,
            measurement_signature: "m".into(),
            target_key: "t".into(),
            engine_key: "e".into(),
            mimo_key: None,
            mimo_calibration_key: None,
            labels: CachedRunLabels {
                mode: "mixed".into(),
                ultra_ms: 100.0,
                latency_ms: 4.0,
                rc_hi_hz: 300.0,
                mimo_max_supports: 0,
            },
            mimo_paths: None,
        }
    }

    #[test]
    fn round_trip_and_ignore_pickle() {
        let root = std::env::temp_dir().join(format!("secs-cache-{}", std::process::id()));
        let project = root.join("project.secs");
        fs::create_dir_all(project.with_extension("irs")).unwrap();
        fs::write(
            project.with_extension("irs").join("calibration_cache.pkl"),
            b"pickle",
        )
        .unwrap();
        save(&project, &sample()).unwrap();
        assert_eq!(load(&project).unwrap().engine_key, "e");
        let mut replacement = sample();
        replacement.engine_key = "replacement".into();
        save(&project, &replacement).unwrap();
        assert_eq!(load(&project).unwrap().engine_key, "replacement");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_and_oversized_files_are_ignored() {
        let root = std::env::temp_dir().join(format!("secs-cache-bad-{}", std::process::id()));
        let project = root.join("project.secs");
        fs::create_dir_all(project.with_extension("irs")).unwrap();
        let path = path(&project);
        fs::write(&path, b"not json").unwrap();
        assert!(load(&project).is_none());
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_CACHE_BYTES + 1).unwrap();
        assert!(load(&project).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_two_missing_summary_fields_restore_run_labels() {
        let root =
            std::env::temp_dir().join(format!("secs-cache-migration-{}", std::process::id()));
        let project = root.join("project.secs");
        fs::create_dir_all(project.with_extension("irs")).unwrap();
        for mode in ["zero", "mixed", "linear", "ultra"] {
            let mut value = serde_json::to_value(CacheFile {
                version: VERSION,
                results: sample(),
            })
            .unwrap();
            value["results"]["labels"]["mode"] = mode.into();
            value["results"]["labels"]["ultra_ms"] = 175.0.into();
            let summary = value["results"]["calibration"].as_object_mut().unwrap();
            for field in ["stale_reason", "mode", "ultra_ms"] {
                summary.remove(field);
            }
            fs::write(path(&project), serde_json::to_vec(&value).unwrap()).unwrap();
            let restored = load(&project).unwrap().calibration;
            assert_eq!(serde_json::to_value(restored.mode).unwrap(), mode);
            assert_eq!(restored.ultra_ms, 175.0);
            assert_eq!(restored.stale_reason, None);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_cache_version_is_ignored() {
        let root = std::env::temp_dir().join(format!("secs-cache-version-{}", std::process::id()));
        let project = root.join("project.secs");
        fs::create_dir_all(project.with_extension("irs")).unwrap();
        let bytes = serde_json::to_vec(&CacheFile {
            version: VERSION + 1,
            results: sample(),
        })
        .unwrap();
        fs::write(path(&project), bytes).unwrap();
        assert!(load(&project).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}

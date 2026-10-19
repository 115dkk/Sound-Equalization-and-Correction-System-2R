use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use secs_types::chain::{MimoPaths, SignalChain};
use secs_types::ipc::{CalibrationSummary, MimoSummary};
use serde::{Deserialize, Serialize};

const VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedRunLabels {
    pub mode: String,
    pub ultra_ms: f64,
    pub latency_ms: f64,
    pub rc_hi_hz: f64,
    pub mimo_max_supports: usize,
}
// 132 x 8,000 support taps alone serialize to about 20 MiB; retain a
// bounded 64 MiB envelope for those paths plus calibration FIRs (ADR 0009).
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const CACHE_NAME: &str = "secs_run_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedResults {
    pub calibration: CalibrationSummary,
    pub calibration_chain: SignalChain,
    pub mimo: Option<MimoSummary>,
    pub measurement_signature: String,
    pub target_key: String,
    pub engine_key: String,
    pub mimo_key: Option<String>,
    pub mimo_calibration_key: Option<String>,
    pub labels: CachedRunLabels,
    pub mimo_paths: Option<MimoPaths>,
    #[serde(default)]
    pub mimo_compute_key: crate::compute::ComputeIdentity,
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
    // Current caches deserialize directly into their owned DSP arrays. Only
    // legacy layouts need a temporary JSON tree for moving the old fields.
    if let Ok(file) = serde_json::from_slice::<CacheFile>(&bytes) {
        return (file.version == VERSION).then_some(file.results);
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let version = value.get("version")?.as_u64()?;
    if version != 2 && version != 3 {
        return None;
    }
    // Version 2 already stored the run's mode and budget in labels. Restore
    // missing summary fields from that source, never from today's settings.
    let results = value.get_mut("results")?;
    let mode = results.get("labels")?.get("mode")?.clone();
    let ultra_ms = results.get("labels")?.get("ultra_ms")?.clone();
    let summary = results.get_mut("calibration")?.as_object_mut()?;
    let chain = summary.remove("chain")?;
    let roles = |field: &str| -> Option<serde_json::Value> {
        Some(serde_json::Value::Array(
            chain
                .get(field)?
                .as_array()?
                .iter()
                .map(|channel| channel.get("role").cloned())
                .collect::<Option<Vec<_>>>()?,
        ))
    };
    summary.insert("main_roles".into(), roles("mains")?);
    summary.insert("sub_roles".into(), roles("subs")?);
    summary.entry("mode").or_insert(mode);
    summary.entry("ultra_ms").or_insert(ultra_ms);
    summary
        .entry("stale_reason")
        .or_insert(serde_json::Value::Null);
    results
        .as_object_mut()?
        .insert("calibration_chain".into(), chain);
    let input_count = results
        .get("mimo_paths")
        .and_then(|paths| paths.get("paths"))
        .and_then(serde_json::Value::as_array)
        .map_or(0, |paths| {
            paths
                .iter()
                .filter_map(|path| path.get("input").and_then(serde_json::Value::as_str))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        });
    if let Some(mimo) = results
        .get_mut("mimo")
        .and_then(serde_json::Value::as_object_mut)
    {
        mimo.remove("paths");
        mimo.insert("input_count".into(), input_count.into());
        mimo.entry("compute").or_insert(
            serde_json::to_value(secs_types::compute::ComputeReport {
                requested: secs_types::compute::ComputePreference::Cpu,
                ..Default::default()
            })
            .ok()?,
        );
    }
    let cache: CacheFile = serde_json::from_value(value).ok()?;
    Some(cache.results)
}

pub(crate) fn save(project_path: &Path, results: &CachedResults) -> Result<(), std::io::Error> {
    save_with_limit(project_path, results, MAX_CACHE_BYTES)
}

fn save_with_limit(
    project_path: &Path,
    results: &CachedResults,
    limit: u64,
) -> Result<(), std::io::Error> {
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
    let Some((temporary, file)) = temporary else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create run cache temporary file",
        ));
    };
    let result = (|| {
        let mut writer = LimitedWriter {
            inner: BufWriter::new(file),
            remaining: limit,
        };
        serde_json::to_writer(
            &mut writer,
            &CacheFileRef {
                version: VERSION,
                results,
            },
        )
        .map_err(std::io::Error::other)?;
        writer.flush()?;
        writer.inner.get_ref().sync_all()?;
        drop(writer);
        fs::rename(&temporary, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[derive(Serialize)]
struct CacheFileRef<'a> {
    version: u32,
    results: &'a CachedResults,
}

struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
}
impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("run cache exceeds size limit"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
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
                main_roles: vec![],
                sub_roles: vec![],
            },
            calibration_chain: SignalChain {
                sample_rate: 48_000,
                mains: vec![],
                subs: vec![],
                sub_bus_peq: vec![],
                main_extra_ms: 0.0,
                lift_ms: 0.0,
                latency_ms: 4.0,
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
            mimo_compute_key: Default::default(),
        }
    }

    fn legacy_value(results: CachedResults, version: u32) -> serde_json::Value {
        let mut value = serde_json::to_value(CacheFile { version, results }).unwrap();
        let fields = value["results"].as_object_mut().unwrap();
        let chain = fields.remove("calibration_chain").unwrap();
        let paths = fields["mimo_paths"].clone();
        let calibration = fields
            .get_mut("calibration")
            .unwrap()
            .as_object_mut()
            .unwrap();
        calibration.remove("main_roles");
        calibration.remove("sub_roles");
        calibration.insert("chain".into(), chain);
        if let Some(mimo) = fields
            .get_mut("mimo")
            .and_then(serde_json::Value::as_object_mut)
        {
            mimo.remove("input_count");
            mimo.insert("paths".into(), paths);
        }
        value
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
            let mut value = legacy_value(sample(), 2);
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
    fn compute_provenance_round_trips_and_legacy_mimo_defaults_to_cpu() {
        use secs_types::compute::*;
        let root = std::env::temp_dir().join(format!("secs-cache-compute-{}", std::process::id()));
        let project = root.join("project.secs");
        let mut run = sample();
        run.mimo = Some(MimoSummary {
            stale: false,
            kind: "fir".into(),
            fsiso: 120.0,
            support_count: 0,
            fir_taps_total: 0,
            notes: vec![],
            input_count: 0,
            compute: ComputeReport {
                requested: ComputePreference::Gpu,
                backend: ComputeBackend::Cuda,
                device: Some("Test GPU".into()),
                stage: Some(ComputeStage::IdealFirSweep),
                accelerated_systems: 2,
                cpu_systems: 1,
                fallbacks: 1,
                reason: Some(ComputeReason::DeviceFailure),
            },
        });
        save(&project, &run).unwrap();
        assert_eq!(load(&project).unwrap().mimo, run.mimo);
        let mut value = legacy_value(run, 2);
        value["results"]
            .as_object_mut()
            .unwrap()
            .remove("mimo_compute_key");
        value["results"]["mimo"]
            .as_object_mut()
            .unwrap()
            .remove("compute");
        fs::write(path(&project), serde_json::to_vec(&value).unwrap()).unwrap();
        let legacy = load(&project).unwrap();
        assert_eq!(
            legacy.mimo_compute_key,
            crate::compute::ComputeIdentity::default()
        );
        assert_eq!(
            legacy.mimo.unwrap().compute,
            ComputeReport {
                requested: ComputePreference::Cpu,
                ..Default::default()
            }
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn fir_sample(path_count: usize, taps: usize) -> CachedResults {
        use secs_types::chain::{MainChannel, MimoFilter, MimoPath};
        let mut result = sample();
        let coefficients: Vec<_> = (0..taps)
            .map(|index| (index as f64 * 0.12345678901234567).sin() * 0.01)
            .collect();
        result.calibration_chain.mains.push(MainChannel {
            role: "L".into(),
            input_peq: vec![],
            fir: coefficients.clone(),
            gain_db: -0.5,
            delay_ms: 2.0,
            xo_hz: Some(80.0),
            top_peq: vec![],
            align_delay_ms: 2.0,
        });
        result.calibration.main_roles = vec!["L".into()];
        result.mimo_paths = Some(MimoPaths {
            fsiso: 120.0,
            paths: (0..path_count)
                .map(|index| MimoPath {
                    input: format!("input{}", index / 12),
                    support: format!("support{}", index % 12),
                    filter: MimoFilter::Fir {
                        taps_48k: coefficients.clone(),
                    },
                    low_hz: 30.0,
                    delay_ms: 2.0,
                })
                .collect(),
        });
        result.mimo = Some(MimoSummary {
            compute: Default::default(),
            stale: false,
            kind: "fir".into(),
            fsiso: 120.0,
            support_count: path_count,
            input_count: path_count.div_ceil(12),
            fir_taps_total: path_count * taps,
            notes: vec![],
        });
        result
    }

    #[test]
    fn large_fir_cache_round_trips_every_coefficient_without_large_screen_summaries() {
        let root = std::env::temp_dir().join(format!("secs-cache-large-{}", std::process::id()));
        let project = root.join("project.secs");
        let results = fir_sample(132, 8000);
        save(&project, &results).unwrap();
        let bytes = fs::metadata(path(&project)).unwrap().len();
        assert!(
            bytes > 8 * 1024 * 1024 && bytes < MAX_CACHE_BYTES,
            "cache bytes: {bytes}"
        );
        let loaded = load(&project).unwrap();
        assert_eq!(loaded.calibration_chain, results.calibration_chain);
        assert_eq!(loaded.mimo_paths, results.mimo_paths);
        assert_eq!(loaded.calibration, results.calibration);
        assert_eq!(loaded.mimo, results.mimo);
        let summary = serde_json::to_vec(&secs_types::ipc::ResultsSummary {
            calibration: Some(loaded.calibration),
            mimo: loaded.mimo,
        })
        .unwrap();
        assert!(summary.len() < 4096);
        eprintln!(
            "132 x 8000 cache_bytes={bytes} summary_bytes={}",
            summary.len()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_v2_and_v3_move_dsp_into_private_cache_without_changing_samples() {
        let root =
            std::env::temp_dir().join(format!("secs-cache-legacy-dsp-{}", std::process::id()));
        let project = root.join("project.secs");
        fs::create_dir_all(project.with_extension("irs")).unwrap();
        for version in [2, 3] {
            let original = fir_sample(25, 17);
            let value = legacy_value(original.clone(), version);
            fs::write(path(&project), serde_json::to_vec(&value).unwrap()).unwrap();
            let restored = load(&project).unwrap();
            assert_eq!(restored.calibration_chain, original.calibration_chain);
            assert_eq!(restored.mimo_paths, original.mimo_paths);
            assert_eq!(restored.calibration.main_roles, vec!["L"]);
            assert_eq!(restored.mimo.unwrap().input_count, 3);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejected_cache_write_preserves_previous_file_and_removes_temporary() {
        let root =
            std::env::temp_dir().join(format!("secs-cache-atomic-limit-{}", std::process::id()));
        let project = root.join("project.secs");
        save(&project, &sample()).unwrap();
        let previous = fs::read(path(&project)).unwrap();
        assert!(save_with_limit(&project, &fir_sample(2, 17), 32).is_err());
        assert_eq!(fs::read(path(&project)).unwrap(), previous);
        assert_eq!(
            fs::read_dir(project.with_extension("irs")).unwrap().count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_writer_rejects_overflow_before_writing_beyond_limit() {
        let mut writer = LimitedWriter {
            inner: Vec::new(),
            remaining: 4,
        };
        writer.write_all(b"123").unwrap();
        assert!(writer.write_all(b"45").is_err());
        assert_eq!(writer.inner, b"123");
        writer.write_all(b"4").unwrap();
        assert_eq!(writer.inner, b"1234");
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

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{SecondsFormat, TimeZone, Utc};
use secs_graph::{ExportGraph, LfeChoice};
use secs_types::chain::{MimoFilter, MimoPaths, SignalChain};
use secs_types::ipc::ExportKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::APP_NAME;

pub(crate) const MANIFEST_NAME: &str = "secs-design.json";
const INDEX_NAME: &str = "designs.json";
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub(crate) enum DesignExportError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Graph(#[from] secs_graph::GraphError),
    #[error("timestamp is outside the RFC 3339 range")]
    Timestamp,
}

#[derive(Debug, Clone)]
pub(crate) struct DesignMetadata {
    pub design_name: String,
    pub project_name: Option<String>,
    pub app_version: String,
    pub created_utc: String,
    pub export_kind: ExportKind,
    pub mode: String,
    pub latency_ms: f64,
    pub output_rate: u32,
    pub output_rate_auto: bool,
    pub chain: SignalChain,
    pub mimo: Option<MimoPaths>,
    pub bypass_config: Option<String>,
}

#[derive(Serialize)]
struct Manifest<'a> {
    schema: u32,
    design_id: &'a str,
    design_name: &'a str,
    project_name: Option<&'a str>,
    app: App<'a>,
    upstream: Upstream<'a>,
    created_utc: &'a str,
    export_kind: &'static str,
    mode: &'a str,
    latency_ms: f64,
    stages: Stages,
    sample_rate: u32,
    design_sample_rate: u32,
    output_rate_auto: bool,
    inputs: &'a [String],
    outputs: &'a [String],
    headroom_db: f64,
    bypass_config: Option<&'a str>,
    files: Vec<FileRecord>,
}

#[derive(Serialize)]
struct App<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct Upstream<'a> {
    repository: &'a str,
    commit: &'a str,
}

#[derive(Serialize)]
struct Stages {
    rc: bool,
    ba: bool,
    xo: bool,
    mimo: &'static str,
}

#[derive(Serialize)]
struct FileRecord {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DesignIndexEntry {
    design_name: String,
    design_id: String,
    export_kind: String,
    folder: String,
    created_utc: String,
    mode: String,
    mimo: String,
}

#[derive(Deserialize)]
struct ManifestIndexFields {
    design_name: String,
    design_id: String,
    export_kind: String,
    created_utc: String,
    mode: String,
    stages: ManifestStages,
}

#[derive(Deserialize)]
struct ManifestStages {
    mimo: String,
}

pub(crate) fn validate_design_name(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    let count = value.chars().count();
    if !(1..=64).contains(&count) {
        return Err("design_name_length".into());
    }
    if value.chars().any(char::is_control) {
        return Err("design_name_control".into());
    }
    if value
        .chars()
        .any(|ch| matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err("design_name_character".into());
    }
    if value.ends_with(['.', ' ']) {
        return Err("design_name_trailing".into());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        || stem
            .strip_prefix("LPT")
            .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"));
    if reserved {
        return Err("design_name_reserved".into());
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn created_utc(unix_ms: i64) -> Result<String, DesignExportError> {
    Utc.timestamp_millis_opt(unix_ms)
        .single()
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or(DesignExportError::Timestamp)
}

pub(crate) fn kind_name(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::CamillaDsp => "camilla_dsp",
        ExportKind::StagedMdat => "staged_mdat",
        ExportKind::BeforeAfterMdat => "before_after_mdat",
        ExportKind::FilterText => "filter_text",
        ExportKind::EqualizerApo => "equalizer_apo",
        ExportKind::Convolution => "convolution",
    }
}

pub(crate) fn named_folder(base: &Path, kind: ExportKind, name: &str) -> PathBuf {
    base.join(format!("SECS_{}_{}", kind_name(kind), name))
}

pub(crate) fn staging_folder(destination: &Path) -> PathBuf {
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let name = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.stage-{}-{id}", std::process::id()))
}

pub(crate) fn replace_folder(staged: &Path, destination: &Path) -> Result<(), DesignExportError> {
    if !destination.exists() {
        fs::rename(staged, destination)?;
        return Ok(());
    }
    let backup = destination.with_file_name(format!(
        ".{}.backup-{}-{}",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(staged, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error.into());
    }
    fs::remove_dir_all(backup)?;
    Ok(())
}

pub(crate) fn write_manifest(
    folder: &Path,
    metadata: &DesignMetadata,
) -> Result<(PathBuf, String), DesignExportError> {
    let graph = ExportGraph::build(&metadata.chain, metadata.mimo.as_ref(), LfeChoice::DOLBY)?;
    let design_id =
        secs_graph::design::design_id(&metadata.chain, metadata.mimo.as_ref(), LfeChoice::DOLBY)?;
    let headroom_db = (secs_graph::headroom_db(graph.plan())? * 2.0).ceil() / 2.0;
    let mimo = match metadata.mimo.as_ref() {
        None => "none",
        Some(paths) if paths.paths.is_empty() => "none",
        Some(paths)
            if paths.paths.iter().any(|path| {
                matches!(
                    path.filter,
                    MimoFilter::Fir { .. } | MimoFilter::FirLowRate { .. }
                )
            }) =>
        {
            "fir"
        }
        Some(_) => "iir",
    };
    let stages = Stages {
        rc: metadata
            .chain
            .mains
            .iter()
            .any(|main| !main.input_peq.is_empty() || main.fir.as_slice() != [1.0]),
        ba: !metadata.chain.subs.is_empty(),
        xo: metadata
            .chain
            .mains
            .iter()
            .any(|main| main.xo_hz.is_some_and(|hz| hz != 0.0)),
        mimo,
    };
    let mut files = collect_files(folder)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let source = crate::upstream();
    let manifest = Manifest {
        schema: 1,
        design_id: &design_id,
        design_name: &metadata.design_name,
        project_name: metadata.project_name.as_deref(),
        app: App {
            name: APP_NAME,
            version: &metadata.app_version,
        },
        upstream: Upstream {
            repository: &source.repository,
            commit: &source.commit,
        },
        created_utc: &metadata.created_utc,
        export_kind: kind_name(metadata.export_kind),
        mode: &metadata.mode,
        latency_ms: metadata.latency_ms,
        stages,
        sample_rate: metadata.output_rate,
        design_sample_rate: 48_000,
        output_rate_auto: metadata.output_rate_auto,
        inputs: &graph.plan().inputs,
        outputs: &graph.plan().outputs,
        headroom_db,
        bypass_config: metadata.bypass_config.as_deref(),
        files,
    };
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let path = folder.join(MANIFEST_NAME);
    secs_types::fs::write_atomic_bytes(&path, &bytes)?;
    Ok((path, design_id))
}

fn collect_files(folder: &Path) -> Result<Vec<FileRecord>, DesignExportError> {
    fn visit(root: &Path, at: &Path, out: &mut Vec<FileRecord>) -> Result<(), DesignExportError> {
        for entry in fs::read_dir(at)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(root, &path, out)?;
            } else if path.file_name().and_then(|value| value.to_str()) != Some(MANIFEST_NAME) {
                let bytes = fs::read(&path)?;
                out.push(FileRecord {
                    path: path
                        .strip_prefix(root)
                        .expect("descendant")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                });
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(folder, folder, &mut out)?;
    Ok(out)
}

pub(crate) fn rebuild_index(base: &Path) -> Result<PathBuf, DesignExportError> {
    fn scan(
        base: &Path,
        at: &Path,
        out: &mut Vec<DesignIndexEntry>,
    ) -> Result<(), DesignExportError> {
        for entry in fs::read_dir(at)? {
            let path = entry?.path();
            if !path.is_dir()
                || path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            let manifest = path.join(MANIFEST_NAME);
            if let Ok(bytes) = fs::read(&manifest)
                && let Ok(fields) = serde_json::from_slice::<ManifestIndexFields>(&bytes)
            {
                out.push(DesignIndexEntry {
                    design_name: fields.design_name,
                    design_id: fields.design_id,
                    export_kind: fields.export_kind,
                    folder: path
                        .strip_prefix(base)
                        .expect("descendant")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    created_utc: fields.created_utc,
                    mode: fields.mode,
                    mimo: fields.stages.mimo,
                });
            }
            scan(base, &path, out)?;
        }
        Ok(())
    }
    fs::create_dir_all(base)?;
    let mut entries = Vec::new();
    scan(base, base, &mut entries)?;
    entries.sort_by(|left, right| {
        right
            .created_utc
            .cmp(&left.created_utc)
            .then_with(|| left.folder.cmp(&right.folder))
    });
    let mut bytes = serde_json::to_vec_pretty(&entries)?;
    bytes.push(b'\n');
    let path = base.join(INDEX_NAME);
    secs_types::fs::write_atomic_bytes(&path, &bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_types::chain::MainChannel;

    fn chain() -> SignalChain {
        SignalChain {
            input_hp_hz: None,
            sample_rate: 48_000,
            mains: vec![MainChannel {
                role: "L".into(),
                input_peq: vec![(80.0, 1.0, -2.0)],
                fir: vec![1.0, 0.25],
                gain_db: 0.0,
                delay_ms: 1.0,
                xo_hz: None,
                top_peq: Vec::new(),
                align_delay_ms: 0.0,
            }],
            subs: Vec::new(),
            sub_bus_peq: Vec::new(),
            main_extra_ms: 0.0,
            lift_ms: 0.0,
            latency_ms: 1.0,
        }
    }

    fn metadata() -> DesignMetadata {
        DesignMetadata {
            design_name: "Cinema".into(),
            project_name: Some("Room".into()),
            app_version: "test-version".into(),
            created_utc: "2024-01-02T03:04:05.000Z".into(),
            export_kind: ExportKind::EqualizerApo,
            mode: "mixed".into(),
            latency_ms: 1.0,
            output_rate: 48_000,
            output_rate_auto: false,
            chain: chain(),
            mimo: None,
            bypass_config: None,
        }
    }

    #[test]
    fn named_design_manifest_replaces_bundle_hashes_files_and_rebuilds_index() {
        let base = std::env::temp_dir().join(format!(
            "secs-design-manifest-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        let destination = named_folder(&base, ExportKind::EqualizerApo, "Cinema");
        let first = staging_folder(&destination);
        fs::create_dir_all(first.join("nested")).unwrap();
        fs::write(first.join("nested/a.txt"), b"first").unwrap();
        let (_, first_id) = write_manifest(&first, &metadata()).unwrap();
        replace_folder(&first, &destination).unwrap();
        fs::write(base.join(INDEX_NAME), b"corrupt").unwrap();
        rebuild_index(&base).unwrap();

        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join(MANIFEST_NAME)).unwrap()).unwrap();
        assert_eq!(manifest["design_id"], first_id);
        assert_eq!(manifest["files"].as_array().unwrap().len(), 1);
        assert_eq!(manifest["files"][0]["path"], "nested/a.txt");
        assert_eq!(manifest["files"][0]["bytes"], 5);
        assert_eq!(
            manifest["files"][0]["sha256"],
            "a7937b64b8caa58f03721bb6bacf5c78cb235febe0e70b1b84cd99541461a08e"
        );

        let second = staging_folder(&destination);
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("new.txt"), b"second").unwrap();
        let (_, second_id) = write_manifest(&second, &metadata()).unwrap();
        assert_eq!(first_id, second_id);
        replace_folder(&second, &destination).unwrap();
        rebuild_index(&base).unwrap();
        assert!(!destination.join("nested/a.txt").exists());
        let index: Vec<DesignIndexEntry> =
            serde_json::from_slice(&fs::read(base.join(INDEX_NAME)).unwrap()).unwrap();
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].design_name, "Cinema");
        assert_eq!(index[0].design_id, second_id);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn invalid_design_names_are_rejected_before_a_folder_is_chosen() {
        for (name, reason) in [
            (" ", "design_name_length"),
            ("bad/name", "design_name_character"),
            ("line\nfeed", "design_name_control"),
            ("trailing.", "design_name_trailing"),
            ("CON.txt", "design_name_reserved"),
        ] {
            assert_eq!(validate_design_name(Some(name)).unwrap_err(), reason);
        }
        assert_eq!(
            validate_design_name(Some("  Cinema  ")).unwrap(),
            Some("Cinema".into())
        );
    }
}

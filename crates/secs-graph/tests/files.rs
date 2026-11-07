#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Bundle installation against the file system, at the crate's interface.
//! The link case lives here rather than in the unit tests: creating a
//! junction on a machine without the symlink privilege spawns `mklink`, and
//! the subprocess policy (`secs-policy` gates) scans only `src/`.

use secs_graph::files::write_bundle_observed;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "secs-graph-it-{name}-{}-{serial}",
        std::process::id()
    ))
}

fn leftovers(parent: &Path, needle: &str) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .contains(needle)
        })
        .collect()
}

#[cfg(windows)]
fn link_directory(target: &Path, link: &Path) {
    if let Err(symlink_error) = std::os::windows::fs::symlink_dir(target, link) {
        // Without the symlink privilege a junction stands in; it is a
        // reparse point too, which is what the writer must refuse.
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("run mklink /J after symlink_dir failed");
        assert!(
            output.status.success(),
            "symlink_dir failed ({symlink_error}); mklink /J failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(unix)]
fn link_directory(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// TD-270: a bundle whose parent folder is a link is refused before any file
/// is written, naming the link.
#[test]
fn symlinked_bundle_parent_is_refused_td270() {
    let parent = temp_dir("symlink-parent");
    let root = parent.join("bundle");
    let outside = parent.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let linked = root.join("coeffs");
    link_directory(&outside, &linked);
    let files = vec![
        (PathBuf::from("coeffs/L.wav"), b"coefficient".to_vec()),
        (PathBuf::from("config.txt"), b"config".to_vec()),
    ];

    let error = write_bundle_observed(
        &root,
        &files,
        Path::new("config.txt"),
        &|_, _| Ok(()),
        &|_, _| Ok(()),
    )
    .unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains(&linked.display().to_string()));
    assert!(!outside.join("L.wav").exists());
    assert!(leftovers(&parent, ".stage-").is_empty());
    fs::remove_dir(&linked).unwrap();
    fs::remove_dir_all(parent).unwrap();
}

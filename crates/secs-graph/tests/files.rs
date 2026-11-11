#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Bundle installation against the file system, at the crate's interface.
//! The link case lives here rather than in the unit tests: creating a
//! junction on a machine without the symlink privilege spawns `mklink`, and
//! the subprocess policy (`secs-policy` gates) scans only `src/`.

use secs_graph::files::{write_bundle_observed, write_bundle_reported};
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

/// A Windows directory symlink or junction goes with `remove_dir`; a Unix symlink
/// to a directory is a file entry and goes with `remove_file`.
fn unlink_directory(link: &Path) {
    if cfg!(windows) {
        fs::remove_dir(link).unwrap();
    } else {
        fs::remove_file(link).unwrap();
    }
}

#[test]
fn reported_bundle_counts_synced_bytes_after_each_file() {
    let parent = temp_dir("reported-bundle");
    let root = parent.join("bundle");
    let files = vec![
        (PathBuf::from("a.bin"), vec![1; 2]),
        (PathBuf::from("nested/b.bin"), vec![2; 5]),
        (PathBuf::from("config.txt"), vec![3; 11]),
    ];
    let reports = std::cell::RefCell::new(Vec::new());

    write_bundle_reported(&root, &files, Path::new("config.txt"), &|done, total| {
        reports.borrow_mut().push((done, total));
    })
    .unwrap();

    assert_eq!(reports.into_inner(), [(0, 18), (2, 18), (7, 18), (18, 18)]);
    for (relative, bytes) in files {
        assert_eq!(fs::read(root.join(relative)).unwrap(), bytes);
    }
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn reported_bundle_with_only_empty_files_reports_nothing() {
    let parent = temp_dir("reported-empty-bundle");
    let root = parent.join("bundle");
    let files = vec![
        (PathBuf::from("empty.bin"), Vec::new()),
        (PathBuf::from("config.txt"), Vec::new()),
    ];
    let reports = std::cell::RefCell::new(Vec::new());

    write_bundle_reported(&root, &files, Path::new("config.txt"), &|done, total| {
        reports.borrow_mut().push((done, total));
    })
    .unwrap();

    assert!(reports.into_inner().is_empty());
    assert_eq!(fs::read(root.join("empty.bin")).unwrap(), b"");
    assert_eq!(fs::read(root.join("config.txt")).unwrap(), b"");
    fs::remove_dir_all(parent).unwrap();
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
    unlink_directory(&linked);
    fs::remove_dir_all(parent).unwrap();
}

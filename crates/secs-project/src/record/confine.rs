use super::*;

pub(super) fn confined_ir_path(project: &Path, candidate: &Path) -> Result<(), ()> {
    if candidate
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(());
    }
    let root = ir_directory(project);
    let root_canonical = canonicalize_existing_ancestor(&root)?;
    let candidate_canonical = canonicalize_existing_ancestor(candidate)?;
    if candidate_canonical.starts_with(root_canonical) {
        Ok(())
    } else {
        Err(())
    }
}

pub(super) fn canonicalize_existing_ancestor(path: &Path) -> Result<PathBuf, ()> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().ok_or(())?;
        missing.push(name.to_owned());
        existing = existing.parent().ok_or(())?;
    }
    let mut canonical = existing.canonicalize().map_err(|_| ())?;
    for name in missing.into_iter().rev() {
        canonical.push(name);
    }
    Ok(canonical)
}

pub(super) fn relative_to_project(project: &Path, path: &Path) -> String {
    path.strip_prefix(project.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const PUBLISHED: [&str; 15] = [
    "secs-types",
    "secs-dsp",
    "secs-compute",
    "secs-engine",
    "secs-jos",
    "secs-mdat",
    "secs-project",
    "secs-dirac",
    "secs-export",
    "secs-graph",
    "secs-camilla",
    "secs-filters",
    "secs-rew",
    "secs-audio",
    "secs-service",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn manifest(path: &Path) -> toml::Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    toml::from_str(&text)
        .unwrap_or_else(|error| panic!("{}: failed to parse TOML: {error}", path.display()))
}

fn packages() -> BTreeMap<String, (PathBuf, toml::Value)> {
    let mut packages = BTreeMap::new();
    for parent in ["crates", "apps"] {
        for entry in fs::read_dir(root().join(parent)).expect("workspace package directory") {
            let dir = entry.expect("workspace package entry").path();
            let path = dir.join("Cargo.toml");
            if !path.is_file() {
                continue;
            }
            let value = manifest(&path);
            let name = value["package"]["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: package.name", path.display()))
                .to_owned();
            assert!(packages.insert(name, (dir, value)).is_none());
        }
    }
    packages
}

fn publish_allowed(value: &toml::Value) -> bool {
    value["package"].get("publish").is_none()
}

#[test]
fn exactly_the_internal_library_set_allows_publishing() {
    let actual: BTreeSet<_> = packages()
        .into_iter()
        .filter(|(_, (_, value))| publish_allowed(value))
        .map(|(name, _)| name)
        .collect();
    let expected = PUBLISHED.into_iter().map(str::to_owned).collect();
    assert_eq!(actual, expected);
}

fn dependency_package(alias: &str, specification: &toml::Value, workspace: &toml::Table) -> String {
    let table = specification.as_table();
    if let Some(package) = table
        .and_then(|table| table.get("package"))
        .and_then(toml::Value::as_str)
    {
        return package.to_owned();
    }
    if table
        .and_then(|table| table.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true)
    {
        return workspace
            .get(alias)
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("package"))
            .and_then(toml::Value::as_str)
            .unwrap_or(alias)
            .to_owned();
    }
    alias.to_owned()
}

fn dependency_version<'a>(
    alias: &str,
    specification: &'a toml::Value,
    workspace: &'a toml::Table,
) -> Option<&'a str> {
    let table = specification.as_table()?;
    if table.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
        return workspace.get(alias)?.as_table()?.get("version")?.as_str();
    }
    table.get("version")?.as_str()
}

fn dependency_tables(value: &toml::Value) -> Vec<(&str, &toml::Table)> {
    let mut tables = Vec::new();
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        if let Some(table) = value.get(section).and_then(toml::Value::as_table) {
            tables.push((section, table));
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
                if let Some(table) = target.get(section).and_then(toml::Value::as_table) {
                    tables.push((section, table));
                }
            }
        }
    }
    tables
}

#[test]
fn published_versions_and_dependencies_are_closed() {
    let packages = packages();
    let published: BTreeSet<_> = PUBLISHED.into_iter().collect();
    let workspace_manifest = manifest(&root().join("Cargo.toml"));
    let workspace = workspace_manifest["workspace"]["dependencies"]
        .as_table()
        .expect("workspace dependencies");
    let mut version = None;
    let mut violations = Vec::new();

    for name in PUBLISHED {
        let (_, value) = &packages[name];
        let literal = value["package"]["version"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: package.version must be literal"));
        if let Some(expected) = version {
            if literal != expected {
                violations.push(format!("{name}: version {literal}, expected {expected}"));
            }
        } else {
            version = Some(literal);
        }
        let exact = format!("={literal}");
        for (section, dependencies) in dependency_tables(value) {
            for (alias, specification) in dependencies {
                let package = dependency_package(alias, specification, workspace);
                if published.contains(package.as_str()) {
                    if dependency_version(alias, specification, workspace) != Some(exact.as_str()) {
                        violations.push(format!(
                            "{name} [{section}] {alias}: published dependency {package} must use {exact}"
                        ));
                    }
                } else if section != "dev-dependencies" && packages.contains_key(&package) {
                    violations.push(format!(
                        "{name} [{section}] {alias}: unpublished workspace dependency {package}"
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn published_packages_carry_required_metadata_and_files() {
    let root = root();
    let root_license = fs::read(root.join("LICENSE")).expect("root LICENSE");
    let packages = packages();
    let mut violations = Vec::new();

    for name in PUBLISHED {
        let (dir, value) = &packages[name];
        let package = value["package"].as_table().expect("package table");
        for key in ["description", "readme", "keywords", "categories", "include"] {
            if !package.contains_key(key) {
                violations.push(format!("{name}: missing package.{key}"));
            }
        }
        if package.get("readme").and_then(toml::Value::as_str) != Some("README.md") {
            violations.push(format!("{name}: readme must be README.md"));
        }
        if !dir.join("README.md").is_file() {
            violations.push(format!("{name}: README.md is missing"));
        }
        match fs::read(dir.join("LICENSE")) {
            Ok(license) if license == root_license => {}
            Ok(_) => violations.push(format!("{name}: LICENSE differs from root LICENSE")),
            Err(error) => violations.push(format!("{name}: LICENSE: {error}")),
        }
        let includes = package
            .get("include")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        for required in ["Cargo.toml", "README.md", "LICENSE", "src/**"] {
            if !includes.iter().any(|item| item.as_str() == Some(required)) {
                violations.push(format!("{name}: include is missing {required}"));
            }
        }
        for item in includes.iter().filter_map(toml::Value::as_str) {
            let lower = item.to_ascii_lowercase();
            if ["tests", "examples", "benches"]
                .iter()
                .any(|forbidden| lower.split('/').any(|part| part == *forbidden))
            {
                violations.push(format!("{name}: forbidden package include {item}"));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn packaged_upstream_copies_are_exact() {
    let root = root();
    for (copy, source) in [
        ("crates/secs-service/UPSTREAM.toml", "UPSTREAM.toml"),
        (
            "crates/secs-mdat/data/template_1.mdat",
            "src/secs/rew_mdat/template_1.mdat",
        ),
    ] {
        assert_eq!(
            fs::read(root.join(copy)).unwrap_or_else(|error| panic!("{copy}: {error}")),
            fs::read(root.join(source)).unwrap_or_else(|error| panic!("{source}: {error}")),
            "{copy} differs from {source}"
        );
    }
}

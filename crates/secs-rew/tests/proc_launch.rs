#![forbid(unsafe_code)]
use secs_rew::{RewError, launch::*};
use std::{collections::BTreeMap, ffi::OsString, path::Path};
#[test]
fn test_clean_env_drops_the_bundle_from_path() {
    let root = std::env::current_dir().unwrap();
    let bundle = root.join("SECS/_internal");
    let other = root.join("Other");
    let env = BTreeMap::from([
        (
            OsString::from("PATH"),
            std::env::join_paths([bundle.clone(), bundle.join("PyQt6"), other.clone()]).unwrap(),
        ),
        (
            "QT_PLUGIN_PATH".into(),
            bundle.join("plugins").into_os_string(),
        ),
    ]);
    let clean = clean_env(&env, Some(&bundle));
    assert_eq!(
        std::env::split_paths(&clean[&OsString::from("PATH")]).collect::<Vec<_>>(),
        [other]
    );
    assert!(!clean.contains_key(&OsString::from("QT_PLUGIN_PATH")));
}
#[test]
fn test_clean_env_is_untouched_outside_a_bundle() {
    let env = BTreeMap::from([
        ("PATH".into(), "a:b".into()),
        ("PYTHONHOME".into(), "python".into()),
    ]);
    assert_eq!(clean_env(&env, None), env);
}
#[test]
fn test_launch_external_runs_in_the_programs_own_folder() {
    let exe = std::env::current_dir()
        .unwrap()
        .join("REW/roomeqwizard.exe");
    let c = launch_command(&exe, Platform::Windows);
    assert_eq!(c.program, exe);
    assert_eq!(c.args, [OsString::from("-api")]);
    assert_eq!(c.cwd, exe.parent().map(Path::to_path_buf));
    let mac = launch_command(Path::new(MAC_PATH), Platform::Mac);
    assert_eq!(mac.program, Path::new("open"));
    assert_eq!(
        mac.args,
        vec![
            OsString::from("-a"),
            MAC_PATH.into(),
            "--args".into(),
            "-api".into()
        ]
    );
}
#[test]
fn test_find_rew_exe_searches_standard_install_locations() {
    assert_eq!(
        find_rew_exe_for(Platform::Windows, |p| p == Path::new(WINDOWS_PATHS[2])).unwrap(),
        Path::new(WINDOWS_PATHS[2])
    );
    assert_eq!(
        find_rew_exe_for(Platform::Mac, |_| true).unwrap(),
        Path::new(MAC_PATH)
    );
    assert_eq!(find_rew_exe_for(Platform::Other, |_| true), None);
    assert_eq!(find_rew_exe_for(Platform::Windows, |_| false), None);
}
#[derive(Debug)]
struct Probe(Result<Vec<String>, RewError>);
impl ProcessProbe for Probe {
    fn process_names(&self) -> Result<Vec<String>, RewError> {
        self.0.clone()
    }
}
#[test]
fn test_rew_process_running_uses_injected_probe() {
    assert!(rew_process_running(&Probe(Ok(vec![
        "ROOMeqWizard.exe".into()
    ]))));
    assert!(!rew_process_running(&Probe(Ok(vec!["not-rew.exe".into()]))));
    assert!(!rew_process_running(&Probe(Err(RewError::Connection {
        path: "processes".into(),
        message: "unavailable".into()
    }))));
}

//! Filesystem discovery and child launch only. OS process lists belong to secs-sys.
use crate::RewError;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const WINDOWS_PATHS: [&str; 4] = [
    r"C:\Program Files\REW\roomeqwizard.exe",
    r"C:\Program Files (x86)\REW\roomeqwizard.exe",
    r"C:\Program Files\REW\REW.exe",
    r"C:\Program Files (x86)\REW\REW.exe",
];
pub const MAC_PATH: &str = "/Applications/REW/REW.app";
pub const REW_PROCESS_NAMES: [&str; 2] = ["roomeqwizard.exe", "rew.exe"];
/// The application supplies secs-sys's process enumerator. Implementations must
/// treat unavailable enumeration as unknown, not attempt shell commands here.
pub trait ProcessProbe {
    fn process_names(&self) -> Result<Vec<String>, RewError>;
}
pub fn rew_process_running(probe: &impl ProcessProbe) -> bool {
    probe
        .process_names()
        .unwrap_or_default()
        .iter()
        .any(|name| {
            REW_PROCESS_NAMES
                .iter()
                .any(|rew| name.eq_ignore_ascii_case(rew))
        })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
    Other,
}
impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}
pub fn find_rew_exe() -> Option<PathBuf> {
    find_rew_exe_for(Platform::current(), |p| {
        if Platform::current() == Platform::Mac {
            p.exists()
        } else {
            p.is_file()
        }
    })
}
/// Filesystem predicate permits testing standard paths on a different host OS.
pub fn find_rew_exe_for(platform: Platform, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let candidates: &[&str] = match platform {
        Platform::Windows => &WINDOWS_PATHS,
        Platform::Mac => &[MAC_PATH],
        Platform::Other => &[],
    };
    candidates.iter().map(PathBuf::from).find(|p| exists(p))
}
/// Rust is not a PyInstaller bundle and never changes SetDllDirectory. This
/// pure helper supports hosts which deliberately supply a bundled environment.
pub fn clean_env(
    env: &BTreeMap<OsString, OsString>,
    bundle: Option<&Path>,
) -> BTreeMap<OsString, OsString> {
    let mut env = env.clone();
    let Some(bundle) = bundle else {
        return env;
    };
    if let Some(path) = env.get(&OsString::from("PATH")) {
        let paths = std::env::split_paths(path)
            .filter(|p| !p.starts_with(bundle))
            .collect::<Vec<_>>();
        if let Ok(path) = std::env::join_paths(paths) {
            env.insert("PATH".into(), path);
        }
    }
    for key in [
        "PYTHONHOME",
        "PYTHONPATH",
        "QT_PLUGIN_PATH",
        "QML2_IMPORT_PATH",
        "TCL_LIBRARY",
        "TK_LIBRARY",
    ] {
        env.remove(&OsString::from(key));
    }
    env
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
}
pub fn launch_command(exe: &Path, platform: Platform) -> LaunchCommand {
    if platform == Platform::Mac {
        LaunchCommand {
            program: "open".into(),
            args: vec![
                "-a".into(),
                exe.as_os_str().into(),
                "--args".into(),
                "-api".into(),
            ],
            cwd: None,
        }
    } else {
        LaunchCommand {
            program: exe.into(),
            args: vec!["-api".into()],
            cwd: exe
                .is_absolute()
                .then(|| exe.parent().map(Path::to_path_buf))
                .flatten(),
        }
    }
}
pub fn launch_external(exe: &Path, platform: Platform) -> Result<Child, RewError> {
    let spec = launch_command(exe, platform);
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd) = spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map_err(|e| RewError::connection("launch", e))
}
pub fn launch() -> Result<(), RewError> {
    let exe = find_rew_exe()
        .ok_or_else(|| RewError::connection("launch", "REW installation not found"))?;
    let mut child = launch_external(&exe, Platform::current())?;
    // Reap the launcher without holding up the service or killing REW on Drop.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#![forbid(unsafe_code)]

#[path = "support/case_project.rs"]
mod support;

fn main() {
    let mut args = std::env::args_os().skip(1).peekable();
    // `--mimo-on` stores the case's settings with MIMO enabled, so the CLI's
    // `run --mimo` computes MIMO with the case's own settings otherwise.
    let mimo_on = args.next_if(|value| value == "--mimo-on").is_some();
    let name = args
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "room_dba".into());
    let path = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    if args.next().is_some() {
        eprintln!("usage: case_project [--mimo-on] [CASE] [PATH.secs]");
        std::process::exit(2);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let (path, _) = if mimo_on {
        support::write_with(&path, &name, |settings| settings.mimo_on = true)
    } else {
        support::write(&path, &name)
    };
    println!("{}", path.display());
}

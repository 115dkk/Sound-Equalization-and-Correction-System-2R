#![no_main]

use libfuzzer_sys::fuzz_target;
use secs_project::Project;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(project) = Project::load_text(text) {
        let saved = project.save_text();
        let reloaded = Project::load_text(&saved).expect("saved project must load");
        assert_eq!(reloaded.save_text(), saved);
    }
});

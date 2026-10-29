use super::{ConvolutionRoute, ConvolutionRouting, convolution_reaper, lua_string};
use std::fs;
use std::path::PathBuf;

fn example(name: Option<&str>) -> String {
    let inputs = vec!["L".to_owned(), "R".to_owned()];
    let outputs = vec!["R".to_owned(), "L".to_owned(), "SW1".to_owned()];
    let paths = [("R", "SW1"), ("L", "R")]
        .into_iter()
        .map(|(input, output)| ConvolutionRoute {
            input: input.to_owned(),
            output: output.to_owned(),
            wav: format!("{input}_to_{output}.wav"),
            csv: format!("{input}_to_{output}.csv"),
            peak_gain_db: 0.0,
            delay_ms: 0.0,
        })
        .collect();
    convolution_reaper(
        &ConvolutionRouting {
            schema: 1,
            sample_rate: 48_000,
            length: 4096,
            format: "f32",
            headroom_db: 0.0,
            inputs: &inputs,
            outputs: &outputs,
            paths,
        },
        name,
    )
}

#[test]
fn reaper_script_uses_routing_order_and_quotes_names() {
    let name = "거실 \"보정\"\\새 폴더\n다음\r\t\0\u{7f}9 ]=]";
    let script = example(Some(name));
    assert!(script.starts_with(
        "local design_name = \"거실 \\\"보정\\\"\\\\새 폴더\\010다음\\013\\009\\000\\1279 ]=]\"\n"
    ));
    assert!(script.contains("local input_channels = 2\nlocal output_channels = 4\n"));
    assert!(script.contains("{input = 2, output = 3, wav = \"R_to_SW1.wav\", name = \"R → SW1\"}"));
    assert!(script.contains("{input = 1, output = 1, wav = \"L_to_R.wav\", name = \"L → R\"}"));
    for file in ["R_to_SW1.wav", "L_to_R.wav"] {
        assert_eq!(script.matches(file).count(), 1);
    }
    assert_eq!(script, example(Some(name)));
    assert!(example(None).starts_with("local design_name = \"보정\"\n"));
    assert_eq!(example(None), example(Some(" \t")));
    assert_eq!(lua_string("\u{85}"), "\"\\194\\133\"");
    // Optional, explicit fixture output for the real-REAPER harness. Never reads
    // or writes REAPER's installation, resource folder or user configuration.
    if let Some(folder) = std::env::var_os("SECS_REAPER_EXAMPLE") {
        let folder = PathBuf::from(folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("secs2r_reaper.lua"), example(Some("거실"))).unwrap();
        let mut impulse = vec![0.0; 4096];
        impulse[0] = 0.5;
        let bytes = super::wav_bytes(&impulse, 48_000).unwrap();
        for file in ["R_to_SW1.wav", "L_to_R.wav"] {
            fs::write(folder.join(file), &bytes).unwrap();
        }
    }
}

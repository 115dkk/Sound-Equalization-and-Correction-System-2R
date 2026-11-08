#![no_main]

use libfuzzer_sys::fuzz_target;
use secs_mdat::CalFile;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = secs_audio::MicCalibration::parse_checked(text);

    let (devices, calibration) = text.split_once('\n').unwrap_or((text, ""));
    let mut fields = devices.split('|');
    let output_device = fields.next().unwrap_or("");
    let input_device = fields.next().unwrap_or("");
    let output_channel = fields.next().unwrap_or("");
    let path = fields.next().unwrap_or("fuzz.cal");
    let cal = CalFile {
        path: path.to_owned(),
        text: Some(calibration.as_bytes().to_vec()),
    };
    let _ =
        secs_mdat::from_rew_measurement(Some(&cal), output_device, input_device, output_channel, 0);
});

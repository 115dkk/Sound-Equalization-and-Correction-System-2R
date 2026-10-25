#![no_main]

// The IR WAV readers of secs-project (audit #92 TD-60): any input either fails
// cleanly or yields samples that survive a float32 write and read unchanged.
use libfuzzer_sys::fuzz_target;
use secs_project::{read_mono_float_wav, read_wav_float32, try_write_wav_float32};

fuzz_target!(|data: &[u8]| {
    let _ = read_wav_float32(data);
    if let Ok((samples, rate)) = read_mono_float_wav(data)
        && let Ok(bytes) = try_write_wav_float32(&samples, rate)
    {
        let (again, again_rate) = read_wav_float32(&bytes).expect("written WAV must read");
        assert_eq!(again_rate, rate);
        assert!(
            again.iter().map(|v| v.to_bits()).eq(samples.iter().map(|v| v.to_bits())),
            "samples changed in a float32 round trip"
        );
    }
});

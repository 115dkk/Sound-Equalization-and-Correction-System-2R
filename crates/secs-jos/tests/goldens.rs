#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
use secs_jos::{JosError, Limits, decode_modified_utf8, encode_modified_utf8, parse, write};
use secs_oracle::Fixture;

#[test]
#[cfg_attr(miri, ignore)]
fn template_and_build_roundtrip() {
    let template = Fixture::load("mdat", "template");
    let bytes = template.blob(&template.inputs()["bytes"]);
    let s = parse(&bytes, &Limits::default()).unwrap();
    assert_eq!(write(&s), bytes);
    for f in Fixture::all("mdat", "build_") {
        let bytes = f.blob(&f.outputs()["bytes"]);
        let s = parse(&bytes, &Limits::default()).unwrap();
        assert_eq!(write(&s), bytes, "{}", f.id);
    }
}
#[test]
#[cfg_attr(miri, ignore)]
fn errors_match_oracle() {
    for f in Fixture::all("mdat", "error_") {
        let mut limits = Limits::default();
        if let Some(overrides) = f.inputs()["limits"].as_object() {
            for (key, value) in overrides {
                let v = value.as_u64().unwrap() as usize;
                match key.as_str() {
                    "max_file_bytes" => limits.max_file_bytes = v,
                    "max_array_bytes" => limits.max_array_bytes = v,
                    "max_array_elements" => limits.max_array_elements = v,
                    "max_string_bytes" => limits.max_string_bytes = v,
                    "max_depth" => limits.max_depth = v,
                    "max_handles" => limits.max_handles = v,
                    "max_fields_per_class" => limits.max_fields_per_class = v,
                    "max_top_level_values" => limits.max_top_level_values = v,
                    _ => panic!("unknown limit"),
                }
            }
        }
        let bytes = f.blob(&f.inputs()["bytes"]);
        let e = parse(&bytes, &limits).unwrap_err();
        let kind = match e {
            JosError::Format { .. } => "StreamFormatError",
            JosError::Unsupported { .. } => "UnsupportedFeatureError",
            JosError::InvalidHandle { .. } => "InvalidHandleError",
            JosError::LimitExceeded { .. } => "LimitExceededError",
        };
        assert_eq!(kind, f.outputs()["kind"].as_str().unwrap(), "{}", f.id);
        assert_eq!(e.offset(), f.outputs()["offset"].as_u64(), "{}", f.id);
        assert_eq!(
            e.to_string(),
            f.outputs()["message"].as_str().unwrap(),
            "{}",
            f.id
        );
    }
}
#[test]
#[cfg_attr(miri, ignore)]
fn modified_utf8_matches_oracle() {
    for f in Fixture::all("mdat", "utf8_") {
        if let Some(text) = f.inputs()["text"].as_str() {
            let bytes = f.blob(&f.outputs()["bytes"]);
            assert_eq!(encode_modified_utf8(text), bytes);
            assert_eq!(decode_modified_utf8(&bytes).unwrap(), text);
        } else {
            let bytes = f.blob(&f.inputs()["bytes"]);
            let e = decode_modified_utf8(&bytes).unwrap_err();
            assert_eq!(e.to_string(), f.outputs()["message"].as_str().unwrap());
        }
    }
}

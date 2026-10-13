//! Golden fixtures of the upstream oracle, for parity tests (docs/adr/0004).
//!
//! `oracle/export_<family>.py` runs SECS 2 on fixed inputs and writes
//! `oracle/goldens/<family>/<case>.json` (`{"inputs", "outputs", "meta"}`)
//! with float64 sidecars and byte blobs beside it; `oracle/README.md` is the
//! format. This crate reads them back:
//!
//! ```no_run
//! let fx = secs_oracle::Fixture::load("dsp", "peaking_0");
//! let expected = fx.f64s(&fx.outputs()["response_db"]);
//! # let got = expected.clone();
//! secs_oracle::assert_close("peaking_0", &got, &expected, 0.0, secs_oracle::rtol(0.0));
//! ```
//!
//! It is a dev-dependency only. Whether the oracle's exact budgets hold depends
//! on the platform's C math library ([`oracle_libm`]): the oracle ran on glibc,
//! which is also what Rust's `f64` functions call on Linux.
//!
//! The loader and the libm probe follow Impulcifer-py313's
//! `crates/impulcifer-dsp/tests/golden_secs2_helpers.rs` (MIT, Copyright (c)
//! 2024- 115dkk); see docs/THIRD_PARTY.md.
#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `oracle/goldens` of this workspace.
pub fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/goldens")
}

/// One fixture file and the family folder its sidecars live in.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// `<family>/<case>`, for messages.
    pub id: String,
    dir: PathBuf,
    value: Value,
}

impl Fixture {
    /// `oracle/goldens/<family>/<case>.json`. Panics with the regeneration
    /// command when it is missing.
    pub fn load(family: &str, case: &str) -> Fixture {
        let dir = goldens().join(family);
        let path = dir.join(format!("{case}.json"));
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}; regenerate with `<oracle venv>/bin/python oracle/run.py {family}`",
                path.display()
            )
        });
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        Fixture {
            id: format!("{family}/{case}"),
            dir,
            value,
        }
    }

    /// Every fixture of `family` whose case name starts with `prefix`, sorted
    /// by name. Panics when there is none (a family a test expects must exist).
    pub fn all(family: &str, prefix: &str) -> Vec<Fixture> {
        let dir = goldens().join(family);
        let mut cases: Vec<String> = fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
            .map(|entry| entry.expect("directory entry").file_name())
            .filter_map(|name| name.into_string().ok())
            .filter_map(|name| name.strip_suffix(".json").map(str::to_owned))
            .filter(|case| case.starts_with(prefix))
            .collect();
        cases.sort();
        assert!(!cases.is_empty(), "no fixture {family}/{prefix}*");
        cases
            .iter()
            .map(|case| Fixture::load(family, case))
            .collect()
    }

    /// The calls a bundled fixture holds, each as a fixture of its own.
    ///
    /// A fixture that records many calls of one function keeps them in one
    /// file as `{"inputs": {"calls": [..]}, "outputs": {"calls": [..]}}`
    /// (oracle/README.md); call `n` reads as a fixture with id `<id>#n`
    /// whose sidecars resolve in the same family. Any other fixture is its
    /// own single call.
    pub fn calls(&self) -> Vec<Fixture> {
        let (Some(inputs), Some(outputs)) = (
            self.inputs()["calls"].as_array(),
            self.outputs()["calls"].as_array(),
        ) else {
            return vec![self.clone()];
        };
        assert_eq!(inputs.len(), outputs.len(), "{}: calls", self.id);
        inputs
            .iter()
            .zip(outputs)
            .enumerate()
            .map(|(n, (i, o))| Fixture {
                id: format!("{}#{n}", self.id),
                dir: self.dir.clone(),
                value: serde_json::json!({"inputs": i, "outputs": o, "meta": self.meta()}),
            })
            .collect()
    }

    /// [`Fixture::all`], with bundled fixtures expanded into their calls.
    pub fn all_calls(family: &str, prefix: &str) -> Vec<Fixture> {
        Fixture::all(family, prefix)
            .iter()
            .flat_map(Fixture::calls)
            .collect()
    }

    /// The whole fixture.
    pub fn value(&self) -> &Value {
        &self.value
    }
    /// `inputs`.
    pub fn inputs(&self) -> &Value {
        &self.value["inputs"]
    }
    /// `outputs`.
    pub fn outputs(&self) -> &Value {
        &self.value["outputs"]
    }
    /// `meta` (oracle, versions).
    pub fn meta(&self) -> &Value {
        &self.value["meta"]
    }

    /// A float64 array: inline (numbers or `"nan"`/`"+inf"`/`"-inf"`), one
    /// sidecar descriptor, or a list of descriptors in order. Sidecar length
    /// and SHA-256 are checked.
    pub fn f64s(&self, v: &Value) -> Vec<f64> {
        if v.get("file").is_some() {
            return self.sidecar(v);
        }
        let items = v
            .as_array()
            .unwrap_or_else(|| panic!("{}: not a float64 array: {v}", self.id));
        if items
            .first()
            .is_some_and(|first| first.get("file").is_some())
        {
            return items.iter().flat_map(|part| self.sidecar(part)).collect();
        }
        items.iter().map(number).collect()
    }

    /// A complex array: `{"re", "im"}` arrays, or inline `[[re, im], …]`.
    pub fn c128s(&self, v: &Value) -> Vec<(f64, f64)> {
        if let (Some(re), Some(im)) = (v.get("re"), v.get("im")) {
            let (re, im) = (self.f64s(re), self.f64s(im));
            assert_eq!(re.len(), im.len(), "{}: re/im lengths", self.id);
            return re.into_iter().zip(im).collect();
        }
        v.as_array()
            .unwrap_or_else(|| panic!("{}: not a complex array: {v}", self.id))
            .iter()
            .map(|pair| (number(&pair[0]), number(&pair[1])))
            .collect()
    }

    /// Whether `v` is a digest descriptor (`{"digest", "length", "first",
    /// "last"}`): an array frozen by its SHA-256 instead of a sidecar.
    pub fn is_digest(v: &Value) -> bool {
        v.get("digest").is_some() && v.get("length").is_some()
    }

    /// Asserts `got` has the digest descriptor's length, first and last values
    /// (bit for bit) and SHA-256 of its little-endian float64 bytes.
    pub fn assert_digest(&self, name: &str, v: &Value, got: &[f64]) {
        assert_eq!(
            got.len() as u64,
            integer(&v["length"]),
            "{}: {name} length",
            self.id
        );
        let first = self.f64s(&v["first"]);
        let last = self.f64s(&v["last"]);
        let bits = |x: &[f64]| x.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&got[..first.len()]),
            bits(&first),
            "{}: {name} first",
            self.id
        );
        assert_eq!(
            bits(&got[got.len() - last.len()..]),
            bits(&last),
            "{}: {name} last",
            self.id
        );
        let mut hasher = Sha256::new();
        for value in got {
            hasher.update(value.to_le_bytes());
        }
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            Some(hex.as_str()),
            v["digest"].as_str(),
            "{}: {name} sha256",
            self.id
        );
    }

    /// The bytes of a blob descriptor (`{"blob", "length", "sha256"}`), checked.
    pub fn blob(&self, v: &Value) -> Vec<u8> {
        let name = v["blob"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: not a blob: {v}", self.id));
        let bytes = self.read_beside(name);
        check_digest(&self.id, name, &bytes, v);
        bytes
    }

    fn sidecar(&self, v: &Value) -> Vec<f64> {
        let name = v["file"].as_str().expect("sidecar file name");
        let bytes = self.read_beside(name);
        check_digest(
            &self.id,
            name,
            &bytes,
            &serde_json::json!({
                "length": integer(&v["length"]) * 8,
                "sha256": v["sha256"],
            }),
        );
        let (chunks, rest) = bytes.as_chunks::<8>();
        assert!(rest.is_empty(), "{}: {name} is not whole float64s", self.id);
        chunks
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk))
            .collect()
    }

    fn read_beside(&self, name: &str) -> Vec<u8> {
        assert!(
            !name.contains(['/', '\\']) && !name.starts_with('.'),
            "{}: sidecar name {name:?} leaves the family folder",
            self.id
        );
        let path = self.dir.join(name);
        fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }
}

fn check_digest(id: &str, name: &str, bytes: &[u8], descriptor: &Value) {
    let length = integer(&descriptor["length"]);
    assert_eq!(bytes.len() as u64, length, "{id}: {name} length");
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        Some(hex.as_str()),
        descriptor["sha256"].as_str(),
        "{id}: {name} sha256"
    );
}

/// A float64 fixture value; nonfinite values are the strings
/// `"nan"`, `"+inf"`, `"-inf"`.
pub fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("nan") => f64::NAN,
        Some("+inf" | "inf") => f64::INFINITY,
        Some("-inf") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")),
    }
}

/// A non-negative integer fixture value.
pub fn integer(v: &Value) -> u64 {
    v.as_u64()
        .unwrap_or_else(|| panic!("not a non-negative integer: {v}"))
}

/// Relative budget for comparisons that are exact on the oracle's libm.
pub const PLATFORM_RTOL: f64 = 1e-9;

/// Whether Rust's `f64` math here is bit for bit the oracle's C library
/// (`oracle/goldens/libm/probe.json`). Exact gates hold only there; elsewhere
/// they widen to [`PLATFORM_RTOL`]. `SECS_ORACLE_FOREIGN_LIBM=1` forces the
/// foreign path so it can be exercised on the oracle's own libm.
/// sin/cos over the phases the engine evaluates on its design grid
/// (`secs-engine` `measurement::ramp_phase`): glibc versions that agree on the
/// probe's fixed points can still differ in the last bit on some of these.
fn design_phases_match(probe: &Fixture) -> bool {
    let n = 1usize << 17;
    let step = 1.0 / (n as f64 * (1.0 / 48000.0));
    probe.outputs()["design_phases"]
        .as_array()
        .unwrap_or_else(|| panic!("libm probe has no design_phases"))
        .iter()
        .all(|case| {
            let ms = number(&case["ms"]);
            let mut hasher = Sha256::new();
            for k in 0..=n / 2 {
                let theta = (0.0 + (-2.0 * std::f64::consts::PI * (k as f64 * step)) * ms) * 0.001;
                hasher.update(theta.cos().to_le_bytes());
                hasher.update(theta.sin().to_le_bytes());
            }
            let hex: String = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            Some(hex.as_str()) == case["digest"]["digest"].as_str()
        })
}

pub fn oracle_libm() -> bool {
    static SAME: OnceLock<bool> = OnceLock::new();
    *SAME.get_or_init(|| {
        if std::env::var_os("SECS_ORACLE_FOREIGN_LIBM").is_some() {
            println!("PLATFORM libm treated as foreign (SECS_ORACLE_FOREIGN_LIBM)");
            return false;
        }
        let probe = Fixture::load("libm", "probe");
        let cases = &probe.outputs()["cases"];
        let unary = |name: &str, f: fn(f64) -> f64| {
            cases[name]
                .as_array()
                .unwrap_or_else(|| panic!("libm probe has no {name}"))
                .iter()
                .all(|c| f(number(&c[0])).to_bits() == number(&c[1]).to_bits())
        };
        let binary = |name: &str, f: fn(f64, f64) -> f64| {
            cases[name]
                .as_array()
                .unwrap_or_else(|| panic!("libm probe has no {name}"))
                .iter()
                .all(|c| f(number(&c[0]), number(&c[1])).to_bits() == number(&c[2]).to_bits())
        };
        let same = unary("ln", f64::ln)
            && unary("log10", f64::log10)
            && unary("log2", f64::log2)
            && unary("exp", f64::exp)
            && unary("sin", f64::sin)
            && unary("cos", f64::cos)
            && unary("tan", f64::tan)
            && unary("atan", f64::atan)
            && unary("tanh", f64::tanh)
            && binary("atan2", f64::atan2)
            && binary("pow", f64::powf)
            && design_phases_match(&probe);
        if !same {
            println!(
                "PLATFORM libm differs from the oracle's: exact gates widen to {PLATFORM_RTOL:e}"
            );
        }
        same
    })
}

/// `exact` on the oracle's libm, at least [`PLATFORM_RTOL`] elsewhere.
pub fn rtol(exact: f64) -> f64 {
    if oracle_libm() {
        exact
    } else {
        exact.max(PLATFORM_RTOL)
    }
}

/// Largest absolute and relative differences between two equal-length arrays
/// (relative to `max(|expected|, 1)`). NaN must meet NaN, infinities their
/// equal; otherwise the pair counts as infinitely far.
pub fn errors(got: &[f64], expected: &[f64]) -> (f64, f64) {
    assert_eq!(got.len(), expected.len(), "shape");
    let mut worst = (0.0_f64, 0.0_f64);
    for (&g, &e) in got.iter().zip(expected) {
        let (abs, rel) = if g.is_nan() && e.is_nan() || g == e {
            (0.0, 0.0)
        } else if !g.is_finite() || !e.is_finite() {
            (f64::INFINITY, f64::INFINITY)
        } else {
            let abs = (g - e).abs();
            (abs, abs / e.abs().max(1.0))
        };
        worst = (worst.0.max(abs), worst.1.max(rel));
    }
    worst
}

/// Prints `MEASURE <name> max_abs=… max_rel=…` and asserts every element is
/// within `atol + rtol·max(|expected|, 1)`. ADR 0004 sets each budget at
/// 1.5× the measured error (with a floor); the MEASURE line is the evidence.
pub fn assert_close(name: &str, got: &[f64], expected: &[f64], atol: f64, rtol: f64) {
    assert_eq!(got.len(), expected.len(), "{name}: shape");
    let (abs, rel) = errors(got, expected);
    println!(
        "MEASURE {name} n={} max_abs={abs:e} max_rel={rel:e}",
        got.len()
    );
    for (i, (&g, &e)) in got.iter().zip(expected).enumerate() {
        let same = g.is_nan() && e.is_nan() || g == e;
        let near = (g - e).abs() <= atol + rtol * e.abs().max(1.0);
        assert!(same || near, "{name}[{i}]: got {g:e}, expected {e:e}");
    }
}

/// Bit equality on the oracle's libm; [`PLATFORM_RTOL`] elsewhere.
pub fn assert_same(name: &str, got: &[f64], expected: &[f64]) {
    if oracle_libm() {
        assert_eq!(got.len(), expected.len(), "{name}: shape");
        for (i, (&g, &e)) in got.iter().zip(expected).enumerate() {
            assert!(
                g.to_bits() == e.to_bits() || g.is_nan() && e.is_nan(),
                "{name}[{i}]: got {g:e}, expected {e:e}"
            );
        }
        println!("MEASURE {name} n={} exact", got.len());
    } else {
        assert_close(name, got, expected, 0.0, PLATFORM_RTOL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libm_probe_reads_and_reports() {
        let probe = Fixture::load("libm", "probe");
        assert!(
            probe.meta()["oracle"]
                .as_str()
                .is_some_and(|o| o.starts_with("SECS 2"))
        );
        // Either answer is valid; the call must not panic.
        let _ = oracle_libm();
    }

    #[test]
    fn every_sidecar_and_blob_matches_its_descriptor() {
        fn walk(fx: &Fixture, v: &Value, seen: &mut usize) {
            match v {
                Value::Object(map) if map.contains_key("file") && map.contains_key("sha256") => {
                    let values = fx.f64s(v);
                    let first = fx.f64s(&v["first"]);
                    assert_eq!(&values[..first.len()], first.as_slice(), "{}: first", fx.id);
                    *seen += 1;
                }
                Value::Object(map) if map.contains_key("blob") => {
                    fx.blob(v);
                    *seen += 1;
                }
                Value::Object(map) => map.values().for_each(|x| walk(fx, x, seen)),
                Value::Array(items) => items.iter().for_each(|x| walk(fx, x, seen)),
                _ => {}
            }
        }
        let mut fixtures = 0;
        let mut sidecars = 0;
        for family in fs::read_dir(goldens()).expect("oracle/goldens") {
            let family = family
                .expect("entry")
                .file_name()
                .into_string()
                .expect("utf-8");
            for fx in Fixture::all(&family, "") {
                walk(&fx, fx.value(), &mut sidecars);
                fixtures += 1;
            }
        }
        println!("{fixtures} fixtures, {sidecars} sidecars and blobs checked");
        assert!(fixtures > 0);
    }

    #[test]
    fn digest_descriptor_checks_length_ends_and_hash() {
        let values = [1.5, -0.0, f64::NAN, 3.25];
        let mut hasher = Sha256::new();
        values.iter().for_each(|v| hasher.update(v.to_le_bytes()));
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let v = serde_json::json!({"digest": hex, "length": 4,
            "first": [1.5, -0.0, "nan", 3.25], "last": [1.5, -0.0, "nan", 3.25]});
        let fx = Fixture {
            id: "test".into(),
            dir: goldens(),
            value: Value::Null,
        };
        assert!(Fixture::is_digest(&v));
        fx.assert_digest("values", &v, &values);
        let changed = [1.5, 0.0, f64::NAN, 3.25];
        assert!(std::panic::catch_unwind(|| fx.assert_digest("changed", &v, &changed)).is_err());
    }

    #[test]
    fn errors_treat_matching_nonfinite_values_as_equal() {
        let (abs, rel) = errors(
            &[f64::NAN, f64::INFINITY, 1.0],
            &[f64::NAN, f64::INFINITY, 1.5],
        );
        assert_eq!((abs, rel), (0.5, 0.5 / 1.5));
        assert_eq!(errors(&[f64::NAN], &[1.0]).0, f64::INFINITY);
    }
}

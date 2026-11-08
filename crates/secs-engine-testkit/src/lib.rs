#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Oracle-backed facade for `secs_engine::testkit`.
//!
//! This facade keeps the engine's `test-support` feature behind a
//! dev-dependency, so normal builds of dependent crates do not enable it.

use serde_json::Value;

pub use secs_engine::testkit::{
    Case, FixtureStore, MimoAcousticMetrics, RecordedFit, ReplayCase, manual, pairs, peq, perf,
    replay_fitter,
};
pub use secs_oracle::{Fixture, number};

/// The workspace oracle fixture store.
#[derive(Debug)]
pub struct Oracle;

fn fixture(dir: &str, name: &str) -> Fixture {
    Fixture::load(dir, name.split_once('#').map_or(name, |(name, _)| name))
}

impl FixtureStore for Oracle {
    fn load(&self, dir: &str, name: &str) -> Value {
        fixture(dir, name).value().clone()
    }

    fn names(&self, dir: &str, prefix: &str) -> Vec<String> {
        Fixture::all(dir, prefix)
            .into_iter()
            .map(|fixture| {
                fixture
                    .id
                    .strip_prefix(&format!("{dir}/"))
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    fn calls(&self, dir: &str, prefix: &str) -> Vec<(String, Value)> {
        Fixture::all_calls(dir, prefix)
            .into_iter()
            .map(|fixture| {
                let value = fixture.value().clone();
                (fixture.id, value)
            })
            .collect()
    }

    fn f64s(&self, dir: &str, name: &str, value: &Value) -> Vec<f64> {
        fixture(dir, name).f64s(value)
    }

    fn blob(&self, dir: &str, name: &str, value: &Value) -> Vec<u8> {
        fixture(dir, name).blob(value)
    }

    fn assert_digest(&self, dir: &str, name: &str, field: &str, descriptor: &Value, got: &[f64]) {
        fixture(dir, name).assert_digest(field, descriptor, got);
    }

    fn platform_rtol(&self) -> f64 {
        secs_oracle::PLATFORM_RTOL
    }

    fn oracle_libm(&self) -> bool {
        secs_oracle::oracle_libm()
    }
}

/// Checked replay backed by [`Oracle`].
#[derive(Debug)]
pub struct Replay(secs_engine::testkit::Replay);

impl Replay {
    pub fn for_case(name: &str) -> Self {
        Self(secs_engine::testkit::Replay::for_case(&Oracle, name))
    }

    pub fn factory(&self) -> Box<secs_engine::FitterFactory<'_>> {
        self.0.factory()
    }

    pub fn finish(self) {
        self.0.finish();
    }
}

pub fn names() -> Vec<String> {
    secs_engine::testkit::names(&Oracle)
}

pub fn load(name: &str) -> Case {
    secs_engine::testkit::load(&Oracle, name)
}

pub fn load_fixture(fixture: &Fixture) -> Case {
    let (dir, name) = fixture.id.split_once('/').expect("fixture family/name");
    secs_engine::testkit::load_fixture(&Oracle, dir, name, fixture.value())
}

pub fn production_case(name: &str, threads: usize) -> ReplayCase {
    secs_engine::testkit::production_case(&Oracle, name, threads)
}

pub fn replay_case(name: &str) -> ReplayCase {
    secs_engine::testkit::replay_case(&Oracle, name)
}

pub fn mimo_acoustic_metrics(
    case: &ReplayCase,
    run: &secs_engine::MimoRun,
    ctx: &secs_compute::Context,
) -> Result<Vec<MimoAcousticMetrics>, secs_engine::EngineError> {
    secs_engine::testkit::mimo_acoustic_metrics(&Oracle, case, run, ctx)
}

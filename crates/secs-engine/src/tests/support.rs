use crate::testkit::FixtureStore;
use secs_oracle::Fixture;
use serde_json::Value;

pub(crate) struct Oracle;

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

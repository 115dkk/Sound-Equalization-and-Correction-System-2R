use super::*;

pub(super) fn measurement_values(project: &Project) -> &[PyJson] {
    match &project.measurements {
        PyJson::Array(records) => records,
        _ => &[],
    }
}

pub(super) fn measurement_values_mut(project: &mut Project) -> &mut Vec<PyJson> {
    if !matches!(project.measurements, PyJson::Array(_)) {
        project.measurements = PyJson::Array(Vec::new());
    }
    match &mut project.measurements {
        PyJson::Array(records) => records,
        _ => unreachable!("measurements was replaced with an array"),
    }
}

pub(super) fn pos_of(value: &PyJson) -> Option<i64> {
    crate::project::integer(Some(value.get("pos")?), -1, "measurement.pos").ok()
}

pub(super) fn pos_of_or_minus_one(value: &PyJson) -> i64 {
    let Some(object) = value.object() else {
        return -1;
    };
    let Some(value) = value_for(object, "pos") else {
        return -1;
    };
    crate::project::integer(Some(value), -1, "measurement.pos").unwrap_or(-1)
}

pub(super) fn new_entry(pos: i64, label: String) -> PyJson {
    PyJson::Object(vec![
        ("pos".into(), PyJson::Int(pos.to_string())),
        ("label".into(), PyJson::Str(label)),
        ("uuid".into(), PyJson::Object(Vec::new())),
        ("mdat".into(), PyJson::Null),
        ("ir".into(), PyJson::Object(Vec::new())),
        ("spl_db".into(), PyJson::Object(Vec::new())),
        ("t0".into(), PyJson::Object(Vec::new())),
        ("done".into(), PyJson::Bool(false)),
        ("sample_rate".into(), PyJson::Null),
    ])
}

pub(super) fn repair_entry(object: &mut Vec<(String, PyJson)>, pos: i64, label: String) {
    set_value(object, "pos", PyJson::Int(pos.to_string()));
    setdefault_value(object, "label", PyJson::Str(label));
    setdefault_value(object, "mdat", PyJson::Null);
    for key in ["uuid", "ir", "spl_db", "t0"] {
        if !value_for(object, key).is_some_and(|value| matches!(value, PyJson::Object(_))) {
            set_value(object, key, PyJson::Object(Vec::new()));
        }
    }
    if value_for(object, "spl_offset").is_some_and(|value| !matches!(value, PyJson::Object(_))) {
        set_value(object, "spl_offset", PyJson::Object(Vec::new()));
    }
    setdefault_value(object, "done", PyJson::Bool(false));
    setdefault_value(object, "sample_rate", PyJson::Null);
}

pub(super) fn value_for<'a>(object: &'a [(String, PyJson)], key: &str) -> Option<&'a PyJson> {
    object
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

pub(super) fn value_for_mut<'a>(object: &'a mut [(String, PyJson)], key: &str) -> Option<&'a mut PyJson> {
    object
        .iter_mut()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

pub(super) fn set_value(object: &mut Vec<(String, PyJson)>, key: &str, value: PyJson) {
    if let Some(old) = value_for_mut(object, key) {
        *old = value;
    } else {
        object.push((key.into(), value));
    }
}

pub(super) fn setdefault_value(object: &mut Vec<(String, PyJson)>, key: &str, value: PyJson) {
    if value_for(object, key).is_none() {
        object.push((key.into(), value));
    }
}

pub(super) fn nested_object<'a>(object: &'a [(String, PyJson)], key: &str) -> Option<&'a [(String, PyJson)]> {
    value_for(object, key).and_then(PyJson::object)
}

pub(super) fn set_nested(object: &mut [(String, PyJson)], key: &str, role: &str, value: PyJson) {
    let nested = value_for_mut(object, key)
        .and_then(PyJson::object_mut)
        .expect("repair_entry made the field an object");
    set_value(nested, role, value);
}

pub(super) fn setdefault_nested(object: &mut Vec<(String, PyJson)>, key: &str, role: &str, value: PyJson) {
    setdefault_value(object, key, PyJson::Object(Vec::new()));
    let nested = value_for_mut(object, key)
        .and_then(PyJson::object_mut)
        .expect("upstream setdefault requires an object");
    set_value(nested, role, value);
}

pub(super) fn string_map(value: Option<&PyJson>) -> BTreeMap<String, String> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .filter_map(|(key, value)| value.str().map(|value| (key.clone(), value.into())))
                .collect()
        })
}

pub(super) fn optional_string_map(value: Option<&PyJson>) -> BTreeMap<String, Option<String>> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.str().map(str::to_owned)))
                .collect()
        })
}

pub(super) fn float_map(value: Option<&PyJson>) -> BTreeMap<String, f64> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    crate::project::float(value)
                        .ok()
                        .map(|value| (key.clone(), value))
                })
                .collect()
        })
}

pub(super) fn optional_float_map(value: Option<&PyJson>) -> BTreeMap<String, Option<f64>> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if matches!(value, PyJson::Null) {
                            None
                        } else {
                            crate::project::float(value).ok()
                        },
                    )
                })
                .collect()
        })
}


//! Validation shared by export writers before they emit text or bytes.

use crate::GraphError;

/// Require a finite numeric value and name its field on failure.
pub fn finite(name: &str, value: f64) -> Result<(), GraphError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(GraphError::NonFinite(name.into()))
    }
}

/// Require a non-empty ASCII identifier made of alphanumerics and the
/// caller's explicitly permitted punctuation.
pub fn identifier(name: &str, value: &str, extra_ascii: &[u8]) -> Result<(), GraphError> {
    if !value.is_ascii() {
        return Err(GraphError::NonAscii(format!("{name} = {value:?}")));
    }
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || extra_ascii.contains(&byte))
    {
        return Err(GraphError::Invalid(format!(
            "unsafe identifier {name} = {value:?}"
        )));
    }
    Ok(())
}

/// Require every output name to occur once.
pub fn distinct_outputs(names: &[String]) -> Result<(), GraphError> {
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Err(GraphError::Invalid(format!(
                "duplicate output name {name:?}"
            )));
        }
    }
    Ok(())
}

/// Reject line breaks in a value that one output line must contain.
pub fn no_line_breaks(name: &str, value: &str) -> Result<(), GraphError> {
    if value.contains(['\n', '\r']) {
        Err(GraphError::Invalid(format!("unsafe {name} = {value:?}")))
    } else {
        Ok(())
    }
}

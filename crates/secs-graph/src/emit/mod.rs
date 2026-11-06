//! Text and byte emission rules shared by export writers.

pub mod guard;
pub mod note;
pub mod pyfmt;

/// Format an unsigned integer with ASCII thousands separators.
pub fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

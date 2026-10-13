//! The small subset of CPython's float formatting used by the writer.

use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::{One, Zero};

fn parts(value: f64) -> (bool, u64, i32) {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    if raw_exp == 0 {
        (negative, fraction, -1074)
    } else {
        (negative, fraction | (1_u64 << 52), raw_exp - 1023 - 52)
    }
}

fn pow10(power: u32) -> BigUint {
    BigUint::from(10_u8).pow(power)
}

/// The half-even integer nearest to `abs(value) * 10.powi(scale)`.
fn coefficient(value: f64, scale: i32) -> BigUint {
    let (_, mantissa, exponent) = parts(value.abs());
    if mantissa == 0 {
        return BigUint::zero();
    }
    let mut numerator = BigUint::from(mantissa);
    let mut denominator = BigUint::one();
    if exponent >= 0 {
        numerator <<= exponent as usize;
    } else {
        denominator <<= (-exponent) as usize;
    }
    if scale >= 0 {
        numerator *= pow10(scale as u32);
    } else {
        denominator *= pow10((-scale) as u32);
    }
    let (mut quotient, remainder) = numerator.div_rem(&denominator);
    let twice = &remainder << 1_usize;
    if twice > denominator || (twice == denominator && quotient.is_odd()) {
        quotient += 1_u8;
    }
    quotient
}

fn decimal_from_coefficient(coefficient: &BigUint, places: usize) -> String {
    let digits = coefficient.to_str_radix(10);
    if places == 0 {
        return digits;
    }
    if digits.len() <= places {
        format!("0.{}{}", "0".repeat(places - digits.len()), digits)
    } else {
        let split = digits.len() - places;
        format!("{}.{}", &digits[..split], &digits[split..])
    }
}

/// CPython `float.__round__(value, ndigits)` for the positive ndigits used here.
pub fn round_ndigits(value: f64, ndigits: i32) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let rounded = coefficient(value, ndigits);
    let mut text = decimal_from_coefficient(&rounded, ndigits.max(0) as usize);
    if value.is_sign_negative() {
        text.insert(0, '-');
    }
    let out = text.parse::<f64>().expect("rounded decimal is an f64");
    if out == 0.0 && value.is_sign_negative() {
        -0.0
    } else {
        out
    }
}

/// Python's shortest-round-trip `repr(float)` layout.
pub fn repr_f64(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    if value == 0.0 && value.is_sign_negative() {
        return "-0.0".into();
    }
    let mut text = ryu::Buffer::new().format_finite(value).to_owned();
    let absolute = value.abs();
    if !text.contains(['e', 'E']) && absolute != 0.0 && (absolute < 1e-4 || absolute >= 1e16) {
        let negative = text.starts_with('-');
        let unsigned = text.trim_start_matches('-');
        let (digits, exponent) = if absolute < 1.0 {
            let fractional = unsigned.trim_start_matches("0.");
            let zeroes = fractional.chars().take_while(|ch| *ch == '0').count();
            (
                fractional.trim_start_matches('0').to_owned(),
                -(zeroes as i32) - 1,
            )
        } else {
            let whole = unsigned.find('.').unwrap_or(unsigned.len());
            (unsigned.replace('.', ""), whole as i32 - 1)
        };
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.into()
        } else {
            format!("{first}.{rest}")
        };
        text = format!("{}{mantissa}e{exponent:+}", if negative { "-" } else { "" });
    }
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    if let Some(index) = text.find('e') {
        let exponent = text[index + 1..].parse::<i32>().expect("ryu exponent");
        text = format!(
            "{}e{}{:02}",
            &text[..index],
            if exponent >= 0 { '+' } else { '-' },
            exponent.abs()
        );
    }
    text
}

fn decimal_exponent(value: f64) -> i32 {
    let text = ryu::Buffer::new().format_finite(value.abs()).to_owned();
    if let Some(index) = text.find('e') {
        let before = &text[..index];
        let exponent = text[index + 1..].parse::<i32>().expect("ryu exponent");
        return exponent + before.find('.').unwrap_or(before.len()) as i32 - 1;
    }
    let dot = text.find('.').unwrap_or(text.len());
    if !text.starts_with('0') {
        return dot as i32 - 1;
    }
    let fraction = text.strip_prefix("0.").unwrap_or("");
    -(fraction.chars().take_while(|ch| *ch == '0').count() as i32) - 1
}

/// Python's default-precision `g` format, with an optional positive sign.
pub fn format_g(value: f64, plus: bool) -> String {
    if value == 0.0 {
        return format!(
            "{}0",
            if value.is_sign_negative() {
                "-"
            } else if plus {
                "+"
            } else {
                ""
            }
        );
    }
    let sign = if value.is_sign_negative() {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    let mut exponent = decimal_exponent(value);
    let mut digits = coefficient(value, 5 - exponent).to_str_radix(10);
    if digits.len() > 6 {
        exponent += 1;
        digits.pop();
    }
    while digits.ends_with('0') {
        digits.pop();
    }
    if !(-4..6).contains(&exponent) {
        let mut mantissa = digits[..1].to_owned();
        if digits.len() > 1 {
            mantissa.push('.');
            mantissa.push_str(&digits[1..]);
        }
        format!(
            "{sign}{mantissa}e{}{:02}",
            if exponent >= 0 { '+' } else { '-' },
            exponent.abs()
        )
    } else {
        let decimal = exponent + 1;
        let body = if decimal <= 0 {
            format!("0.{}{}", "0".repeat((-decimal) as usize), digits)
        } else if decimal as usize >= digits.len() {
            format!("{}{}", digits, "0".repeat(decimal as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..decimal as usize],
                &digits[decimal as usize..]
            )
        };
        format!("{sign}{body}")
    }
}

/// Python fixed-point format (`.1f`, `.2f`, `.3f`) with optional positive sign.
pub fn format_fixed(value: f64, places: usize, plus: bool) -> String {
    let sign = if value.is_sign_negative() {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    format!(
        "{sign}{}",
        decimal_from_coefficient(&coefficient(value, places as i32), places)
    )
}

/// Python `{:3d}`.
pub fn format_i3(value: usize) -> String {
    format!("{value:>3}")
}

/// Python `{:30s}` (minimum width; never truncates).
pub fn format_s30(value: &str) -> String {
    format!("{value:<30}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ties_and_negative_zero() {
        assert_eq!(format_fixed(2.675, 2, true), "+2.67");
        assert_eq!(
            format_fixed(f64::from_bits(2.675_f64.to_bits() + 1), 2, true),
            "+2.68"
        );
        assert_eq!(repr_f64(round_ndigits(-0.000_01, 4)), "-0.0");
        assert_eq!(format_g(5e-324, false), "4.94066e-324");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Sensor value format functions based on the AOOSTAR-X application.

#[derive(Debug, Clone)]
pub enum IntegerDigits {
    /// Keep all integer digits
    Auto, // -1 in Python
    /// Only keep the integer part of a decimal number
    Zero, // 0 in Python
    /// Limit integer part of a decimal number to the given length
    Fixed(usize), // positive values
}

impl From<i32> for IntegerDigits {
    fn from(value: i32) -> Self {
        match value {
            -1 => IntegerDigits::Auto,
            0 => IntegerDigits::Zero,
            n if n > 0 => IntegerDigits::Fixed(n as usize),
            _ => IntegerDigits::Auto,
        }
    }
}

impl From<Option<i32>> for IntegerDigits {
    fn from(value: Option<i32>) -> Self {
        match value {
            None => IntegerDigits::Auto,
            Some(digits) => IntegerDigits::from(digits),
        }
    }
}

/// Format a sensor value in string format to the specified fixed point number.
///
/// # Arguments
///
/// * `value`: decimal number to format
/// * `integer_digits`: number of integer places
/// * `decimal_digits`: fixed point numbers
/// * `unit`: unit suffix to append after the formatted number
///
/// returns: String
///
/// # Examples
///
/// ```
/// let value = asterctl::format_value("123.456", asterctl::IntegerDigits::Auto, 0, "foobar");
/// assert_eq!(value, "123foobar");
/// ```
pub fn format_value(
    value: &str,
    integer_digits: IntegerDigits,
    decimal_digits: usize,
    unit: &str,
) -> String {
    let num = match value.parse::<f64>() {
        Ok(n) => n,
        Err(_) => return format!("{}{}", value, unit),
    };

    // Round number to the specified decimal digits
    let factor = 10f64.powi(decimal_digits as i32);
    let rounded = if decimal_digits == 0 {
        num.round()
    } else {
        (num * factor).round() / factor
    };

    // Get integer and decimal parts
    // The integer part may increase due to rounding!
    let integer_part = rounded.trunc() as i64;
    let decimal_part = if decimal_digits > 0 {
        let mut dec = (rounded.fract().abs() * factor).round() as u64;
        // Handle cases where rounding makes the decimal part equal to factor
        if dec == factor as u64 {
            // e.g. 9.999 rounded to 1 decimal = 10.0
            // We set decimal part to 0
            dec = 0;
        }
        format!("{:0width$}", dec, width = decimal_digits)
    } else {
        "".to_string()
    };

    // Format integer part according to padding rules
    let integer_str = integer_part.to_string();
    let integer_filled = match integer_digits {
        IntegerDigits::Auto => integer_str.clone(),
        IntegerDigits::Zero => "".to_string(),
        IntegerDigits::Fixed(digits) => {
            if integer_str.len() > digits {
                "9".repeat(digits)
            } else {
                format!("{:0width$}", integer_part, width = digits)
            }
        }
    };

    let formatted = if decimal_digits > 0 {
        format!("{}.{}", integer_filled, decimal_part)
    } else {
        integer_filled
    };

    format!("{}{}", formatted, unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(5, 2, "00123.46°C")]
    #[case(5, 1, "00123.5°C")]
    #[case(5, 0, "00123°C")]
    #[case(-1, 2, "123.46°C")]
    #[case(-1, 1, "123.5°C")]
    #[case(-1, 0, "123°C")]
    #[case(2, 0, "99°C")]
    fn test_format_value_with_decimal(
        #[case] digits: i32,
        #[case] decimals: usize,
        #[case] output: &str,
    ) {
        let result = format_value("123.456", IntegerDigits::from(digits), decimals, "°C");
        assert_eq!(output, result);
    }

    #[rstest]
    #[case(5, 2, "00123.00°C")]
    #[case(5, 1, "00123.0°C")]
    #[case(5, 0, "00123°C")]
    #[case(-1, 2, "123.00°C")]
    #[case(-1, 1, "123.0°C")]
    #[case(-1, 0, "123°C")]
    #[case(2, 0, "99°C")]
    fn test_format_value_with_integer(
        #[case] digits: i32,
        #[case] decimals: usize,
        #[case] output: &str,
    ) {
        let result = format_value("123", IntegerDigits::from(digits), decimals, "°C");
        assert_eq!(output, result);
    }

    #[rstest]
    #[case(5, 2, "-0123.00°C")]
    #[case(5, 0, "-0123°C")]
    #[case(-1, 2, "-123.00°C")]
    #[case(-1, 1, "-123.0°C")]
    #[case(-1, 0, "-123°C")]
    #[case(2, 0, "99°C")] // Overflow
    fn test_format_value_with_negative_integer(
        #[case] digits: i32,
        #[case] decimals: usize,
        #[case] output: &str,
    ) {
        let result = format_value("-123", IntegerDigits::from(digits), decimals, "°C");
        assert_eq!(output, result);
    }

    #[rstest]
    #[case("0", 3, 1, "V", "000.0V")]
    #[case("999.99", 2, 1, "%", "99.0%")]
    #[case("invalid", 2, 2, "unit", "invalidunit")]
    fn test_format_value_edge_cases(
        #[case] input: &str,
        #[case] digits: i32,
        #[case] decimals: usize,
        #[case] unit: &str,
        #[case] output: &str,
    ) {
        let result = format_value(input, IntegerDigits::from(digits), decimals, unit);
        assert_eq!(output, result);
    }

    #[rstest]
    #[case("1.999", 2, 1, "", "02.0")]
    #[case("1.999", 2, 0, "", "02")]
    #[case("1.999", 1, 1, "", "2.0")]
    #[case("1.999", -1, 1, "", "2.0")]
    #[case("0.999", 1, 2, "", "1.00")]
    #[case("0.999", 1, 1, "", "1.0")]
    #[case("0.999", 1, 0, "", "1")]
    #[case("123.6", -1, 0, "", "124")]
    fn test_format_value_rounding(
        #[case] input: &str,
        #[case] digits: i32,
        #[case] decimals: usize,
        #[case] unit: &str,
        #[case] output: &str,
    ) {
        let result = format_value(input, IntegerDigits::from(digits), decimals, unit);
        assert_eq!(output, result);
    }
}

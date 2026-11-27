pub(super) fn parse_quantity(quantity: &str, decimal_scale: i32) -> Option<i64> {
    let quantity = quantity.strip_prefix('+').unwrap_or(quantity);
    let suffix_start = quantity
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(quantity.len());
    let (number, suffix) = quantity.split_at(suffix_start);
    let (integer, fraction) = number.split_once('.').unwrap_or((number, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }

    let value = integer
        .bytes()
        .chain(fraction.bytes())
        .try_fold(0_u128, |value, digit| {
            let digit = digit.checked_sub(b'0').filter(|digit| *digit < 10)?;
            value.checked_mul(10)?.checked_add(u128::from(digit))
        })?;
    let (decimal_exponent, binary_exponent) = match suffix {
        "n" => (-9, 0),
        "u" => (-6, 0),
        "m" => (-3, 0),
        "" => (0, 0),
        "k" => (3, 0),
        "M" => (6, 0),
        "G" => (9, 0),
        "T" => (12, 0),
        "P" => (15, 0),
        "E" => (18, 0),
        "Ki" => (0, 10),
        "Mi" => (0, 20),
        "Gi" => (0, 30),
        "Ti" => (0, 40),
        "Pi" => (0, 50),
        "Ei" => (0, 60),
        _ => (suffix.strip_prefix(['e', 'E'])?.parse::<i32>().ok()?, 0),
    };
    let exponent = decimal_exponent
        .checked_add(decimal_scale)?
        .checked_sub(i32::try_from(fraction.len()).ok()?)?;
    let value = value.checked_mul(1_u128 << binary_exponent)?;
    let value = if value == 0 {
        0
    } else if exponent >= 0 {
        value.checked_mul(10_u128.checked_pow(exponent.unsigned_abs())?)?
    } else {
        match 10_u128.checked_pow(exponent.unsigned_abs()) {
            Some(divisor) => value.div_ceil(divisor),
            None => 1,
        }
    };
    i64::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::parse_quantity;

    #[test]
    fn converts_resource_quantities() {
        for (quantity, scale, expected) in [
            ("500m", 3, 500),
            ("1", 3, 1000),
            ("1.5", 3, 1500),
            (".5", 3, 500),
            ("1.", 3, 1000),
            ("+2", 3, 2000),
            ("100u", 3, 1),
            ("1n", 3, 1),
            ("1e-3", 3, 1),
            ("0", 3, 0),
            ("0e100", 0, 0),
            ("256Mi", 0, 268_435_456),
            ("1.5Gi", 0, 1_610_612_736),
            ("1Ki", 0, 1024),
            ("1Ti", 0, 1_099_511_627_776),
            ("1Pi", 0, 1_125_899_906_842_624),
            ("1Ei", 0, 1_152_921_504_606_846_976),
            ("400m", 0, 1),
            ("1k", 0, 1000),
            ("1M", 0, 1_000_000),
            ("1G", 0, 1_000_000_000),
            ("1T", 0, 1_000_000_000_000),
            ("1P", 0, 1_000_000_000_000_000),
            ("1E", 0, 1_000_000_000_000_000_000),
            ("129e6", 0, 129_000_000),
            ("129E+6", 0, 129_000_000),
            ("1e-100", 0, 1),
            ("9007199254740993", 0, 9_007_199_254_740_993),
            ("9223372036854775807", 0, i64::MAX),
        ] {
            assert_eq!(
                parse_quantity(quantity, scale),
                Some(expected),
                "{quantity}"
            );
        }
    }

    #[test]
    fn rejects_invalid_or_unrepresentable_quantities() {
        for quantity in [
            "",
            ".",
            "-1",
            "NaN",
            "inf",
            "1.2.3",
            "1GiB",
            "1K",
            "1Mi ",
            " 1",
            "1e",
            "1e100",
            "1e2147483648",
            "9223372036854775808",
            "8Ei",
            "340282366920938463463374607431768211456",
        ] {
            assert_eq!(parse_quantity(quantity, 0), None, "{quantity}");
        }
        assert_eq!(parse_quantity("9223372036854775807", 3), None);
    }
}

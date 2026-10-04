use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};
use std::str::FromStr;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// A signed number with exactly two decimal places, stored as a whole number of hundredths.
/// All rule arithmetic uses it, never floats, so results are identical on every machine
/// (DESIGN.md §4.1, DECISIONS.md P-1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Fixed(i64);

/// Why some text isn't a `Fixed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFixedError {
    NotANumber(String),
    TooManyDecimals(String),
    OutOfRange(String),
}

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(HUNDREDTHS_PER_UNIT);

    /// `Fixed::from_hundredths(1250)` is 12.50.
    pub const fn from_hundredths(hundredths: i64) -> Fixed {
        Fixed(hundredths)
    }

    /// The value as a whole number of hundredths: 12.50 is 1250.
    pub const fn hundredths(self) -> i64 {
        self.0
    }

    /// `None` if the result is beyond the range a `Fixed` can hold.
    pub fn checked_add(self, rhs: Fixed) -> Option<Fixed> {
        self.0.checked_add(rhs.0).map(Fixed)
    }

    /// `None` if the result is beyond the range a `Fixed` can hold.
    pub fn checked_sub(self, rhs: Fixed) -> Option<Fixed> {
        self.0.checked_sub(rhs.0).map(Fixed)
    }

    /// Computes the exact product, then rounds it once, half away from zero. `None` if the
    /// result is beyond the range a `Fixed` can hold.
    pub fn checked_mul(self, rhs: Fixed) -> Option<Fixed> {
        let exact = i128::from(self.0) * i128::from(rhs.0);
        narrow(div_round(exact, i128::from(HUNDREDTHS_PER_UNIT)))
    }

    /// Computes the exact quotient, then rounds it once, half away from zero. `None` if
    /// `divisor` is zero, or if the result is beyond the range a `Fixed` can hold.
    pub fn checked_div(self, divisor: Fixed) -> Option<Fixed> {
        if divisor.0 == 0 {
            return None;
        }
        let scaled = i128::from(self.0) * i128::from(HUNDREDTHS_PER_UNIT);
        narrow(div_round(scaled, i128::from(divisor.0)))
    }
}

const HUNDREDTHS_PER_UNIT: i64 = 100;

/// A wide intermediate result back as a `Fixed`, if it fits.
fn narrow(hundredths: i128) -> Option<Fixed> {
    i64::try_from(hundredths).ok().map(Fixed)
}

/// Divides, rounding half away from zero: the engine's one rounding rule (DECISIONS.md P-1).
/// Panics if `denominator` is zero.
pub fn div_round(numerator: i128, denominator: i128) -> i128 {
    let (n, d) = (numerator.unsigned_abs(), denominator.unsigned_abs());
    // Round the magnitude half up, then put the sign back: that's half away from zero.
    let magnitude = i128::try_from((n + d / 2) / d).expect("a rounded quotient fits in i128");
    // `numerator ^ denominator` is negative exactly when their signs differ.
    if (numerator ^ denominator) < 0 {
        -magnitude
    } else {
        magnitude
    }
}

// The operators panic, in every build, if a result is beyond the range a `Fixed` can hold:
// about ±92 trillion, which content validation keeps rule values far inside. A silently
// wrapped number would be worse than a crash (DECISIONS.md P-30).

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, rhs: Fixed) -> Fixed {
        self.checked_add(rhs)
            .expect("Fixed addition is out of range")
    }
}

impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, rhs: Fixed) -> Fixed {
        self.checked_sub(rhs)
            .expect("Fixed subtraction is out of range")
    }
}

impl Mul for Fixed {
    type Output = Fixed;
    fn mul(self, rhs: Fixed) -> Fixed {
        self.checked_mul(rhs)
            .expect("Fixed multiplication is out of range")
    }
}

impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        self.0
            .checked_neg()
            .map(Fixed)
            .expect("Fixed negation is out of range")
    }
}

impl FromStr for Fixed {
    type Err = ParseFixedError;

    /// Accepts digits with an optional sign and at most two decimal places: `12`, `-0.5`,
    /// `+3.25`. Anything else, including `.5`, `5.` and exponents, is not a number.
    fn from_str(text: &str) -> Result<Fixed, ParseFixedError> {
        let not_a_number = || ParseFixedError::NotANumber(text.to_owned());
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, fraction) = match unsigned.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (unsigned, None),
        };
        let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !all_digits(whole) || fraction.is_some_and(|fraction| !all_digits(fraction)) {
            return Err(not_a_number());
        }
        let fraction = fraction.unwrap_or("");
        if fraction.len() > 2 {
            return Err(ParseFixedError::TooManyDecimals(text.to_owned()));
        }

        // Accumulate in a wider integer, so even absurdly long input can't overflow unnoticed.
        let out_of_range = || ParseFixedError::OutOfRange(text.to_owned());
        let padding = "00"[fraction.len()..].bytes();
        let mut hundredths: i128 = 0;
        for digit in whole.bytes().chain(fraction.bytes()).chain(padding) {
            hundredths = hundredths
                .checked_mul(10)
                .and_then(|n| n.checked_add(i128::from(digit - b'0')))
                .ok_or_else(out_of_range)?;
        }
        let signed = if negative { -hundredths } else { hundredths };
        narrow(signed).ok_or_else(out_of_range)
    }
}

impl fmt::Display for Fixed {
    /// Always two decimal places: `12.50`, `-0.05`, `0.00`. Honours width and alignment, so
    /// `{:>7}` lines up columns.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let per_unit = HUNDREDTHS_PER_UNIT.unsigned_abs();
        f.pad(&format!(
            "{sign}{}.{:02}",
            magnitude / per_unit,
            magnitude % per_unit
        ))
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({self})")
    }
}

impl fmt::Display for ParseFixedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseFixedError::NotANumber(text) => write!(f, "'{text}' is not a number"),
            ParseFixedError::TooManyDecimals(text) => {
                write!(f, "{text} has more than 2 decimal places")
            }
            ParseFixedError::OutOfRange(text) => write!(f, "{text} is out of range"),
        }
    }
}

impl std::error::Error for ParseFixedError {}

impl<'de> Deserialize<'de> for Fixed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Fixed, D::Error> {
        deserializer.deserialize_any(FixedVisitor)
    }
}

struct FixedVisitor;

impl Visitor<'_> for FixedVisitor {
    type Value = Fixed;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number with at most 2 decimal places")
    }

    /// A whole number: `30` is 30.00.
    fn visit_i64<E: de::Error>(self, whole: i64) -> Result<Fixed, E> {
        whole
            .checked_mul(HUNDREDTHS_PER_UNIT)
            .map(Fixed)
            .ok_or_else(|| E::custom(ParseFixedError::OutOfRange(whole.to_string())))
    }

    /// A float arrives through its shortest decimal string (`0.3` becomes `"0.3"`), which
    /// parses exactly. No float arithmetic happens anywhere (DESIGN.md §4.1).
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Fixed, E> {
        value.to_string().parse().map_err(E::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// `h(1250)` is 12.50. Tests build values from hundredths, so arithmetic tests don't
    /// depend on parsing being right.
    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn parse(text: &str) -> Result<Fixed, ParseFixedError> {
        text.parse()
    }

    // Parsing and display

    #[test]
    fn parses_decimals_and_whole_numbers() {
        assert_eq!(parse("12.5"), Ok(h(1250)));
        assert_eq!(parse("7"), Ok(h(700)));
        assert_eq!(parse("-0.05"), Ok(h(-5)));
        assert_eq!(parse("+3.25"), Ok(h(325)));
        assert_eq!(parse("-0.00"), Ok(h(0)));
    }

    #[test]
    fn displays_exactly_two_decimals() {
        assert_eq!(h(1250).to_string(), "12.50");
        assert_eq!(h(700).to_string(), "7.00");
        assert_eq!(h(-5).to_string(), "-0.05");
        assert_eq!(h(-1234).to_string(), "-12.34");
        assert_eq!(h(0).to_string(), "0.00");
    }

    #[test]
    fn negative_zero_displays_as_zero() {
        assert_eq!(parse("-0.00").map(|x| x.to_string()), Ok("0.00".into()));
    }

    #[test]
    fn display_respects_width_and_alignment() {
        assert_eq!(format!("{:>7}", h(1250)), "  12.50");
        assert_eq!(format!("{:<7}|", h(-5)), "-0.05  |");
    }

    #[test]
    fn exposes_its_value_in_hundredths() {
        assert_eq!(h(1250).hundredths(), 1250);
        assert_eq!(parse("-0.05").map(Fixed::hundredths), Ok(-5));
        assert_eq!(Fixed::ONE.hundredths(), 100);
    }

    #[test]
    fn debug_shows_the_decimal_value() {
        assert_eq!(format!("{:?}", h(1250)), "Fixed(12.50)");
    }

    // Bad input

    #[test]
    fn rejects_more_than_two_decimal_places() {
        assert_eq!(
            parse("12.345").unwrap_err().to_string(),
            "12.345 has more than 2 decimal places"
        );
    }

    #[test]
    fn rejects_text_that_is_not_a_number_and_names_it() {
        for input in [
            "", "1.2.3", "abc", ".5", "5.", "1 2", "- 1", "--1", "1e3", "1.2a",
        ] {
            assert_eq!(
                parse(input).unwrap_err().to_string(),
                format!("'{input}' is not a number"),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn rejects_numbers_beyond_the_range_it_can_hold() {
        assert_eq!(parse("92233720368547758.07"), Ok(h(i64::MAX)));
        assert_eq!(parse("-92233720368547758.08"), Ok(h(i64::MIN)));
        for input in [
            "92233720368547758.08",
            "-92233720368547758.09",
            "1000000000000000000000000000000000000000",
        ] {
            assert_eq!(
                parse(input).unwrap_err().to_string(),
                format!("{input} is out of range")
            );
        }
    }

    // Arithmetic

    #[test]
    fn adds_subtracts_and_negates_exactly() {
        assert_eq!(h(1250) + h(-5), h(1245));
        assert_eq!(h(100) - h(325), h(-225));
        assert_eq!(-h(325), h(-325));
    }

    #[test]
    fn multiplies_rounding_half_away_from_zero() {
        assert_eq!(h(5) * h(50), h(3)); // 0.05 × 0.50 = 0.025 → 0.03
        assert_eq!(h(-5) * h(50), h(-3)); // −0.05 × 0.50 = −0.025 → −0.03
        assert_eq!(h(400) * h(41), h(164)); // 4.00 × 0.41 = 1.64
        assert_eq!(h(4) * h(10), h(0)); // 0.04 × 0.10 = 0.004 → 0.00
    }

    #[test]
    fn divides_rounding_half_away_from_zero() {
        assert_eq!(h(100).checked_div(h(300)), Some(h(33))); // 1.00 ÷ 3.00 = 0.333… → 0.33
        assert_eq!(h(200).checked_div(h(300)), Some(h(67))); // 0.666… → 0.67
        assert_eq!(h(-200).checked_div(h(300)), Some(h(-67)));
        assert_eq!(h(200).checked_div(h(-300)), Some(h(-67)));
        assert_eq!(h(1).checked_div(h(200)), Some(h(1))); // 0.01 ÷ 2.00 = 0.005 → 0.01
    }

    #[test]
    fn division_by_zero_is_refused() {
        assert_eq!(h(100).checked_div(h(0)), None);
    }

    #[test]
    fn checked_arithmetic_refuses_results_beyond_the_range() {
        assert_eq!(h(i64::MAX).checked_add(h(1)), None);
        assert_eq!(h(i64::MIN).checked_sub(h(1)), None);
        assert_eq!(h(i64::MAX).checked_mul(h(200)), None);
        assert_eq!(h(i64::MAX).checked_div(h(1)), None);
        assert_eq!(h(100).checked_add(h(5)), Some(h(105)));
        assert_eq!(h(100).checked_sub(h(5)), Some(h(95)));
        assert_eq!(h(250).checked_mul(h(200)), Some(h(500)));
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn addition_beyond_the_range_panics_in_every_build() {
        let _ = h(i64::MAX) + h(1);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn subtraction_beyond_the_range_panics_in_every_build() {
        let _ = h(i64::MIN) - h(1);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn multiplication_beyond_the_range_panics_in_every_build() {
        let _ = h(i64::MAX) * h(200);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn negating_the_smallest_value_panics_in_every_build() {
        let _ = -h(i64::MIN);
    }

    #[test]
    fn clamps_into_a_range() {
        assert_eq!((h(9500) + h(1000)).clamp(h(-10000), h(10000)), h(10000));
    }

    #[test]
    fn div_round_rounds_half_away_from_zero() {
        assert_eq!(div_round(250, 100), 3);
        assert_eq!(div_round(-250, 100), -3);
        assert_eq!(div_round(250, -100), -3);
        assert_eq!(div_round(-250, -100), 3);
        assert_eq!(div_round(249, 100), 2);
        assert_eq!(div_round(-249, 100), -2);
        assert_eq!(div_round(7, 7), 1);
        assert_eq!(div_round(-7, -7), 1);
        assert_eq!(div_round(0, 7), 0);
        assert_eq!(div_round(0, -7), 0);
    }

    // TOML

    #[derive(Debug, serde::Deserialize)]
    struct Doc {
        x: Fixed,
    }

    fn from_toml(text: &str) -> Result<Fixed, String> {
        toml::from_str::<Doc>(text)
            .map(|doc| doc.x)
            .map_err(|error| error.message().to_owned())
    }

    #[test]
    fn reads_toml_floats_exactly() {
        assert_eq!(from_toml("x = 0.3"), Ok(h(30)));
        assert_eq!(from_toml("x = -12.75"), Ok(h(-1275)));
    }

    #[test]
    fn reads_toml_integers_as_whole_numbers() {
        assert_eq!(from_toml("x = 30"), Ok(h(3000)));
        assert_eq!(from_toml("x = -7"), Ok(h(-700)));
    }

    #[test]
    fn rejects_toml_numbers_with_more_than_two_decimals() {
        assert_eq!(
            from_toml("x = 0.333"),
            Err("0.333 has more than 2 decimal places".into())
        );
    }

    #[test]
    fn rejects_toml_integers_beyond_the_range() {
        assert_eq!(
            from_toml("x = 92233720368547759"),
            Err("92233720368547759 is out of range".into())
        );
    }

    #[test]
    fn rejects_toml_values_that_are_not_finite_numbers() {
        assert_eq!(from_toml("x = nan"), Err("'NaN' is not a number".into()));
        assert_eq!(from_toml("x = inf"), Err("'inf' is not a number".into()));
    }

    #[test]
    fn rejects_toml_values_that_are_not_numbers() {
        let error = from_toml("x = \"12.5\"").unwrap_err();
        assert!(
            error.contains("expected a number with at most 2 decimal places"),
            "{error}"
        );
    }

    // Properties

    fn any_fixed() -> impl Strategy<Value = Fixed> {
        any::<i64>().prop_map(h)
    }

    /// −1,000,000.00 to 1,000,000.00: far wider than any rule value, and products stay in range.
    fn domain_fixed() -> impl Strategy<Value = Fixed> {
        (-100_000_000_i64..=100_000_000).prop_map(h)
    }

    proptest! {
        #[test]
        fn display_then_parse_gives_back_the_same_value(x in any_fixed()) {
            prop_assert_eq!(x.to_string().parse::<Fixed>(), Ok(x));
        }

        #[test]
        fn multiplication_commutes(a in domain_fixed(), b in domain_fixed()) {
            prop_assert_eq!(a * b, b * a);
        }

        #[test]
        fn multiplication_is_within_half_a_hundredth_of_the_exact_product(
            a in domain_fixed(),
            b in domain_fixed(),
        ) {
            // The exact product, in ten-thousandths, against ours scaled to match.
            let exact = i128::from(a.hundredths()) * i128::from(b.hundredths());
            let ours = i128::from((a * b).hundredths()) * 100;
            prop_assert!((ours - exact).abs() <= 50);
        }

        #[test]
        fn rounding_treats_negative_numbers_symmetrically(
            a in domain_fixed(),
            b in domain_fixed(),
        ) {
            prop_assert_eq!((-a) * b, -(a * b));
        }

        #[test]
        fn division_is_within_half_a_hundredth_of_the_exact_quotient(
            a in domain_fixed(),
            b in domain_fixed().prop_filter("non-zero divisor", |b| *b != Fixed::ZERO),
        ) {
            // q is the nearest hundredth to a ÷ b when |q·b − 100·a| ≤ |b| / 2.
            let q = i128::from(a.checked_div(b).expect("in range").hundredths());
            let (a, b) = (i128::from(a.hundredths()), i128::from(b.hundredths()));
            prop_assert!(2 * (q * b - 100 * a).abs() <= b.abs());
        }

        #[test]
        fn clamping_stays_within_its_bounds(
            x in any_fixed(),
            bound1 in domain_fixed(),
            bound2 in domain_fixed(),
        ) {
            let (low, high) = (bound1.min(bound2), bound1.max(bound2));
            let clamped = x.clamp(low, high);
            prop_assert!(low <= clamped && clamped <= high);
        }
    }
}

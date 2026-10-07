use std::fmt;

use crate::{Fixed, div_round};

/// An exact product of fixed-point numbers and curve values, kept as a fraction so a
/// computation built from several of them rounds once, at the end (P-1). Always in lowest
/// terms with a positive denominator, so equal values compare equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    numerator: i128,
    denominator: i128,
}

impl Ratio {
    pub const ONE: Ratio = Ratio {
        numerator: 1,
        denominator: 1,
    };

    /// `numerator / denominator`; `denominator` must not be 0.
    pub(crate) fn new(numerator: i128, denominator: i128) -> Ratio {
        let divisor = gcd(numerator, denominator).max(1);
        let sign = if denominator < 0 { -1 } else { 1 };
        Ratio {
            numerator: sign * numerator / divisor,
            denominator: sign * denominator / divisor,
        }
    }

    /// A fixed-point number, exactly.
    pub fn from_fixed(value: Fixed) -> Ratio {
        Ratio::new(i128::from(value.hundredths()), 100)
    }

    /// The exact product; `None` if it's too big to hold.
    pub fn checked_mul(self, other: Ratio) -> Option<Ratio> {
        // Cross-cancelling first keeps the parts as small as they can be.
        let a = gcd(self.numerator, other.denominator).max(1);
        let b = gcd(other.numerator, self.denominator).max(1);
        let numerator = (self.numerator / a).checked_mul(other.numerator / b)?;
        let denominator = (self.denominator / b).checked_mul(other.denominator / a)?;
        Some(Ratio::new(numerator, denominator))
    }

    /// The value rounded to hundredths, half away from zero; `None` if it's beyond the range
    /// a `Fixed` can hold.
    pub fn round(self) -> Option<Fixed> {
        let hundredths = div_round(self.numerator.checked_mul(100)?, self.denominator);
        i64::try_from(hundredths).ok().map(Fixed::from_hundredths)
    }
}

impl serde::Serialize for Ratio {
    /// As `numerator/denominator`, such as `"81/200"`, for saves (T4).
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{}/{}", self.numerator, self.denominator))
    }
}

impl<'de> serde::Deserialize<'de> for Ratio {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Ratio, D::Error> {
        let text = String::deserialize(deserializer)?;
        let parts = text
            .split_once('/')
            .and_then(|(n, d)| Some((n.parse::<i128>().ok()?, d.parse::<i128>().ok()?)));
        match parts {
            Some((numerator, denominator)) if denominator > 0 => {
                Ok(Ratio::new(numerator, denominator))
            }
            _ => Err(serde::de::Error::custom(format!(
                "'{text}' isn't a fraction like 81/200 with a positive denominator"
            ))),
        }
    }
}

impl fmt::Display for Ratio {
    /// Exactly, with two to four decimals, such as `0.58` or `0.405`; a value that needs more
    /// is shown to four, marked `≈`, such as `≈0.3333`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const PLACES: i128 = 10_000;
        let scaled = self.numerator.saturating_mul(PLACES);
        let exact = scaled % self.denominator == 0;
        let places = div_round(scaled, self.denominator);
        let sign = if places < 0 { "-" } else { "" };
        let magnitude = places.unsigned_abs();
        let decimals = format!("{:04}", magnitude % 10_000);
        let decimals = decimals.trim_end_matches('0');
        let approximately = if exact { "" } else { "≈" };
        write!(
            f,
            "{approximately}{sign}{}.{decimals:0<2}",
            magnitude / 10_000
        )
    }
}

/// The greatest common divisor of `a` and `b`, ignoring signs; 0 only if both are 0.
fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    i128::try_from(a).unwrap_or(i128::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn ratio(value: i64) -> Ratio {
        Ratio::from_fixed(h(value))
    }

    #[test]
    fn multiplies_exactly_and_rounds_once() {
        // 4.00 × 0.405 is 1.62; rounding 0.405 first would give 4.00 × 0.41 = 1.64.
        let multiplier = Ratio::new(405, 1000);
        let product = ratio(4_00).checked_mul(multiplier).expect("small");
        assert_eq!(product.round(), Some(h(1_62)));
        // 0.25 × 0.25 × 0.5 = 0.03125, rounded once to 0.03.
        let product = ratio(25)
            .checked_mul(ratio(25))
            .and_then(|p| p.checked_mul(ratio(50)))
            .expect("small");
        assert_eq!(product.round(), Some(h(3)));
        // Common factors cancel across the two, either way round.
        assert_eq!(
            Ratio::new(1, 2).checked_mul(Ratio::new(2, 3)),
            Some(Ratio::new(1, 3))
        );
        assert_eq!(
            Ratio::new(3, 4).checked_mul(Ratio::new(1, 3)),
            Some(Ratio::new(1, 4))
        );
    }

    #[test]
    fn rounds_half_away_from_zero() {
        assert_eq!(Ratio::new(5, 1000).round(), Some(h(1)));
        assert_eq!(Ratio::new(-5, 1000).round(), Some(h(-1)));
        assert_eq!(Ratio::new(4, 1000).round(), Some(h(0)));
        assert_eq!(Ratio::new(1, 3).round(), Some(h(33)));
        assert_eq!(Ratio::new(2, 3).round(), Some(h(67)));
    }

    #[test]
    fn equal_values_are_equal_however_they_were_made() {
        assert_eq!(Ratio::new(1, 2), Ratio::new(50, 100));
        assert_eq!(Ratio::new(-1, 2), Ratio::new(1, -2));
        assert_eq!(ratio(1_00), Ratio::ONE);
        assert_ne!(Ratio::new(1, 2), Ratio::new(1, 3));
    }

    #[test]
    fn says_when_a_value_is_too_big() {
        let huge = Ratio::from_fixed(h(i64::MAX));
        assert_eq!(huge.round(), Some(h(i64::MAX)));
        let bigger = huge.checked_mul(ratio(2_00)).expect("fits in i128");
        assert_eq!(bigger.round(), None);
        let mut power = huge;
        let mut overflowed = false;
        for _ in 0..4 {
            match power.checked_mul(huge) {
                Some(next) => power = next,
                None => overflowed = true,
            }
        }
        assert!(overflowed, "i64::MAX⁵ can't fit in i128");
    }

    #[test]
    fn saves_as_a_fraction_in_lowest_terms() {
        let json = serde_json::to_string(&Ratio::new(405, 1000)).expect("serialises");
        assert_eq!(json, "\"81/200\"");
        let read = |text: &str| serde_json::from_str::<Ratio>(text).ok();
        assert_eq!(read("\"81/200\""), Some(Ratio::new(81, 200)));
        assert_eq!(
            read("\"-2/4\""),
            Some(Ratio::new(-1, 2)),
            "put in lowest terms"
        );
        assert_eq!(read("\"1/0\""), None, "no denominator of 0");
        assert_eq!(read("\"1/-2\""), None, "the denominator is positive");
        assert_eq!(read("\"half\""), None);
        assert_eq!(read("\"1\""), None);
    }

    #[test]
    fn shows_exact_values_with_two_to_four_decimals() {
        assert_eq!(Ratio::new(58, 100).to_string(), "0.58");
        assert_eq!(Ratio::new(405, 1000).to_string(), "0.405");
        assert_eq!(Ratio::ONE.to_string(), "1.00");
        assert_eq!(Ratio::new(0, 7).to_string(), "0.00");
        assert_eq!(Ratio::new(-12_345, 10_000).to_string(), "-1.2345");
        assert_eq!(Ratio::new(1, 3).to_string(), "≈0.3333");
        assert_eq!(Ratio::new(-2, 3).to_string(), "≈-0.6667");
    }
}

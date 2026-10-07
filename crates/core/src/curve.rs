use std::fmt;

use serde::de::{self, Deserialize, Deserializer, IntoDeserializer, SeqAccess, Visitor};

use crate::fixed::Number;
use crate::{Fixed, Ratio};

/// A piecewise-linear map from one number to another: the shape of most tuning knobs
/// (DESIGN.md §4.2, DECISIONS.md P-4). Either a constant, or two or more points with strictly
/// increasing x. Beyond the first and last points it holds their values; between points it
/// interpolates linearly and rounds once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Curve(Shape);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Constant(Fixed),
    Points(Vec<Point>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: Fixed,
    y: Fixed,
}

/// Why some points don't make a curve, or a curve doesn't fit where it's used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurveError {
    TooFewPoints,
    XNotIncreasing {
        before: Fixed,
        after: Fixed,
    },
    ValueOutOfRange {
        value: Fixed,
        low: Fixed,
        high: Fixed,
    },
}

impl Curve {
    /// A flat curve: the same value everywhere.
    pub fn constant(y: Fixed) -> Curve {
        Curve(Shape::Constant(y))
    }

    /// A curve through `points`, given as `(x, y)` with strictly increasing x.
    pub fn from_points(points: Vec<(Fixed, Fixed)>) -> Result<Curve, CurveError> {
        if points.len() < 2 {
            return Err(CurveError::TooFewPoints);
        }
        if let Some(pair) = points.windows(2).find(|pair| pair[1].0 <= pair[0].0) {
            return Err(CurveError::XNotIncreasing {
                before: pair[0].0,
                after: pair[1].0,
            });
        }
        let points = points.into_iter().map(|(x, y)| Point { x, y }).collect();
        Ok(Curve(Shape::Points(points)))
    }

    /// The curve's exact value at `x`, unrounded, for a computation that multiplies it by
    /// other values and rounds once at the end (P-1).
    pub fn exact_at(&self, x: Fixed) -> Ratio {
        match &self.0 {
            Shape::Constant(y) => Ratio::from_fixed(*y),
            Shape::Points(points) => {
                // Clamping holds the end values beyond the first and last points.
                let x = x.clamp(points[0].x, points[points.len() - 1].x);
                let segment = points
                    .windows(2)
                    .find(|pair| x <= pair[1].x)
                    .expect("a clamped x lies on one of the segments");
                interpolate(segment[0], segment[1], x)
            }
        }
    }

    /// Its points as `(x, y)`, in order; none for a constant.
    pub fn points(&self) -> Vec<(Fixed, Fixed)> {
        match &self.0 {
            Shape::Constant(_) => Vec::new(),
            Shape::Points(points) => points.iter().map(|point| (point.x, point.y)).collect(),
        }
    }

    /// The curve's value at `x`, rounded once.
    pub fn at(&self, x: Fixed) -> Fixed {
        self.exact_at(x)
            .round()
            .expect("a curve's value lies between two of its points' values")
    }

    /// The lowest value anywhere on the curve: its lowest point, since between points the
    /// curve never leaves the range of its points.
    pub fn lowest(&self) -> Fixed {
        match &self.0 {
            Shape::Constant(y) => *y,
            Shape::Points(points) => points
                .iter()
                .map(|point| point.y)
                .min()
                .expect("a curve has at least two points"),
        }
    }

    /// Checks that every value on the curve lies within `low..=high`, for knobs whose values
    /// must stay in bounds, such as multipliers that can't be negative. Between points the
    /// curve never leaves the range of its points, so checking the points is enough.
    pub fn check_y_within(&self, low: Fixed, high: Fixed) -> Result<(), CurveError> {
        let ys: Vec<Fixed> = match &self.0 {
            Shape::Constant(y) => vec![*y],
            Shape::Points(points) => points.iter().map(|point| point.y).collect(),
        };
        match ys.into_iter().find(|y| *y < low || *y > high) {
            Some(value) => Err(CurveError::ValueOutOfRange { value, low, high }),
            None => Ok(()),
        }
    }
}

/// The value at `x` on the straight line from `start` to `end`, exactly: in hundredths,
/// `(y0·(x1 − x0) + (y1 − y0)·(x − x0)) / (x1 − x0)`.
fn interpolate(start: Point, end: Point, x: Fixed) -> Ratio {
    let [x0, y0, x1, y1, x] =
        [start.x, start.y, end.x, end.y, x].map(|value| i128::from(value.hundredths()));
    let span = x1 - x0;
    Ratio::new(y0 * span + (y1 - y0) * (x - x0), span * 100)
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurveError::TooFewPoints => {
                f.write_str("a curve is a single number or at least 2 points")
            }
            CurveError::XNotIncreasing { before, after } => {
                write!(
                    f,
                    "curve points must have increasing x: {before} then {after}"
                )
            }
            CurveError::ValueOutOfRange { value, low, high } => {
                write!(f, "curve value {value} is outside {low} to {high}")
            }
        }
    }
}

impl std::error::Error for CurveError {}

impl<'de> Deserialize<'de> for Curve {
    /// A number is a constant curve; a list of `[x, y]` pairs is a curve through those points.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Curve, D::Error> {
        deserializer.deserialize_any(CurveVisitor)
    }
}

struct CurveVisitor;

impl<'de> Visitor<'de> for CurveVisitor {
    type Value = Curve;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number, or a list of [x, y] points")
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Curve, E> {
        Fixed::deserialize(value.into_deserializer()).map(Curve::constant)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Curve, E> {
        Fixed::deserialize(value.into_deserializer()).map(Curve::constant)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Curve, A::Error> {
        let mut points = Vec::new();
        // Each point is read as a list and its length checked here: reading straight into a
        // pair would let TOML drop a third number without a word.
        while let Some(values) = seq.next_element::<Vec<Number>>()? {
            let values: Vec<Fixed> = values.into_iter().map(|Number(value)| value).collect();
            match values[..] {
                [x, y] => points.push((x, y)),
                _ => {
                    let shown: Vec<String> = values.iter().map(Fixed::to_string).collect();
                    return Err(de::Error::custom(format!(
                        "each curve point must be exactly [x, y], but one is [{}]",
                        shown.join(", ")
                    )));
                }
            }
        }
        Curve::from_points(points).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::{btree_set, vec};
    use proptest::prelude::*;

    /// `h(1250)` is 12.50.
    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    /// A curve through points given in hundredths.
    fn curve(points: &[(i64, i64)]) -> Curve {
        Curve::from_points(points.iter().map(|&(x, y)| (h(x), h(y))).collect())
            .expect("a valid curve")
    }

    /// `[[0, 1.00], [50, 0.70], [100, 0.30]]`, from PLAN.md F2.
    fn falloff() -> Curve {
        curve(&[(0, 100), (5000, 70), (10000, 30)])
    }

    #[test]
    fn a_curve_in_content_is_numbers_not_text() {
        #[derive(Debug, serde::Deserialize)]
        struct Doc {
            #[allow(dead_code)]
            c: Curve,
        }
        assert!(toml::from_str::<Doc>("c = [[0, 1.0], [10, 2.0]]").is_ok());
        assert!(toml::from_str::<Doc>("c = [[\"0\", \"1.0\"], [\"10\", \"2.0\"]]").is_err());
        assert!(toml::from_str::<Doc>("c = \"0.5\"").is_err());
    }

    #[test]
    fn lists_its_points_as_written_and_a_constant_has_none() {
        assert_eq!(
            falloff().points(),
            [(h(0), h(1_00)), (h(50_00), h(70)), (h(100_00), h(30))]
        );
        assert_eq!(Curve::constant(h(7)).points(), []);
    }

    #[test]
    fn the_lowest_value_is_the_lowest_point() {
        assert_eq!(falloff().lowest(), h(30));
        assert_eq!(curve(&[(0, -5), (100, 20), (200, -10)]).lowest(), h(-10));
        assert_eq!(Curve::constant(h(7)).lowest(), h(7));
    }

    #[test]
    fn exact_values_are_not_rounded_until_asked() {
        // DESIGN.md §5.3's hardening `toward_good`: 0.405 at 85, which `at` rounds to 0.41.
        let toward_good = curve(&[(-100_00, 50), (0, 1_00), (100_00, 30)]);
        assert_eq!(toward_good.exact_at(h(85_00)), Ratio::new(405, 1000));
        assert_eq!(toward_good.at(h(85_00)), h(41));
        assert_eq!(toward_good.exact_at(h(60_00)), Ratio::new(58, 100));
        // Beyond the ends it holds the end values, exactly.
        assert_eq!(toward_good.exact_at(h(-150_00)), Ratio::new(1, 2));
        assert_eq!(Curve::constant(h(1_25)).exact_at(h(7_00)), Ratio::new(5, 4));
        // A third of the way along a segment needs more than two decimals.
        let thirds = curve(&[(0, 0), (3_00, 1)]);
        assert_eq!(thirds.exact_at(h(1_00)), Ratio::new(1, 300));
    }

    /// The disposition affinity curve, `[[0, 50], [60, 0], [200, -50]]` (DESIGN.md §8.1).
    fn affinity() -> Curve {
        curve(&[(0, 5000), (6000, 0), (20000, -5000)])
    }

    fn points(curve: &Curve) -> Vec<Point> {
        match &curve.0 {
            Shape::Constant(_) => Vec::new(),
            Shape::Points(points) => points.clone(),
        }
    }

    #[test]
    fn interpolates_linearly_between_points() {
        assert_eq!(falloff().at(h(2500)), h(85));
        assert_eq!(falloff().at(h(7500)), h(50));
    }

    #[test]
    fn rounds_an_interpolated_value_once() {
        assert_eq!(falloff().at(h(3300)), h(80)); // 0.802 → 0.80
    }

    #[test]
    fn passes_through_its_points() {
        assert_eq!(falloff().at(h(0)), h(100));
        assert_eq!(falloff().at(h(5000)), h(70));
        assert_eq!(falloff().at(h(10000)), h(30));
    }

    #[test]
    fn holds_the_end_values_beyond_its_points() {
        assert_eq!(falloff().at(h(-1000)), h(100));
        assert_eq!(falloff().at(h(12000)), h(30));
    }

    #[test]
    fn gives_the_affinity_values_worked_out_in_the_design() {
        assert_eq!(affinity().at(h(7018)), h(-364));
        assert_eq!(affinity().at(h(8548)), h(-910));
        assert_eq!(affinity().at(h(559)), h(4534));
    }

    #[test]
    fn a_constant_curve_is_flat() {
        let flat = Curve::constant(h(100));
        for x in [-100_000, 0, 2500, 999_900] {
            assert_eq!(flat.at(h(x)), h(100));
        }
    }

    #[test]
    fn two_points_are_enough() {
        let line = curve(&[(0, 0), (10000, 10000)]);
        assert_eq!(line.at(h(4200)), h(4200));
    }

    #[test]
    fn rejects_points_whose_x_decreases() {
        let error = Curve::from_points(vec![(h(5000), h(100)), (h(4000), h(0))]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "curve points must have increasing x: 50.00 then 40.00"
        );
    }

    #[test]
    fn rejects_two_points_at_the_same_x() {
        let error = Curve::from_points(vec![(h(0), h(100)), (h(0), h(200))]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "curve points must have increasing x: 0.00 then 0.00"
        );
    }

    #[test]
    fn rejects_fewer_than_two_points() {
        for few in [vec![], vec![(h(0), h(100))]] {
            assert_eq!(
                Curve::from_points(few).unwrap_err().to_string(),
                "a curve is a single number or at least 2 points"
            );
        }
    }

    #[test]
    fn bounds_checks_include_their_ends() {
        assert_eq!(falloff().check_y_within(h(30), h(100)), Ok(()));
        assert_eq!(Curve::constant(h(0)).check_y_within(h(0), h(1000)), Ok(()));
    }

    #[test]
    fn bounds_checks_report_the_first_value_outside() {
        let dips = curve(&[(0, 50), (100, -50), (200, -80)]);
        assert_eq!(
            dips.check_y_within(h(0), h(1000)).unwrap_err().to_string(),
            "curve value -0.50 is outside 0.00 to 10.00"
        );
        assert_eq!(
            falloff()
                .check_y_within(h(0), h(50))
                .unwrap_err()
                .to_string(),
            "curve value 1.00 is outside 0.00 to 0.50"
        );
        assert_eq!(
            Curve::constant(h(-1))
                .check_y_within(h(0), h(100))
                .unwrap_err()
                .to_string(),
            "curve value -0.01 is outside 0.00 to 1.00"
        );
    }

    // TOML

    #[derive(Debug, serde::Deserialize)]
    struct Doc {
        c: Curve,
    }

    fn from_toml(text: &str) -> Result<Curve, String> {
        toml::from_str::<Doc>(text)
            .map(|doc| doc.c)
            .map_err(|error| error.message().to_owned())
    }

    #[test]
    fn reads_a_toml_number_as_a_constant_curve() {
        assert_eq!(from_toml("c = 1.0"), Ok(Curve::constant(h(100))));
        assert_eq!(from_toml("c = 2"), Ok(Curve::constant(h(200))));
    }

    #[test]
    fn reads_a_toml_list_of_points() {
        assert_eq!(
            from_toml("c = [[0, 1.00], [50, 0.70], [100, 0.30]]"),
            Ok(falloff())
        );
    }

    #[test]
    fn reports_invalid_toml_curves_in_a_designers_words() {
        assert_eq!(
            from_toml("c = [[50, 1.0], [40, 0.0]]"),
            Err("curve points must have increasing x: 50.00 then 40.00".into())
        );
        assert_eq!(
            from_toml("c = [[0, 1.0]]"),
            Err("a curve is a single number or at least 2 points".into())
        );
        assert_eq!(
            from_toml("c = [[0, 1.234], [1, 2]]"),
            Err("1.234 has more than 2 decimal places".into())
        );
    }

    #[test]
    fn rejects_a_toml_point_that_is_not_exactly_x_and_y() {
        assert_eq!(
            from_toml("c = [[0, 1.0], [5, 2, 7]]"),
            Err("each curve point must be exactly [x, y], but one is [5.00, 2.00, 7.00]".into())
        );
        assert_eq!(
            from_toml("c = [[0, 1.0], [5]]"),
            Err("each curve point must be exactly [x, y], but one is [5.00]".into())
        );
        assert_eq!(
            from_toml("c = [[0, 1.0], []]"),
            Err("each curve point must be exactly [x, y], but one is []".into())
        );
    }

    #[test]
    fn rejects_toml_that_is_neither_a_number_nor_points() {
        let error = from_toml("c = \"steep\"").unwrap_err();
        assert!(
            error.contains("expected a number, or a list of [x, y] points"),
            "{error}"
        );
    }

    // Properties

    /// 2 to 6 points, x strictly increasing within ±1,000.00, y within ±10,000.00.
    fn any_curve() -> impl Strategy<Value = Curve> {
        (
            btree_set(-100_000_i64..=100_000, 2..=6),
            vec(-1_000_000_i64..=1_000_000, 6),
        )
            .prop_map(|(xs, ys)| curve(&xs.into_iter().zip(ys).collect::<Vec<_>>()))
    }

    /// Like `any_curve`, but y never decreases from one point to the next.
    fn non_decreasing_curve() -> impl Strategy<Value = Curve> {
        (
            btree_set(-100_000_i64..=100_000, 2..=6),
            vec(-1_000_000_i64..=1_000_000, 6),
        )
            .prop_map(|(xs, mut ys)| {
                ys.sort_unstable();
                curve(&xs.into_iter().zip(ys).collect::<Vec<_>>())
            })
    }

    fn any_x() -> impl Strategy<Value = Fixed> {
        (-200_000_i64..=200_000).prop_map(h)
    }

    proptest! {
        #[test]
        fn stays_within_the_range_of_its_points(curve in any_curve(), x in any_x()) {
            let ys: Vec<Fixed> = points(&curve).iter().map(|point| point.y).collect();
            let y = curve.at(x);
            prop_assert!(ys.iter().min().is_some_and(|low| *low <= y));
            prop_assert!(ys.iter().max().is_some_and(|high| y <= *high));
        }

        #[test]
        fn never_decreases_when_its_points_never_decrease(
            curve in non_decreasing_curve(),
            a in any_x(),
            b in any_x(),
        ) {
            prop_assert!(curve.at(a.min(b)) <= curve.at(a.max(b)));
        }

        #[test]
        fn passes_through_every_point(curve in any_curve()) {
            for point in points(&curve) {
                prop_assert_eq!(curve.at(point.x), point.y);
            }
        }
    }
}

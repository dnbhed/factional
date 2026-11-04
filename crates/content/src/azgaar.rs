//! Reading Azgaar's map (W2a, P-119, P-134): a Full JSON export read into a `Source`. Its
//! version is checked first; then every cell, vertex and feature is read, every problem in
//! them reported at once by key path, and every reference kept checked to resolve. Numbers
//! with decimals are rounded once, as they're read, to hundredths, half away from zero.

use std::fmt;

use factional_core::Fixed;
use factional_worldgen::{Feature, Point, Size, Source, SourceCell, SourceVertex};
use serde_json::Value;

/// The generator's versions this build reads: its major and minor version, any patch.
const READS: (u64, u64) = (1, 154);

/// The lists a Full export has and the others lack, in the order they're checked.
const FULL: [&str; 3] = ["cells", "vertices", "features"];

/// One problem in an export, at its key path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzgaarProblem {
    pub path: String,
    pub what: String,
}

impl fmt::Display for AzgaarProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.what)
    }
}

/// Why an export can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AzgaarError {
    NotJson(String),
    /// JSON with no `info.version`.
    NotAzgaar,
    /// Another version of the generator's, as the export names it.
    Version(String),
    /// A Minimal or Cells export, or anything else without one of the Full export's lists.
    NotFull(&'static str),
    /// Every problem in the export's cells, vertices and features, in the order they're read.
    Problems(Vec<AzgaarProblem>),
}

impl fmt::Display for AzgaarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AzgaarError::NotJson(why) => write!(f, "this isn't JSON: {why}"),
            AzgaarError::NotAzgaar => {
                f.write_str("this isn't an Azgaar map: it has no info.version")
            }
            AzgaarError::Version(found) => write!(
                f,
                "info.version: this map is from Azgaar {found}, and Factional reads {}.{}",
                READS.0, READS.1
            ),
            AzgaarError::NotFull(list) => write!(
                f,
                "this isn't a Full export: it has no pack.{list}; export the Full JSON from Azgaar"
            ),
            AzgaarError::Problems(problems) => {
                let lines: Vec<String> = problems.iter().map(ToString::to_string).collect();
                f.write_str(&lines.join("\n"))
            }
        }
    }
}

impl std::error::Error for AzgaarError {}

/// The source map in `text`, an Azgaar Full JSON export, or why it can't be read.
pub fn read_azgaar(text: &str) -> Result<Source, AzgaarError> {
    let json: Value =
        serde_json::from_str(text).map_err(|error| AzgaarError::NotJson(error.to_string()))?;
    let version = json
        .pointer("/info/version")
        .and_then(Value::as_str)
        .ok_or(AzgaarError::NotAzgaar)?;
    if !reads(version) {
        return Err(AzgaarError::Version(version.to_owned()));
    }
    let lists = FULL.map(|list| {
        json.pointer(&format!("/pack/{list}"))
            .ok_or(AzgaarError::NotFull(list))
    });
    let [cells, vertices, features] = match lists {
        [Ok(cells), Ok(vertices), Ok(features)] => [cells, vertices, features],
        _ => {
            return Err(lists
                .into_iter()
                .find_map(Result::err)
                .expect("one is missing"));
        }
    };
    let mut reader = Reader::default();
    let info = &json["info"];
    let name = reader
        .field(info, "info", "mapName")
        .and_then(|name| reader.text(name, "info.mapName"));
    let mut side = |key: &str| {
        let path = format!("info.{key}");
        reader
            .field(info, "info", key)
            .and_then(|pixels| reader.whole(pixels, &path))
            .and_then(|pixels| match u32::try_from(pixels) {
                Ok(pixels) if pixels > 0 => Some(pixels),
                _ => reader.problem(&path, format!("expected at least 1 pixel, not {pixels}")),
            })
    };
    let (width, height) = (side("width"), side("height"));
    let cells = reader.list(cells, "pack.cells");
    let vertices = reader.list(vertices, "pack.vertices");
    let features = reader.list(features, "pack.features");
    let counts = Counts {
        cells: cells.len(),
        vertices: vertices.len(),
        // Azgaar's `features[0]` is a placeholder, so feature 1 is the first.
        features: features.len().saturating_sub(1),
    };
    let cells: Vec<Option<SourceCell>> = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| reader.cell(cell, &format!("pack.cells[{i}]"), &counts))
        .collect();
    let vertices: Vec<Option<SourceVertex>> = vertices
        .iter()
        .enumerate()
        .map(|(i, vertex)| reader.vertex(vertex, &format!("pack.vertices[{i}]"), &counts))
        .collect();
    let features: Vec<Option<Feature>> = features
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, feature)| reader.feature(feature, &format!("pack.features[{i}]")))
        .collect();
    if !reader.problems.is_empty() {
        return Err(AzgaarError::Problems(reader.problems));
    }
    let every = "with no problems, everything was read";
    Ok(Source {
        name: name.expect(every),
        version: version.to_owned(),
        size: Size {
            width: width.expect(every),
            height: height.expect(every),
        },
        cells: cells.into_iter().map(|cell| cell.expect(every)).collect(),
        vertices: vertices
            .into_iter()
            .map(|vertex| vertex.expect(every))
            .collect(),
        features: features
            .into_iter()
            .map(|feature| feature.expect(every))
            .collect(),
    })
}

/// Whether this build reads exports from the generator's `version`.
fn reads(version: &str) -> bool {
    let mut parts = version.split('.').map(str::parse::<u64>);
    matches!(
        (parts.next(), parts.next()),
        (Some(Ok(major)), Some(Ok(minor))) if (major, minor) == READS
    )
}

/// How many cells, vertices and features the export has, so references can be checked.
struct Counts {
    cells: usize,
    vertices: usize,
    features: usize,
}

/// Reads values, keeping every problem it finds.
#[derive(Default)]
struct Reader {
    problems: Vec<AzgaarProblem>,
}

impl Reader {
    /// Notes `what` is wrong at `path`; nothing is read there.
    fn problem<T>(&mut self, path: &str, what: String) -> Option<T> {
        self.problems.push(AzgaarProblem {
            path: path.to_owned(),
            what,
        });
        None
    }

    /// `object`'s `key`, or a problem at `path` saying it's missing.
    fn field<'a>(&mut self, object: &'a Value, path: &str, key: &str) -> Option<&'a Value> {
        match object.get(key) {
            Some(value) => Some(value),
            None => self.problem(path, format!("missing {key}")),
        }
    }

    /// The items of the list `value`, none if it isn't one.
    fn list<'a>(&mut self, value: &'a Value, path: &str) -> &'a [Value] {
        match value.as_array() {
            Some(items) => items,
            None => {
                self.problem::<()>(path, "expected a list".to_owned());
                &[]
            }
        }
    }

    fn text(&mut self, value: &Value, path: &str) -> Option<String> {
        match value.as_str() {
            Some(text) => Some(text.to_owned()),
            None => self.problem(path, format!("expected text, not {value}")),
        }
    }

    fn whole(&mut self, value: &Value, path: &str) -> Option<u64> {
        match value.as_u64() {
            Some(whole) => Some(whole),
            None => self.problem(path, format!("expected a whole number, not {value}")),
        }
    }

    /// The list of whole numbers `value`.
    fn wholes(&mut self, value: &Value, path: &str) -> Option<Vec<u64>> {
        let Some(items) = value.as_array() else {
            return self.problem(path, format!("expected whole numbers, not {value}"));
        };
        match items.iter().find(|item| item.as_u64().is_none()) {
            Some(item) => self.problem(path, format!("expected whole numbers, not {item}")),
            None => Some(items.iter().filter_map(Value::as_u64).collect()),
        }
    }

    /// The list `value` of references to a `noun`, each one of the `count` `plural` there are.
    fn references(
        &mut self,
        value: &Value,
        path: &str,
        (noun, count, plural): (&str, usize, &str),
    ) -> Option<Vec<u32>> {
        let numbers = self.wholes(value, path)?;
        match numbers.iter().find(|number| **number >= count as u64) {
            Some(number) => self.problem(
                path,
                format!("{noun} {number} doesn't exist; there are {count} {plural}"),
            ),
            None => Some(numbers.into_iter().map(|number| number as u32).collect()),
        }
    }

    /// The number `value`, rounded once to hundredths.
    fn decimal(&mut self, value: &Value, path: &str) -> Option<Fixed> {
        match value.as_number().and_then(hundredths) {
            Some(hundredths) => Some(Fixed::from_hundredths(hundredths)),
            None => self.problem(path, format!("expected a number, not {value}")),
        }
    }

    /// The point `value`, an x and a y.
    fn point(&mut self, value: &Value, path: &str) -> Option<Point> {
        let numbers: Option<Vec<i64>> =
            value.as_array().filter(|xy| xy.len() == 2).and_then(|xy| {
                xy.iter()
                    .map(|n| n.as_number().and_then(hundredths))
                    .collect()
            });
        match numbers.as_deref() {
            Some(&[x, y]) => Some(Point {
                x: Fixed::from_hundredths(x),
                y: Fixed::from_hundredths(y),
            }),
            _ => self.problem(path, "expected two numbers, an x and a y".to_owned()),
        }
    }

    fn cell(&mut self, cell: &Value, path: &str, counts: &Counts) -> Option<SourceCell> {
        let field = |reader: &mut Reader, key: &str| reader.field(cell, path, key);
        let at = |key: &str| format!("{path}.{key}");
        let centre = field(self, "p").and_then(|p| self.point(p, &at("p")));
        let height = field(self, "h")
            .and_then(|h| self.whole(h, &at("h")))
            .and_then(|h| match u8::try_from(h) {
                Ok(h) if h <= 100 => Some(h),
                _ => self.problem(&at("h"), format!("{h} is outside 0 to 100")),
            });
        let feature = field(self, "f")
            .and_then(|f| self.whole(f, &at("f")))
            .and_then(|f| match f {
                1.. if f <= counts.features as u64 => Some(f as u32 - 1),
                _ => self.problem(
                    &at("f"),
                    format!(
                        "feature {f} doesn't exist; there are {} features",
                        counts.features
                    ),
                ),
            });
        let neighbours = field(self, "c")
            .and_then(|c| self.references(c, &at("c"), ("neighbour", counts.cells, "cells")));
        let corners = field(self, "v")
            .and_then(|v| self.references(v, &at("v"), ("corner", counts.vertices, "vertices")));
        Some(SourceCell {
            centre: centre?,
            height: height?,
            feature: feature?,
            neighbours: neighbours?,
            corners: corners?,
        })
    }

    fn vertex(&mut self, vertex: &Value, path: &str, counts: &Counts) -> Option<SourceVertex> {
        let at = |key: &str| format!("{path}.{key}");
        let point = self
            .field(vertex, path, "p")
            .and_then(|p| self.point(p, &at("p")));
        let cells = self.field(vertex, path, "c").and_then(|c| {
            // A cell past the last is one of the generator's points beyond the map's edge.
            let on = |cell: u64| (cell < counts.cells as u64).then_some(cell as u32);
            match self.wholes(c, &at("c"))?[..] {
                [a, b, c] => Some([on(a), on(b), on(c)]),
                _ => self.problem(&at("c"), "expected three cells".to_owned()),
            }
        });
        Some(SourceVertex {
            at: point?,
            cells: cells?,
        })
    }

    fn feature(&mut self, feature: &Value, path: &str) -> Option<Feature> {
        let kind = self.field(feature, path, "type")?;
        match kind.as_str() {
            Some("ocean") => Some(Feature::Ocean),
            Some("island") => Some(Feature::Island),
            Some("lake") => match feature.get("height") {
                Some(height) => self
                    .decimal(height, &format!("{path}.height"))
                    .map(|surface| Feature::Lake { surface }),
                None => self.problem(path, "missing height, a lake's surface".to_owned()),
            },
            _ => self.problem(
                &format!("{path}.type"),
                format!("{kind} isn't a feature Factional reads: ocean, lake or island"),
            ),
        }
    }
}

/// `number` in hundredths, rounded once, half away from zero; `None` if it's too large.
/// A number with decimals is read from its shortest decimal form, the digits the export
/// wrote, so no binary rounding creeps in.
fn hundredths(number: &serde_json::Number) -> Option<i64> {
    if let Some(whole) = number.as_i64() {
        return whole.checked_mul(100);
    }
    let text = format!("{}", number.as_f64()?);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text.as_str()),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let mut fraction = fraction
        .bytes()
        .map(|digit| i64::from(digit - b'0'))
        .chain([0; 3]);
    let mut next = || fraction.next().expect("padded with zeros");
    let (tenths, hundredths, thousandths) = (next(), next(), next());
    let magnitude = whole
        .parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(tenths * 10 + hundredths + i64::from(thousandths >= 5))?;
    Some(if negative { -magnitude } else { magnitude })
}

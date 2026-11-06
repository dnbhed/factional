//! Reading Azgaar's map (W2a, P-119, P-134): a Full JSON export read into a `Source`. Its
//! version is checked first; then every cell, vertex, feature and river (W3d, P-139) is read,
//! and every grid cell's climate (W4a, P-141), every problem in them reported at once by key
//! path, and every reference kept checked to resolve. Numbers
//! with decimals are rounded once, as they're read, to hundredths, half away from zero.

use std::collections::BTreeMap;
use std::fmt;

use factional_core::{Fixed, div_round};
use factional_worldgen::{
    Biome, Buildings, Feature, HEIGHT_EXPONENTS, Point, RouteKind, SettlementKind, Size, Source,
    SourceCell, SourceMarker, SourceRiver, SourceRoute, SourceSettlement, SourceVertex,
};
use serde_json::Value;

/// The generator's versions this build reads: its major and minor version, any patch.
const READS: (u64, u64) = (1, 154);

/// The lists a Full export has and the others lack, in the order they're checked.
const FULL: [&str; 8] = [
    "pack.cells",
    "pack.vertices",
    "pack.features",
    "pack.rivers",
    "pack.burgs",
    "pack.routes",
    "pack.markers",
    "grid.cells",
];

/// Azgaar's settlement types, by the name it gives each.
const SETTLEMENT_KINDS: [(&str, SettlementKind); 7] = [
    ("Generic", SettlementKind::Generic),
    ("Naval", SettlementKind::Naval),
    ("Lake", SettlementKind::Lake),
    ("Highland", SettlementKind::Highland),
    ("River", SettlementKind::River),
    ("Nomadic", SettlementKind::Nomadic),
    ("Hunting", SettlementKind::Hunting),
];

/// Azgaar's route groups, by the name it gives each.
const ROUTE_KINDS: [(&str, RouteKind); 3] = [
    ("roads", RouteKind::Road),
    ("trails", RouteKind::Trail),
    ("searoutes", RouteKind::SeaRoute),
];

/// Azgaar's 13 biomes, each at its number.
const BIOMES: [Biome; 13] = [
    Biome::Marine,
    Biome::HotDesert,
    Biome::ColdDesert,
    Biome::Savanna,
    Biome::Grassland,
    Biome::TropicalSeasonalForest,
    Biome::TemperateDeciduousForest,
    Biome::TropicalRainforest,
    Biome::TemperateRainforest,
    Biome::Taiga,
    Biome::Tundra,
    Biome::Glacier,
    Biome::Wetland,
];

/// Azgaar's precipitation is in units of 100 mm.
const MM_A_UNIT: u16 = 100;

/// The widths of a river Factional reads, in pixels: its width factor and its source width.
const RIVER_WIDTHS: [Fixed; 2] = [Fixed::ZERO, Fixed::from_hundredths(10_00)];

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
    /// A Minimal or Cells export, or anything else without one of the Full export's lists,
    /// named by its path.
    NotFull(&'static str),
    /// Every problem in the export's cells, vertices, features and rivers, in the order they're
    /// read.
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
                "this isn't a Full export: it has no {list}; export the Full JSON from Azgaar"
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
        json.pointer(&format!("/{}", list.replace('.', "/")))
            .ok_or(AzgaarError::NotFull(list))
    });
    let [
        cells,
        vertices,
        features,
        rivers,
        burgs,
        routes,
        markers,
        grid,
    ] = match lists {
        [
            Ok(cells),
            Ok(vertices),
            Ok(features),
            Ok(rivers),
            Ok(burgs),
            Ok(routes),
            Ok(markers),
            Ok(grid),
        ] => [
            cells, vertices, features, rivers, burgs, routes, markers, grid,
        ],
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
    let exponent = reader.height_exponent(&json);
    let rate = reader.population_rate(&json);
    let cells = reader.list(cells, "pack.cells");
    let vertices = reader.list(vertices, "pack.vertices");
    let features = reader.list(features, "pack.features");
    let rivers = reader.list(rivers, "pack.rivers");
    let grid = reader.list(grid, "grid.cells");
    let counts = Counts {
        cells: cells.len(),
        grid: grid.len(),
        vertices: vertices.len(),
        // Azgaar's `features[0]` is a placeholder, so feature 1 is the first.
        features: features.len().saturating_sub(1),
    };
    let cells: Vec<Option<(SourceCell, usize)>> = cells
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
    let rivers: Vec<Option<SourceRiver>> = rivers
        .iter()
        .enumerate()
        .map(|(i, river)| reader.river(river, &format!("pack.rivers[{i}]"), &counts))
        .collect();
    let climates: Vec<Option<(i8, u16)>> = grid
        .iter()
        .enumerate()
        .map(|(i, climate)| reader.climate(climate, &format!("grid.cells[{i}]")))
        .collect();
    // Burgs[0] is Azgaar's placeholder, and a removed burg is gone.
    let burgs = reader.list(burgs, "pack.burgs");
    let settlements: Vec<(usize, Option<(SourceSettlement, i64)>)> = burgs
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, burg)| burg.get("removed") != Some(&Value::Bool(true)))
        .map(|(i, burg)| {
            (
                i,
                reader.settlement(burg, &format!("pack.burgs[{i}]"), &counts),
            )
        })
        .collect();
    reader.unique(
        "pack.burgs",
        settlements
            .iter()
            .map(|(i, s)| (*i, s.as_ref().map(|s| s.0.id))),
    );
    let routes = reader.list(routes, "pack.routes");
    let routes: Vec<Option<SourceRoute>> = routes
        .iter()
        .enumerate()
        .map(|(i, route)| reader.route(route, &format!("pack.routes[{i}]"), &counts))
        .collect();
    reader.unique(
        "pack.routes",
        routes
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.as_ref().map(|r| r.id))),
    );
    let markers = reader.list(markers, "pack.markers");
    let markers: Vec<Option<SourceMarker>> = markers
        .iter()
        .enumerate()
        .map(|(i, marker)| reader.marker(marker, &format!("pack.markers[{i}]"), &counts))
        .collect();
    reader.unique(
        "pack.markers",
        markers
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.as_ref().map(|m| m.id))),
    );
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
        height_exponent: exponent.expect(every),
        // Each cell with the climate of its grid cell.
        cells: cells
            .into_iter()
            .map(|cell| {
                let (cell, grid) = cell.expect(every);
                let (temperature, precipitation) = climates[grid].expect(every);
                SourceCell {
                    temperature,
                    precipitation,
                    ..cell
                }
            })
            .collect(),
        vertices: vertices
            .into_iter()
            .map(|vertex| vertex.expect(every))
            .collect(),
        features: features
            .into_iter()
            .map(|feature| feature.expect(every))
            .collect(),
        rivers: rivers
            .into_iter()
            .map(|river| river.expect(every))
            .collect(),
        // Each settlement's people: its population in thousandths, times the rate, times
        // urbanization in hundredths, rounded once.
        settlements: settlements
            .into_iter()
            .map(|(_, settlement)| {
                let (settlement, thousandths) = settlement.expect(every);
                let (rate, urbanization) = rate.expect(every);
                let people = div_round(
                    i128::from(thousandths) * i128::from(rate) * i128::from(urbanization),
                    100_000,
                );
                SourceSettlement {
                    people: u64::try_from(people).expect("0 or more"),
                    ..settlement
                }
            })
            .collect(),
        routes: routes
            .into_iter()
            .map(|route| route.expect(every))
            .collect(),
        markers: markers
            .into_iter()
            .map(|marker| marker.expect(every))
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
    grid: usize,
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

    /// The whole number `value`, which may be below zero.
    fn integer(&mut self, value: &Value, path: &str) -> Option<i64> {
        match value.as_i64() {
            Some(integer) => Some(integer),
            None => self.problem(path, format!("expected a whole number, not {value}")),
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

    /// The exponent the export's heights are raised to in metres, `settings.heightExponent`,
    /// within the range Azgaar allows.
    fn height_exponent(&mut self, json: &Value) -> Option<Fixed> {
        let path = "settings.heightExponent";
        let Some(value) = json.pointer("/settings/heightExponent") else {
            return self.problem("settings", "missing heightExponent".to_owned());
        };
        let exponent = self.decimal(value, path)?;
        let [lowest, highest] = HEIGHT_EXPONENTS;
        match lowest <= exponent && exponent <= highest {
            true => Some(exponent),
            false => self.problem(
                path,
                format!("{exponent} is outside {lowest} to {highest}, as Azgaar allows"),
            ),
        }
    }

    /// The people a point of population stands for, `settings.populationRate`, and
    /// urbanization in hundredths, `settings.urbanization`.
    fn population_rate(&mut self, json: &Value) -> Option<(u64, i64)> {
        let settings = json.get("settings").unwrap_or(&Value::Null);
        let rate = self
            .field(settings, "settings", "populationRate")
            .and_then(|rate| self.whole(rate, "settings.populationRate"))
            .and_then(|rate| match rate {
                0 => self.problem(
                    "settings.populationRate",
                    "expected at least 1 person a point, not 0".to_owned(),
                ),
                rate => Some(rate),
            });
        let urbanization = self
            .field(settings, "settings", "urbanization")
            .and_then(|urbanization| self.decimal(urbanization, "settings.urbanization"))
            .and_then(|urbanization| match urbanization < Fixed::ZERO {
                true => self.problem(
                    "settings.urbanization",
                    format!("expected 0 or more, not {urbanization}"),
                ),
                false => Some(urbanization.hundredths()),
            });
        Some((rate?, urbanization?))
    }

    /// The reference `value` to a cell, one of the `count` there are.
    fn cell_of(&mut self, value: &Value, path: &str, count: usize) -> Option<u32> {
        match self.whole(value, path)? {
            cell if cell < count as u64 => Some(cell as u32),
            cell => self.problem(
                path,
                format!("cell {cell} doesn't exist; there are {count} cells"),
            ),
        }
    }

    /// An id, `object`'s `i`.
    fn id(&mut self, object: &Value, path: &str) -> Option<u32> {
        let at = format!("{path}.i");
        let id = self.field(object, path, "i")?;
        match self.whole(id, &at).map(u32::try_from)? {
            Ok(id) => Some(id),
            Err(_) => self.problem(&at, format!("{id} is too large")),
        }
    }

    /// Notes each id in `ids`, by the place in `list` it's at, that an earlier one has.
    fn unique(&mut self, list: &str, ids: impl Iterator<Item = (usize, Option<u32>)>) {
        let mut first = BTreeMap::new();
        for (i, id) in ids {
            let Some(id) = id else { continue };
            match first.get(&id) {
                Some(earlier) => {
                    let what = format!("{id} is also {list}[{earlier}]'s");
                    self.problem::<()>(&format!("{list}[{i}].i"), what);
                }
                None => {
                    first.insert(id, i);
                }
            }
        }
    }

    /// `object`'s text at `key`, if it has any: none when it's missing or empty.
    fn optional_text(&mut self, object: &Value, path: &str, key: &str) -> Option<Option<String>> {
        match object.get(key) {
            None => Some(None),
            Some(value) => {
                let text = self.text(value, &format!("{path}.{key}"))?;
                Some(Some(text).filter(|text| !text.is_empty()))
            }
        }
    }

    /// `object`'s `key`, 0 or 1, as no or yes.
    fn flag(&mut self, object: &Value, path: &str, key: &str) -> Option<bool> {
        let value = self.field(object, path, key)?;
        match value.as_u64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => self.problem(
                &format!("{path}.{key}"),
                format!("expected 0 or 1, not {value}"),
            ),
        }
    }

    /// The point at `object`'s `x` and `y`.
    fn place(&mut self, object: &Value, path: &str) -> Option<Point> {
        let mut coordinate = |key: &str| {
            let value = self.field(object, path, key)?;
            self.decimal(value, &format!("{path}.{key}"))
        };
        let (x, y) = (coordinate("x"), coordinate("y"));
        Some(Point { x: x?, y: y? })
    }

    /// The settlement `burg`, its people yet to come from the settings, and its population in
    /// thousandths.
    fn settlement(
        &mut self,
        burg: &Value,
        path: &str,
        counts: &Counts,
    ) -> Option<(SourceSettlement, i64)> {
        let at = |key: &str| format!("{path}.{key}");
        let id = self.id(burg, path);
        let name = self
            .field(burg, path, "name")
            .and_then(|name| self.text(name, &at("name")));
        let point = self.place(burg, path);
        let cell = self
            .field(burg, path, "cell")
            .and_then(|cell| self.cell_of(cell, &at("cell"), counts.cells));
        let population =
            self.field(burg, path, "population").and_then(|population| {
                match population.as_number().and_then(|n| scaled(n, 3)) {
                    Some(thousandths) if thousandths >= 0 => Some(thousandths),
                    Some(_) => self.problem(
                        &at("population"),
                        format!("expected 0 or more, not {population}"),
                    ),
                    None => self.problem(
                        &at("population"),
                        format!("expected a number, not {population}"),
                    ),
                }
            });
        let kind = self.field(burg, path, "type").and_then(|kind| {
            match SETTLEMENT_KINDS
                .iter()
                .find(|(name, _)| kind.as_str() == Some(name))
            {
                Some(&(_, kind)) => Some(kind),
                None => self.problem(
                    &at("type"),
                    format!(
                        "{kind} isn't a settlement type Factional reads: Generic, Naval, Lake, \
                         Highland, River, Nomadic or Hunting"
                    ),
                ),
            }
        });
        let capital = self.flag(burg, path, "capital");
        // A port is the water feature its harbour is on; none, or 0, is no port.
        let port = match burg.get("port") {
            None => Some(false),
            Some(port) => match self.whole(port, &at("port"))? {
                0 => Some(false),
                feature if feature <= counts.features as u64 => Some(true),
                feature => self.problem(
                    &at("port"),
                    format!(
                        "feature {feature} doesn't exist; there are {} features",
                        counts.features
                    ),
                ),
            },
        };
        let mut building = |key: &str| self.flag(burg, path, key);
        let (citadel, walls, plaza, temple, shanty) = (
            building("citadel"),
            building("walls"),
            building("plaza"),
            building("temple"),
            building("shanty"),
        );
        let settlement = SourceSettlement {
            id: id?,
            name: name?,
            at: point?,
            cell: cell?,
            people: 0,
            kind: kind?,
            capital: capital?,
            port: port?,
            buildings: Buildings {
                citadel: citadel?,
                walls: walls?,
                plaza: plaza?,
                temple: temple?,
                shanty: shanty?,
            },
        };
        Some((settlement, population?))
    }

    /// The route `route`: its points at least two, each an x, a y and a cell.
    fn route(&mut self, route: &Value, path: &str, counts: &Counts) -> Option<SourceRoute> {
        let at = |key: &str| format!("{path}.{key}");
        let id = self.id(route, path);
        let kind =
            self.field(route, path, "group").and_then(|group| {
                match ROUTE_KINDS.iter().find(|(name, _)| group.as_str() == Some(name)) {
                Some(&(_, kind)) => Some(kind),
                None => self.problem(
                    &at("group"),
                    format!(
                        "{group} isn't a route group Factional reads: roads, trails or searoutes"
                    ),
                ),
            }
            });
        let name = self.optional_text(route, path, "name");
        let points = self.field(route, path, "points").and_then(|points| {
            let points = self.list(points, &at("points"));
            let read: Vec<Option<Point>> = points
                .iter()
                .enumerate()
                .map(|(i, point)| {
                    let here = format!("{path}.points[{i}]");
                    match point.as_array().map(Vec::as_slice) {
                        Some([x, y, cell]) => {
                            let cell = self.cell_of(cell, &here, counts.cells);
                            let x = x.as_number().and_then(hundredths);
                            let y = y.as_number().and_then(hundredths);
                            match (x, y) {
                                (Some(x), Some(y)) => cell.map(|_| Point {
                                    x: Fixed::from_hundredths(x),
                                    y: Fixed::from_hundredths(y),
                                }),
                                _ => {
                                    self.problem(&here, "expected an x, a y and a cell".to_owned())
                                }
                            }
                        }
                        _ => self.problem(&here, "expected an x, a y and a cell".to_owned()),
                    }
                })
                .collect();
            if read.len() < 2 {
                return self.problem(
                    &at("points"),
                    format!("a route needs at least two points, not {}", read.len()),
                );
            }
            read.into_iter().collect::<Option<Vec<Point>>>()
        });
        Some(SourceRoute {
            id: id?,
            name: name?,
            kind: kind?,
            points: points?,
        })
    }

    /// The point of interest `marker`.
    fn marker(&mut self, marker: &Value, path: &str, counts: &Counts) -> Option<SourceMarker> {
        let at = |key: &str| format!("{path}.{key}");
        let id = self.id(marker, path);
        let kind = self
            .field(marker, path, "type")
            .and_then(|kind| self.text(kind, &at("type")));
        let icon = self
            .field(marker, path, "icon")
            .and_then(|icon| self.text(icon, &at("icon")));
        let name = self.optional_text(marker, path, "name");
        let note = self.optional_text(marker, path, "note");
        let point = self.place(marker, path);
        let cell = self
            .field(marker, path, "cell")
            .and_then(|cell| self.cell_of(cell, &at("cell"), counts.cells));
        Some(SourceMarker {
            id: id?,
            kind: kind?,
            icon: icon?,
            name: name?,
            note: note?,
            at: point?,
            cell: cell?,
        })
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

    /// The cell `cell`, its climate yet to come from its grid cell, and that grid cell.
    fn cell(&mut self, cell: &Value, path: &str, counts: &Counts) -> Option<(SourceCell, usize)> {
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
        let flux = field(self, "fl")
            .and_then(|fl| self.whole(fl, &at("fl")))
            .and_then(|fl| match u32::try_from(fl) {
                Ok(fl) => Some(fl),
                Err(_) => self.problem(
                    &at("fl"),
                    format!("{fl} is more flux than Factional reads, {}", u32::MAX),
                ),
            });
        let biome = field(self, "biome")
            .and_then(|biome| self.whole(biome, &at("biome")))
            .and_then(
                |biome| match usize::try_from(biome).ok().and_then(|i| BIOMES.get(i)) {
                    Some(&known) => Some(known),
                    None => self.problem(
                        &at("biome"),
                        format!(
                            "biome {biome} isn't one of Azgaar's 13, 0 to 12, which Factional reads"
                        ),
                    ),
                },
            );
        let grid = field(self, "g")
            .and_then(|g| self.whole(g, &at("g")))
            .and_then(|g| match g < counts.grid as u64 {
                true => Some(g as usize),
                false => self.problem(
                    &at("g"),
                    format!(
                        "grid cell {g} doesn't exist; there are {} grid cells",
                        counts.grid
                    ),
                ),
            });
        Some((
            SourceCell {
                centre: centre?,
                height: height?,
                feature: feature?,
                neighbours: neighbours?,
                corners: corners?,
                flux: flux?,
                biome: biome?,
                temperature: 0,
                precipitation: 0,
            },
            grid?,
        ))
    }

    /// A grid cell's climate: its temperature in whole °C, and its precipitation, Azgaar's
    /// figure in units of 100 mm, in mm.
    fn climate(&mut self, climate: &Value, path: &str) -> Option<(i8, u16)> {
        let field = |reader: &mut Reader, key: &str| reader.field(climate, path, key);
        let at = |key: &str| format!("{path}.{key}");
        let temperature = field(self, "temp")
            .and_then(|temp| self.integer(temp, &at("temp")))
            .and_then(|temp| match i8::try_from(temp) {
                Ok(temp) => Some(temp),
                Err(_) => self.problem(
                    &at("temp"),
                    format!("{temp} is outside {} to {} °C", i8::MIN, i8::MAX),
                ),
            });
        let precipitation = field(self, "prec")
            .and_then(|prec| self.whole(prec, &at("prec")))
            .and_then(|prec| match u8::try_from(prec) {
                Ok(prec) => Some(u16::from(prec) * MM_A_UNIT),
                Err(_) => self.problem(&at("prec"), format!("{prec} is outside 0 to 255")),
            });
        Some((temperature?, precipitation?))
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

    fn river(&mut self, river: &Value, path: &str, counts: &Counts) -> Option<SourceRiver> {
        let at = |key: &str| format!("{path}.{key}");
        let cells = self
            .field(river, path, "cells")
            .and_then(|cells| self.river_cells(cells, &at("cells"), counts));
        let width = |reader: &mut Reader, key: &str| {
            let width = reader
                .field(river, path, key)
                .and_then(|width| reader.decimal(width, &at(key)))?;
            let [lowest, highest] = RIVER_WIDTHS;
            match lowest <= width && width <= highest {
                true => Some(width),
                false => reader.problem(
                    &at(key),
                    format!("{width} is outside {lowest} to {highest}"),
                ),
            }
        };
        let width_factor = width(self, "widthFactor");
        let source_width = width(self, "sourceWidth");
        let (cells, off_map) = cells?;
        Some(SourceRiver {
            cells,
            off_map,
            width_factor: width_factor?,
            source_width: source_width?,
        })
    }

    /// A river's cells from its source, `value`, each one of the export's, the last perhaps
    /// -1, past the map's edge; and whether it is.
    fn river_cells(
        &mut self,
        value: &Value,
        path: &str,
        counts: &Counts,
    ) -> Option<(Vec<u32>, bool)> {
        let numbers: Option<Vec<i64>> = value.as_array().and_then(|items| {
            items
                .iter()
                .map(|item| item.as_i64().filter(|&number| number >= -1))
                .collect()
        });
        let Some(numbers) = numbers else {
            return self.problem(
                path,
                format!("expected cells, whole numbers or -1 at the end, not {value}"),
            );
        };
        let (off_map, cells) = match numbers.split_last() {
            Some((-1, cells)) => (true, cells),
            _ => (false, &numbers[..]),
        };
        if cells.contains(&-1) {
            return self.problem(path, "-1, off the map, can only end a river".to_owned());
        }
        if numbers.len() < 2 {
            return self.problem(
                path,
                format!(
                    "a river needs at least two cells, -1 included, not {}",
                    numbers.len()
                ),
            );
        }
        match cells.iter().find(|&&cell| cell >= counts.cells as i64) {
            Some(cell) => self.problem(
                path,
                format!(
                    "cell {cell} doesn't exist; there are {} cells",
                    counts.cells
                ),
            ),
            None => Some((cells.iter().map(|&cell| cell as u32).collect(), off_map)),
        }
    }

    fn feature(&mut self, feature: &Value, path: &str) -> Option<Feature> {
        let kind = self.field(feature, path, "type")?;
        match kind.as_str() {
            Some("ocean") => Some(Feature::Ocean),
            Some("island") => Some(Feature::Island),
            Some("lake") => match feature.get("height") {
                Some(height) => {
                    let at = format!("{path}.height");
                    let [lowest, highest] = [0, 100_00].map(Fixed::from_hundredths);
                    match self.decimal(height, &at)? {
                        surface if lowest <= surface && surface <= highest => {
                            Some(Feature::Lake { surface })
                        }
                        surface => self.problem(
                            &at,
                            format!("{surface} is outside {lowest} to {highest}, Azgaar's heights"),
                        ),
                    }
                }
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
fn hundredths(number: &serde_json::Number) -> Option<i64> {
    scaled(number, 2)
}

/// `number` in units of 10^-`places`, rounded once, half away from zero; `None` if it's too
/// large. A number with decimals is read from its shortest decimal form, the digits the
/// export wrote, so no binary rounding creeps in.
fn scaled(number: &serde_json::Number, places: u32) -> Option<i64> {
    let unit = 10i64.pow(places);
    if let Some(whole) = number.as_i64() {
        return whole.checked_mul(unit);
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
        .chain(std::iter::repeat(0));
    let kept = (0..places).fold(0, |kept, _| kept * 10 + fraction.next().expect("endless"));
    let next = fraction.next().expect("endless");
    let magnitude = whole
        .parse::<i64>()
        .ok()?
        .checked_mul(unit)?
        .checked_add(kept + i64::from(next >= 5))?;
    Some(if negative { -magnitude } else { magnitude })
}

//! `world info <file>` (W1b): what a map file holds, read whole by `factional-content`;
//! `world source <file>` (W2a): what an Azgaar export holds, or every problem in it; and
//! `world import <export> <map>` (W2b): the export rasterised into a map file; and
//! `world suggestions <map>` (W6b): the map's settlements, routes and points of interest, and
//! its cultures, religions and states (W6e); and
//! `world dismiss <map> <id>…` and `world restore <map> <id>…` (W6c): suggestions set aside.

use std::path::Path;

use factional_content::{AzgaarError, fingerprint_of, read_azgaar, read_map, write_map};
use factional_core::Fixed;
use factional_worldgen::{
    Coast, Detail, Drop, Extent, Feature, Foliage, Levels, Placed, PlantKind, Recipe, Region,
    ReligionKind, RouteKind, SettlementKind, Size, Stance, StateForm, Suggestions, Terrain, Tile,
    TilePos, Water, WorldMap, rasterise,
};

use crate::check::count;
use crate::session::WORLD_FORM;

/// The map in `file`, read from `base`, and its length; or why it can't be read.
fn map_in(base: &Path, file: &str) -> Result<(WorldMap, usize), String> {
    let bytes =
        std::fs::read(base.join(file)).map_err(|error| format!("cannot read {file}: {error}"))?;
    let map = read_map(&bytes).map_err(|error| error.to_string())?;
    Ok((map, bytes.len()))
}

/// "a", "a and b", "a, b and c".
fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((only, [])) => only.as_ref().to_owned(),
        Some((last, rest)) => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{} and {}", rest.join(", "), last.as_ref())
        }
    }
}

/// How many of `map`'s suggestions are dismissed.
fn dismissed_count(map: &WorldMap) -> usize {
    (map.suggestions().ids())
        .filter(|id| map.dismissed_by(id).is_some())
        .count()
}

/// " (3 dismissed; 1 dismissal not on the map)", each part only when it isn't none.
fn dismissals(map: &WorldMap) -> String {
    let (dismissed, stale) = (dismissed_count(map), map.not_on_the_map().len());
    let parts: Vec<String> = [
        (dismissed > 0).then(|| format!("{dismissed} dismissed")),
        (stale > 0).then(|| format!("{} not on the map", count(stale, "dismissal"))),
    ]
    .into_iter()
    .flatten()
    .collect();
    match parts.is_empty() {
        true => String::new(),
        false => format!(" ({})", parts.join("; ")),
    }
}

/// What `world dismiss <map> <id>…` does: dismisses each of `ids` on the map in `file`, read
/// from `base`, and writes it back; or why not, writing nothing.
pub fn world_dismiss(base: &Path, file: &str, ids: &[&str]) -> Result<String, String> {
    change_dismissals(base, file, ids, "dismissed", WorldMap::dismiss)
}

/// What `world restore <map> <id>…` does: restores each of `ids` on the map in `file`, read
/// from `base`, and writes it back; or why not, writing nothing.
pub fn world_restore(base: &Path, file: &str, ids: &[&str]) -> Result<String, String> {
    change_dismissals(base, file, ids, "restored", WorldMap::restore)
}

/// The map in `file` changed by `change` with `ids` and written back, saying it was `done`.
fn change_dismissals(
    base: &Path,
    file: &str,
    ids: &[&str],
    done: &str,
    change: fn(&mut WorldMap, &[&str]) -> Result<(), factional_worldgen::MapError>,
) -> Result<String, String> {
    let (mut map, _) = map_in(base, file)?;
    change(&mut map, ids).map_err(|error| error.to_string())?;
    std::fs::write(base.join(file), write_map(&map))
        .map_err(|error| format!("cannot write {file}: {error}"))?;
    Ok(format!(
        "{done} {} in {}; {} of its {} dismissed",
        and_list(ids),
        file_name(file),
        dismissed_count(&map),
        count(map.suggestions().len(), "suggestion")
    ))
}

/// "3 settlements, 3 routes, 2 points of interest, 2 cultures, 2 religions and 2 states".
fn places(suggestions: &Suggestions) -> String {
    and_list(&[
        count(suggestions.settlements.len(), "settlement"),
        count(suggestions.routes.len(), "route"),
        match suggestions.points.len() {
            1 => "1 point of interest".to_owned(),
            n => format!("{n} points of interest"),
        },
        count(suggestions.cultures.len(), "culture"),
        count(suggestions.religions.len(), "religion"),
        count(suggestions.states.len(), "state"),
    ])
}

/// What `world suggestions <map>` says of the map in `file`, read from `base`: how many of
/// each, then each settlement, route, point of interest, culture, religion and state on a line,
/// in the source's order, a link to a suggestion that isn't on the map marked; or why it can't
/// be read.
pub fn world_suggestions(base: &Path, file: &str) -> Result<String, String> {
    let (map, _) = map_in(base, file)?;
    let suggestions = map.suggestions();
    let tile = |at: Placed| format!("({}, {})", at.tile.x, at.tile.y);
    let titled = |id: &str, name: Option<&str>| match name {
        Some(name) => format!("{id} {name}"),
        None => id.to_owned(),
    };
    let marked = |id: &str| match map.dismissed_by(id) {
        None => String::new(),
        Some(by) if by == id => " (dismissed)".to_owned(),
        Some(by) => format!(" (dismissed with {by})"),
    };
    let mut lines = vec![format!(
        "{}: {}{}",
        file_name(file),
        places(suggestions),
        dismissals(&map)
    )];
    for settlement in &suggestions.settlements {
        let mut figures = vec![settlement_kind(settlement.kind).to_owned()];
        figures.extend(settlement.capital.then(|| "capital".to_owned()));
        figures.extend(settlement.port.then(|| "port".to_owned()));
        figures.push(format!("{} people", settlement.people));
        figures.push(format!("at tile {}", tile(settlement.at)));
        let built = settlement.buildings;
        let buildings: Vec<&str> = [
            (built.citadel, "citadel"),
            (built.walls, "walls"),
            (built.plaza, "plaza"),
            (built.temple, "temple"),
            (built.shanty, "shanty town"),
        ]
        .into_iter()
        .filter_map(|(built, name)| built.then_some(name))
        .collect();
        let buildings = match buildings.is_empty() {
            true => String::new(),
            false => format!("; {}", and_list(&buildings)),
        };
        lines.push(format!(
            "{}: {}{buildings}{}",
            titled(&settlement.id, Some(&settlement.name)),
            figures.join(", "),
            marked(&settlement.id)
        ));
    }
    for route in &suggestions.routes {
        let (first, last) = (route.points[0], route.points[route.points.len() - 1]);
        lines.push(format!(
            "{}: {}, {} points, tile {} to {}{}",
            titled(&route.id, route.name.as_deref()),
            match route.kind {
                RouteKind::Road => "road",
                RouteKind::Trail => "trail",
                RouteKind::SeaRoute => "sea route",
            },
            route.points.len(),
            tile(first),
            tile(last),
            marked(&route.id)
        ));
    }
    for point in &suggestions.points {
        let note = point
            .note
            .as_ref()
            .map_or(String::new(), |note| format!("; {note}"));
        lines.push(format!(
            "{}: {}, at tile {}{note}{}",
            titled(&point.id, point.name.as_deref()),
            point.kind,
            tile(point.at),
            marked(&point.id)
        ));
    }
    let linked = |id: &str| match suggestions.ids().any(|on| on == id) {
        true => id.to_owned(),
        false => format!("{id} (not on the map)"),
    };
    let extent = |extent: Extent, tiles: u32| {
        format!(
            "{} and {} people in the source, {} on the map",
            count(extent.cells as usize, "cell"),
            extent.people,
            count(tiles as usize, "tile")
        )
    };
    for culture in &suggestions.cultures {
        lines.push(format!(
            "{} {}: {}; {}{}",
            culture.id,
            culture.name,
            settlement_kind(culture.kind),
            extent(culture.extent, culture.tiles),
            marked(&culture.id)
        ));
    }
    for religion in &suggestions.religions {
        let mut figures = vec![
            match religion.kind {
                ReligionKind::Folk => "folk",
                ReligionKind::Organized => "organized",
                ReligionKind::Cult => "cult",
                ReligionKind::Heresy => "heresy",
            }
            .to_owned(),
            religion.form.clone(),
        ];
        figures.extend(
            religion
                .deity
                .as_ref()
                .map(|deity| format!("deity {deity}")),
        );
        figures.extend(
            religion
                .culture
                .as_ref()
                .map(|c| format!("of {}", linked(c))),
        );
        lines.push(format!(
            "{} {}: {}; {}{}",
            religion.id,
            religion.name,
            figures.join(", "),
            extent(religion.extent, religion.tiles),
            marked(&religion.id)
        ));
    }
    for state in &suggestions.states {
        let mut figures = vec![
            state.full_name.clone(),
            match state.form {
                StateForm::Monarchy => "monarchy",
                StateForm::Republic => "republic",
                StateForm::Union => "union",
                StateForm::Theocracy => "theocracy",
                StateForm::Anarchy => "anarchy",
            }
            .to_owned(),
        ];
        figures.extend(state.culture.as_ref().map(|c| format!("of {}", linked(c))));
        figures.extend(
            state
                .capital
                .as_ref()
                .map(|c| format!("capital {}", linked(c))),
        );
        let stances: Vec<String> = (state.stances.iter())
            .map(|(toward, the)| format!("{} toward {}", stance(*the), linked(toward)))
            .collect();
        let stances = match stances.is_empty() {
            true => String::new(),
            false => format!("; {}", stances.join(", ")),
        };
        lines.push(format!(
            "{} {}: {}{stances}; {}{}",
            state.id,
            state.name,
            figures.join(", "),
            extent(state.extent, state.tiles),
            marked(&state.id)
        ));
    }
    let stale = map.not_on_the_map();
    if !stale.is_empty() {
        lines.push(format!("dismissed, not on the map: {}", stale.join(", ")));
    }
    Ok(lines.join("\n"))
}

/// A stance, as `world suggestions` names it.
fn stance(stance: Stance) -> &'static str {
    match stance {
        Stance::Ally => "ally",
        Stance::Friendly => "friendly",
        Stance::Neutral => "neutral",
        Stance::Suspicion => "suspicion",
        Stance::Enemy => "enemy",
        Stance::Unknown => "unknown",
        Stance::Rival => "rival",
        Stance::Vassal => "vassal",
        Stance::Suzerain => "suzerain",
    }
}

/// A settlement's or culture's type, as `world suggestions` names it.
fn settlement_kind(kind: SettlementKind) -> &'static str {
    match kind {
        SettlementKind::Generic => "generic",
        SettlementKind::Naval => "naval",
        SettlementKind::Lake => "lake",
        SettlementKind::Highland => "highland",
        SettlementKind::River => "river",
        SettlementKind::Nomadic => "nomadic",
        SettlementKind::Hunting => "hunting",
    }
}

/// What `world info <file>` says of the map in `file`, read from `base`: its size, chunks,
/// overrides and suggestions, its length, and its recipe; or why it can't be read.
pub fn world_info(base: &Path, file: &str) -> Result<String, String> {
    let (map, length) = map_in(base, file)?;
    let name = file_name(file);
    let (size, chunks) = (map.size(), map.chunk_count());
    let recipe = map.recipe();
    let region = recipe.region;
    let source = match recipe.source.as_str() {
        "" => "hand-made".to_owned(),
        source => format!("source {source}"),
    };
    Ok(format!(
        "{name}: {} × {} tiles in {} × {} chunks, {}, {}{}; {length} bytes\n\
         recipe: {source}, source pixels ({}, {}) to ({}, {}), {} a source pixel, seed {}; \
         detail {} of the relief, {} pixels, {}; shallows to {} metres, specks under {}; \
         levels every {} of Azgaar's heights, specks under {}; \
         foliage at scale {}, trees {}, shrubs {} and grass {} tiles apart",
        size.width,
        size.height,
        chunks.width,
        chunks.height,
        count(map.overrides().len(), "override"),
        count(map.suggestions().len(), "suggestion"),
        dismissals(&map),
        region.x,
        region.y,
        region.x + region.size.width - 1,
        region.y + region.size.height - 1,
        count(recipe.scale as usize, "tile"),
        recipe.seed,
        recipe.detail.strength,
        recipe.detail.wavelength,
        count(recipe.detail.octaves as usize, "octave"),
        recipe.coast.shallows,
        count(recipe.coast.specks as usize, "tile"),
        recipe.levels.spacing,
        count(recipe.levels.specks as usize, "tile"),
        recipe.foliage.scale,
        recipe.foliage.spacing.tree,
        recipe.foliage.spacing.shrub,
        recipe.foliage.spacing.grass,
    ))
}

/// What `world source <file>` says of the Azgaar export in `file`, read from `base`: its name,
/// version and size, its cells, vertices and features, its rivers, its settlements, routes and
/// points of interest, its cultures, religions and states, and its fingerprint; or why it can't
/// be read, each problem on a line.
pub fn world_source(base: &Path, file: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(base.join(file))
        .map_err(|error| format!("cannot read {file}: {error}"))?;
    let name = file_name(file);
    let source = read_azgaar(&text).map_err(|error| unreadable(&name, error))?;
    let features = |kind: fn(&Feature) -> bool| source.features.iter().filter(|f| kind(f)).count();
    Ok(format!(
        "{name}: {}, from Azgaar {}, {} × {} pixels\n\
         {}, {}; {}, {} and {}; {}; {}, {} and {}; {}, {} and {}; fingerprint {}",
        source.name,
        source.version,
        source.size.width,
        source.size.height,
        count(source.cells.len(), "cell"),
        match source.vertices.len() {
            1 => "1 vertex".to_owned(),
            n => format!("{n} vertices"),
        },
        count(features(|f| *f == Feature::Ocean), "ocean"),
        count(features(|f| matches!(f, Feature::Lake { .. })), "lake"),
        count(features(|f| *f == Feature::Island), "island"),
        count(source.rivers.len(), "river"),
        count(source.settlements.len(), "settlement"),
        count(source.routes.len(), "route"),
        match source.markers.len() {
            1 => "1 point of interest".to_owned(),
            n => format!("{n} points of interest"),
        },
        count(source.cultures.len(), "culture"),
        count(source.religions.len(), "religion"),
        count(source.states.len(), "state"),
        fingerprint_of(text.as_bytes()),
    ))
}

/// What `world import` is told: the export to read, the map file to write, and the recipe's
/// region as typed (the whole source if none), its scale, its seed, its detail, its coast and
/// its levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import<'a> {
    pub export: &'a str,
    pub map: &'a str,
    pub region: Option<&'a str>,
    pub scale: u32,
    pub seed: u32,
    pub detail: Detail,
    pub coast: Coast,
    pub levels: Levels,
    pub foliage: Foliage,
}

impl<'a> Import<'a> {
    /// What the words after `world import` ask for, or why they can't be read.
    pub(crate) fn parse(args: &'a str) -> Result<Import<'a>, String> {
        let mut words = args.split_whitespace();
        let mut files = Vec::new();
        let (mut region, mut scale, mut seed) = (None, 1, 0);
        let (mut detail, mut coast, mut levels) =
            (Detail::default(), Coast::default(), Levels::default());
        let mut foliage = Foliage::default();
        while let Some(word) = words.next() {
            match word {
                "--region" => {
                    region = Some(words.next().ok_or_else(|| REGION_FORM.to_owned())?);
                }
                "--scale" => scale = whole(word, words.next())?,
                "--seed" => seed = whole(word, words.next())?,
                "--detail" => detail.strength = number(word, words.next())?,
                "--wavelength" => detail.wavelength = number(word, words.next())?,
                "--octaves" => detail.octaves = whole(word, words.next())?,
                "--shallows" => coast.shallows = number(word, words.next())?,
                "--specks" => coast.specks = whole(word, words.next())?,
                "--levels" => levels.spacing = number(word, words.next())?,
                "--level-specks" => levels.specks = whole(word, words.next())?,
                "--foliage" => foliage.scale = number(word, words.next())?,
                "--tree-spacing" => foliage.spacing.tree = number(word, words.next())?,
                "--shrub-spacing" => foliage.spacing.shrub = number(word, words.next())?,
                "--grass-spacing" => foliage.spacing.grass = number(word, words.next())?,
                option if option.starts_with("--") => {
                    return Err(format!(
                        "world import has no option {option}; it takes --region, --scale, --seed, \
                         --detail, --wavelength, --octaves, --shallows, --specks, --levels, \
                         --level-specks, --foliage, --tree-spacing, --shrub-spacing and \
                         --grass-spacing"
                    ));
                }
                file => files.push(file),
            }
        }
        match files[..] {
            [export, map] => Ok(Import {
                export,
                map,
                region,
                scale,
                seed,
                detail,
                coast,
                levels,
                foliage,
            }),
            _ => Err(WORLD_FORM.to_owned()),
        }
    }
}

/// How `--region` is written.
const REGION_FORM: &str = "--region needs four whole numbers, <x>,<y>,<width>,<height>";

/// The whole number `value` given to `option`.
fn whole(option: &str, value: Option<&str>) -> Result<u32, String> {
    let needs = format!("{option} needs a whole number");
    let value = value.ok_or_else(|| needs.clone())?;
    value.parse().map_err(|_| format!("{needs}, not {value}"))
}

/// The number `value` given to `option`, to the hundredth.
fn number(option: &str, value: Option<&str>) -> Result<Fixed, String> {
    let needs = format!("{option} needs a number");
    let value = value.ok_or_else(|| needs.clone())?;
    value.parse().map_err(|_| format!("{needs}, not {value}"))
}

/// The region `text` names, `<x>,<y>,<width>,<height>` in source pixels.
fn region(text: &str) -> Result<Region, String> {
    let numbers: Result<Vec<u32>, _> = text.split(',').map(str::parse).collect();
    match numbers.as_deref() {
        Ok(&[x, y, width, height]) => Ok(Region {
            x,
            y,
            size: Size { width, height },
        }),
        _ => Err(format!("{REGION_FORM}, not {text}")),
    }
}

/// What `world import` does, reading from and writing to `base`: rasterises the export with
/// the recipe it's told, writes the map file, and says what it made: its size and chunks, its
/// ocean (deep and shallow), lake and land tiles, its lowest and highest heights, its levels,
/// ledges and cliffs, and its length. Nothing is
/// written if the export doesn't read or the recipe can't make a map of it.
pub fn world_import(base: &Path, import: &Import) -> Result<String, String> {
    let chosen = import.region.map(region).transpose()?;
    let text = std::fs::read_to_string(base.join(import.export))
        .map_err(|error| format!("cannot read {}: {error}", import.export))?;
    let source =
        read_azgaar(&text).map_err(|error| unreadable(&file_name(import.export), error))?;
    let recipe = Recipe {
        source: fingerprint_of(text.as_bytes()),
        region: chosen.unwrap_or(Region {
            x: 0,
            y: 0,
            size: source.size,
        }),
        scale: import.scale,
        seed: import.seed,
        detail: import.detail,
        coast: import.coast,
        levels: import.levels,
        foliage: import.foliage,
    };
    let map = rasterise(&source, recipe).map_err(|error| error.to_string())?;
    let bytes = write_map(&map);
    std::fs::write(base.join(import.map), &bytes)
        .map_err(|error| format!("cannot write {}: {error}", import.map))?;
    let tiles: Vec<_> = map
        .chunks()
        .flat_map(|(_, chunk)| {
            let size = chunk.size();
            (0..size.height)
                .flat_map(move |y| (0..size.width).map(move |x| (x, y)))
                .filter_map(|(x, y)| chunk.tile(x, y))
        })
        .collect();
    let ocean = tiles
        .iter()
        .filter(|tile| {
            matches!(tile.terrain, Terrain::DeepWater | Terrain::ShallowWater)
                && tile.water.is_none()
        })
        .count();
    let lake = tiles
        .iter()
        .filter(|tile| tile.water == Some(Water::Lake))
        .count();
    let river = tiles
        .iter()
        .filter(|tile| matches!(tile.water, Some(Water::River { .. })))
        .count();
    let terrains = land_terrains(&tiles);
    let plants = PlantKind::ALL.map(|kind| {
        tiles
            .iter()
            .filter(|tile| tile.plant.is_some_and(|plant| plant.kind == kind))
            .count()
    });
    let heights = tiles.iter().map(|tile| tile.elevation);
    let temperatures = tiles.iter().map(|tile| tile.temperature);
    let precipitation = tiles.iter().map(|tile| tile.precipitation);
    let biomes: std::collections::BTreeSet<_> = tiles.iter().map(|tile| tile.biome).collect();
    let levels = tiles.iter().map(|tile| tile.level);
    let (size, chunks) = (map.size(), map.chunk_count());
    let mut drops = [0, 0];
    for y in 0..size.height {
        for x in 0..size.width {
            let sides = map.drops(TilePos { x, y }).expect("on the map");
            for drop in [sides.north, sides.west, sides.east, sides.south]
                .into_iter()
                .flatten()
            {
                drops[usize::from(drop == Drop::Cliff)] += 1;
            }
        }
    }
    Ok(format!(
        "imported {} into {}: {} × {} tiles in {} × {} chunks, {ocean} ocean, {lake} lake, {river} river and {} land{terrains}; \
         heights {} to {} m; temperatures {} to {} °C; precipitation {} to {} mm; {}; \
         levels {} to {}, {} and {}; {} ({}, {} and {} grass); {} suggested; {} bytes",
        source.name,
        file_name(import.map),
        size.width,
        size.height,
        chunks.width,
        chunks.height,
        tiles.len() - ocean - lake - river,
        heights.clone().min().expect("a map has tiles"),
        heights.max().expect("a map has tiles"),
        temperatures.clone().min().expect("a map has tiles"),
        temperatures.max().expect("a map has tiles"),
        precipitation.clone().min().expect("a map has tiles"),
        precipitation.max().expect("a map has tiles"),
        count(biomes.len(), "biome"),
        levels.clone().min().expect("a map has tiles"),
        levels.max().expect("a map has tiles"),
        count(drops[0], "ledge"),
        count(drops[1], "cliff"),
        count(plants.iter().sum(), "plant"),
        count(plants[0], "tree"),
        count(plants[1], "shrub"),
        plants[2],
        places(map.suggestions()),
        bytes.len(),
    ))
}

/// How many land tiles take each terrain, in the terrain's order, as ` (22 sand, 339 grass and
/// 47 rock)`; nothing where there's no land.
fn land_terrains(tiles: &[Tile]) -> String {
    let mut counted: Vec<String> = [
        (Terrain::Sand, "sand"),
        (Terrain::Grass, "grass"),
        (Terrain::ForestFloor, "forest floor"),
        (Terrain::Marsh, "marsh"),
        (Terrain::Rock, "rock"),
        (Terrain::Snow, "snow"),
        (Terrain::Ice, "ice"),
    ]
    .into_iter()
    .filter_map(|(terrain, name)| {
        let n = tiles.iter().filter(|tile| tile.terrain == terrain).count();
        (n > 0).then(|| format!("{n} {name}"))
    })
    .collect();
    let Some(last) = counted.pop() else {
        return String::new();
    };
    match counted.is_empty() {
        true => format!(" ({last})"),
        false => format!(" ({} and {last})", counted.join(", ")),
    }
}

/// The last part of `file`'s path.
fn file_name(file: &str) -> std::borrow::Cow<'_, str> {
    Path::new(file)
        .file_name()
        .map_or(file.into(), |name| name.to_string_lossy())
}

/// Why the export `name` doesn't read: each problem on a line, then how many; or its refusal.
fn unreadable(name: &str, error: AzgaarError) -> String {
    match error {
        AzgaarError::Problems(problems) => {
            let mut lines: Vec<String> = problems
                .iter()
                .map(|problem| format!("error: {problem}"))
                .collect();
            lines.push(format!(
                "{name} doesn't read: {}",
                count(problems.len(), "problem")
            ));
            lines.join("\n")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factional_worldgen::Biome;

    fn tiles(terrains: &[Terrain]) -> Vec<Tile> {
        terrains
            .iter()
            .map(|&terrain| Tile {
                elevation: Fixed::ZERO,
                level: 0,
                terrain,
                water: None,
                biome: Biome::Marine,
                temperature: 0,
                precipitation: 0,
                plant: None,
            })
            .collect()
    }

    #[test]
    fn the_land_is_counted_by_terrain_in_the_terrains_order_and_water_not_at_all() {
        use Terrain::*;
        assert_eq!(land_terrains(&tiles(&[DeepWater, ShallowWater])), "");
        assert_eq!(land_terrains(&tiles(&[Ice, DeepWater])), " (1 ice)");
        assert_eq!(
            land_terrains(&tiles(&[Rock, Sand, Rock, ShallowWater])),
            " (1 sand and 2 rock)"
        );
        assert_eq!(
            land_terrains(&tiles(&[Ice, Snow, Rock, Marsh, ForestFloor, Grass, Sand])),
            " (1 sand, 1 grass, 1 forest floor, 1 marsh, 1 rock, 1 snow and 1 ice)"
        );
    }
}

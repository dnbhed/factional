//! `world info <file>` (W1b): what a map file holds, read whole by `factional-content`;
//! `world source <file>` (W2a): what an Azgaar export holds, or every problem in it; and
//! `world import <export> <map>` (W2b): the export rasterised into a map file.

use std::path::Path;

use factional_content::{AzgaarError, fingerprint_of, read_azgaar, read_map, write_map};
use factional_core::Fixed;
use factional_worldgen::{
    Coast, Detail, Drop, Feature, Foliage, Levels, PlantKind, Recipe, Region, Size, Terrain, Tile,
    TilePos, Water, rasterise,
};

use crate::check::count;
use crate::session::WORLD_FORM;

/// What `world info <file>` says of the map in `file`, read from `base`: its size, chunks and
/// overrides, its length, and its recipe; or why it can't be read.
pub fn world_info(base: &Path, file: &str) -> Result<String, String> {
    let bytes =
        std::fs::read(base.join(file)).map_err(|error| format!("cannot read {file}: {error}"))?;
    let map = read_map(&bytes).map_err(|error| error.to_string())?;
    let name = file_name(file);
    let (size, chunks) = (map.size(), map.chunk_count());
    let recipe = map.recipe();
    let region = recipe.region;
    let source = match recipe.source.as_str() {
        "" => "hand-made".to_owned(),
        source => format!("source {source}"),
    };
    Ok(format!(
        "{name}: {} × {} tiles in {} × {} chunks, {}; {} bytes\n\
         recipe: {source}, source pixels ({}, {}) to ({}, {}), {} a source pixel, seed {}; \
         detail {} of the relief, {} pixels, {}; shallows to {} metres, specks under {}; \
         levels every {} of Azgaar's heights, specks under {}; \
         foliage at scale {}, trees {}, shrubs {} and grass {} tiles apart",
        size.width,
        size.height,
        chunks.width,
        chunks.height,
        count(map.overrides().len(), "override"),
        bytes.len(),
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
/// points of interest, and its fingerprint; or why it can't
/// be read, each problem on a line.
pub fn world_source(base: &Path, file: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(base.join(file))
        .map_err(|error| format!("cannot read {file}: {error}"))?;
    let name = file_name(file);
    let source = read_azgaar(&text).map_err(|error| unreadable(&name, error))?;
    let features = |kind: fn(&Feature) -> bool| source.features.iter().filter(|f| kind(f)).count();
    Ok(format!(
        "{name}: {}, from Azgaar {}, {} × {} pixels\n\
         {}, {}; {}, {} and {}; {}; {}, {} and {}; fingerprint {}",
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
         levels {} to {}, {} and {}; {} ({}, {} and {} grass); {} bytes",
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

//! The map file (W1b, D-52, P-133): `world/<name>.map`. A readable header first, TOML text
//! naming the format, version and recipe, and the tiles' length and fingerprint, ended by a
//! zero byte; then the tiles in postcard, each chunk row by row and then the overrides, each
//! compressed by zstd. A file is read back as exactly the map that was written, or refused
//! with why.

use std::fmt;

use factional_core::Fixed;
use factional_worldgen::{
    Biome, CHUNK, ChunkPos, Coast, Detail, Levels, MapError, Override, Plant, PlantKind, Recipe,
    Region, Size, Terrain, Tile, TilePos, Water, WorldMap,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::fingerprint_of;

/// The map file version this build writes and reads.
pub const MAP_VERSION: u32 = 4;

/// How every map file starts: its first line, before anything is parsed.
const FORMAT_LINE: &str = "format = \"factional-map\"\n";

/// zstd's level: its default, as small as lz4_flex's output is twice as large at about the
/// same speed, where level 19 is 5–10% smaller but seconds slower (P-133).
const LEVEL: i32 = 3;

/// The codes each layer is written in: a value's code is its place here. New values go at
/// the end, so files already written keep their meaning.
const TERRAINS: [Terrain; 9] = [
    Terrain::DeepWater,
    Terrain::ShallowWater,
    Terrain::Sand,
    Terrain::Grass,
    Terrain::ForestFloor,
    Terrain::Marsh,
    Terrain::Rock,
    Terrain::Snow,
    Terrain::Ice,
];
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
const KINDS: [PlantKind; 3] = [PlantKind::Tree, PlantKind::Shrub, PlantKind::Grass];

/// Why a map file can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapFileError {
    /// It doesn't start with the format, or nothing ends its header.
    NotAMap,
    Version {
        found: i64,
    },
    Header(String),
    Recipe(MapError),
    Length {
        found: usize,
        expected: u64,
    },
    Fingerprint,
    /// What's wrong with tiles that match their fingerprint, which only a faulty writer makes.
    Damaged(String),
}

impl fmt::Display for MapFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapFileError::NotAMap => f.write_str("this isn't a Factional map"),
            MapFileError::Version { found } => write!(
                f,
                "this map is version {found}, but this build reads version {MAP_VERSION}"
            ),
            MapFileError::Header(why) => write!(f, "this map's header can't be read: {why}"),
            MapFileError::Recipe(why) => write!(f, "this map's recipe can't make a map: {why}"),
            MapFileError::Length { found, expected } => write!(
                f,
                "this map's tiles are {found} bytes, but its header says {expected}; the file is damaged"
            ),
            MapFileError::Fingerprint => {
                f.write_str("this map's tiles don't match their fingerprint; the file is damaged")
            }
            MapFileError::Damaged(what) => write!(f, "this map's tiles are damaged: {what}"),
        }
    }
}

impl std::error::Error for MapFileError {}

/// The header, as read: every key required and none unknown.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    #[expect(dead_code, reason = "checked by its line before the header is parsed")]
    format: String,
    #[expect(dead_code, reason = "checked before the rest of the header")]
    version: i64,
    recipe: HeaderRecipe,
    tiles: HeaderTiles,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderRecipe {
    source: String,
    region: HeaderRegion,
    scale: u32,
    seed: u32,
    detail: HeaderDetail,
    coast: HeaderCoast,
    levels: HeaderLevels,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderDetail {
    strength: Fixed,
    wavelength: Fixed,
    octaves: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderCoast {
    shallows: Fixed,
    specks: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderLevels {
    spacing: Fixed,
    specks: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderTiles {
    bytes: u64,
    fingerprint: String,
}

/// The tiles: each chunk, row by row, then the overrides, each compressed.
#[derive(Serialize, Deserialize)]
struct Body<'a> {
    #[serde(borrow)]
    chunks: Vec<Packed<'a>>,
    overrides: Packed<'a>,
}

/// Compressed bytes, written as postcard's bytes: their length, then each byte.
struct Packed<'a>(&'a [u8]);

impl Serialize for Packed<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Packed<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Packed<'a>, D::Error> {
        <&[u8]>::deserialize(deserializer).map(Packed)
    }
}

/// A chunk's layers, each row by row. Elevation is each tile's difference from the one
/// before it, in hundredths, wrapping, so smooth ground compresses well.
#[derive(Serialize, Deserialize)]
struct Layers {
    elevation: Vec<i64>,
    level: Vec<u8>,
    terrain: Vec<u8>,
    water: Vec<FileWater>,
    biome: Vec<u8>,
    plant: Vec<Option<FilePlant>>,
}

/// Water as written: none, a river with its flow, or a lake, in that order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum FileWater {
    None,
    River { flow: u32 },
    Lake,
}

/// A plant as written: its kind's code, and its offset.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct FilePlant {
    kind: u8,
    offset: [u8; 2],
}

/// An override as written: each layer it sets, elevation in hundredths.
#[derive(Serialize, Deserialize)]
struct FileOverride {
    x: u32,
    y: u32,
    elevation: Option<i64>,
    level: Option<u8>,
    terrain: Option<u8>,
    water: Option<FileWater>,
    biome: Option<u8>,
    plant: Option<Option<FilePlant>>,
}

/// `value`'s code: its place in `table`.
fn code<T: PartialEq>(table: &[T], value: T) -> u8 {
    table
        .iter()
        .position(|one| *one == value)
        .expect("every value has a code") as u8
}

/// The value `code` stands for in `table`, or what's wrong, naming `layer` and `whose`.
fn decode<T: Copy>(table: &[T], code: u8, layer: &str, whose: &str) -> Result<T, MapFileError> {
    table
        .get(usize::from(code))
        .copied()
        .ok_or_else(|| damaged(format!("{whose} has {layer} code {code}, which isn't one")))
}

fn damaged(what: String) -> MapFileError {
    MapFileError::Damaged(what)
}

fn file_water(water: Option<Water>) -> FileWater {
    match water {
        None => FileWater::None,
        Some(Water::River { flow }) => FileWater::River { flow },
        Some(Water::Lake) => FileWater::Lake,
    }
}

fn water_of(water: FileWater) -> Option<Water> {
    match water {
        FileWater::None => None,
        FileWater::River { flow } => Some(Water::River { flow }),
        FileWater::Lake => Some(Water::Lake),
    }
}

fn file_plant(plant: Option<Plant>) -> Option<FilePlant> {
    plant.map(|plant| FilePlant {
        kind: code(&KINDS, plant.kind),
        offset: plant.offset,
    })
}

fn plant_of(plant: Option<FilePlant>, whose: &str) -> Result<Option<Plant>, MapFileError> {
    plant
        .map(|plant| {
            Ok(Plant {
                kind: decode(&KINDS, plant.kind, "plant", whose)?,
                offset: plant.offset,
            })
        })
        .transpose()
}

/// `bytes` compressed.
fn pack(bytes: &[u8]) -> Vec<u8> {
    zstd::bulk::compress(bytes, LEVEL).expect("compressing in memory doesn't fail")
}

/// `value` in postcard.
fn postcard_of<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("a map always serialises")
}

/// `text` as a TOML string, quoted and escaped.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

/// The map file for `map`: its header, a zero byte, then its tiles.
pub fn write_map(map: &WorldMap) -> Vec<u8> {
    let chunks: Vec<Vec<u8>> = map
        .chunks()
        .map(|(_, chunk)| {
            let size = chunk.size();
            let tiles: Vec<Tile> = (0..size.height)
                .flat_map(|y| (0..size.width).map(move |x| (x, y)))
                .map(|(x, y)| chunk.tile(x, y).expect("within the chunk"))
                .collect();
            let mut before = 0i64;
            let layers = Layers {
                elevation: tiles
                    .iter()
                    .map(|tile| {
                        let here = tile.elevation.hundredths();
                        let difference = here.wrapping_sub(before);
                        before = here;
                        difference
                    })
                    .collect(),
                level: tiles.iter().map(|tile| tile.level).collect(),
                terrain: tiles
                    .iter()
                    .map(|tile| code(&TERRAINS, tile.terrain))
                    .collect(),
                water: tiles.iter().map(|tile| file_water(tile.water)).collect(),
                biome: tiles.iter().map(|tile| code(&BIOMES, tile.biome)).collect(),
                plant: tiles.iter().map(|tile| file_plant(tile.plant)).collect(),
            };
            pack(&postcard_of(&layers))
        })
        .collect();
    let overrides: Vec<FileOverride> = map
        .overrides()
        .iter()
        .map(|(at, edit)| FileOverride {
            x: at.x,
            y: at.y,
            elevation: edit.elevation.map(Fixed::hundredths),
            level: edit.level,
            terrain: edit.terrain.map(|terrain| code(&TERRAINS, terrain)),
            water: edit.water.map(file_water),
            biome: edit.biome.map(|biome| code(&BIOMES, biome)),
            plant: edit.plant.map(file_plant),
        })
        .collect();
    let overrides = pack(&postcard_of(&overrides));
    file_of(map.recipe(), &chunks, &overrides)
}

/// A map file of `recipe`, with its packed `chunks` and `overrides`: the header, a zero byte,
/// then the tiles.
fn file_of(recipe: &Recipe, chunks: &[Vec<u8>], overrides: &[u8]) -> Vec<u8> {
    let tiles = postcard_of(&Body {
        chunks: chunks.iter().map(|chunk| Packed(chunk)).collect(),
        overrides: Packed(overrides),
    });
    let region = recipe.region;
    let header = format!(
        "{FORMAT_LINE}version = {MAP_VERSION}\n\
         \n\
         [recipe]\n\
         source = {}\n\
         region = {{ x = {}, y = {}, width = {}, height = {} }}\n\
         scale = {}\n\
         seed = {}\n\
         detail = {{ strength = {}, wavelength = {}, octaves = {} }}\n\
         coast = {{ shallows = {}, specks = {} }}\n\
         levels = {{ spacing = {}, specks = {} }}\n\
         \n\
         [tiles]\n\
         bytes = {}\n\
         fingerprint = {}\n",
        quoted(&recipe.source),
        region.x,
        region.y,
        region.size.width,
        region.size.height,
        recipe.scale,
        recipe.seed,
        recipe.detail.strength,
        recipe.detail.wavelength,
        recipe.detail.octaves,
        recipe.coast.shallows,
        recipe.coast.specks,
        recipe.levels.spacing,
        recipe.levels.specks,
        tiles.len(),
        quoted(&fingerprint_of(&tiles)),
    );
    let mut file = header.into_bytes();
    file.push(0);
    file.extend_from_slice(&tiles);
    file
}

/// The map in `bytes`, exactly as it was written, or why it can't be read: a wrong file, a
/// version or header named before any tile is decoded.
pub fn read_map(bytes: &[u8]) -> Result<WorldMap, MapFileError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .filter(|_| bytes.starts_with(FORMAT_LINE.as_bytes()))
        .ok_or(MapFileError::NotAMap)?;
    let header = std::str::from_utf8(&bytes[..end])
        .map_err(|_| MapFileError::Header("it isn't text".to_owned()))?;
    let unreadable = |error: toml::de::Error| MapFileError::Header(error.message().to_owned());
    let table: toml::Table = toml::from_str(header).map_err(unreadable)?;
    match table.get("version").and_then(toml::Value::as_integer) {
        Some(found) if found != i64::from(MAP_VERSION) => {
            return Err(MapFileError::Version { found });
        }
        _ => {}
    }
    let header: Header = toml::from_str(header).map_err(unreadable)?;
    let recipe = Recipe {
        source: header.recipe.source,
        region: Region {
            x: header.recipe.region.x,
            y: header.recipe.region.y,
            size: Size {
                width: header.recipe.region.width,
                height: header.recipe.region.height,
            },
        },
        scale: header.recipe.scale,
        seed: header.recipe.seed,
        detail: Detail {
            strength: header.recipe.detail.strength,
            wavelength: header.recipe.detail.wavelength,
            octaves: header.recipe.detail.octaves,
        },
        coast: Coast {
            shallows: header.recipe.coast.shallows,
            specks: header.recipe.coast.specks,
        },
        levels: Levels {
            spacing: header.recipe.levels.spacing,
            specks: header.recipe.levels.specks,
        },
    };
    let size = recipe.size().map_err(MapFileError::Recipe)?;
    let tiles = &bytes[end + 1..];
    if tiles.len() as u64 != header.tiles.bytes {
        return Err(MapFileError::Length {
            found: tiles.len(),
            expected: header.tiles.bytes,
        });
    }
    if fingerprint_of(tiles) != header.tiles.fingerprint {
        return Err(MapFileError::Fingerprint);
    }
    let body: Body = postcard::from_bytes(tiles)
        .map_err(|error| damaged(format!("they can't be decoded: {error}")))?;
    let across = size.width.div_ceil(CHUNK);
    let count = across * size.height.div_ceil(CHUNK);
    if body.chunks.len() != count as usize {
        return Err(damaged(format!(
            "there are {} chunks, not {count}",
            body.chunks.len()
        )));
    }
    let chunks = body
        .chunks
        .iter()
        .zip(0u32..)
        .map(|(packed, i)| {
            let at = ChunkPos {
                x: i % across,
                y: i / across,
            };
            let width = (size.width - at.x * CHUNK).min(CHUNK);
            let height = (size.height - at.y * CHUNK).min(CHUNK);
            chunk_tiles(packed.0, at, (width * height) as usize)
        })
        .collect::<Result<Vec<Vec<Tile>>, MapFileError>>()?;
    let mut map = WorldMap::new(recipe, |at| {
        let chunk = (at.y / CHUNK * across + at.x / CHUNK) as usize;
        let width = (size.width - at.x / CHUNK * CHUNK).min(CHUNK);
        chunks[chunk][(at.y % CHUNK * width + at.x % CHUNK) as usize]
    })
    .map_err(MapFileError::Recipe)?;
    let overrides: Vec<FileOverride> = unpack(body.overrides.0, "the overrides")?;
    let mut last: Option<TilePos> = None;
    for written in overrides {
        let at = TilePos {
            x: written.x,
            y: written.y,
        };
        let whose = format!("the override of tile ({}, {})", at.x, at.y);
        if last.is_some_and(|last| last >= at) {
            return Err(damaged(format!("{whose} is out of order")));
        }
        last = Some(at);
        let edit = Override {
            elevation: written.elevation.map(Fixed::from_hundredths),
            level: written.level,
            terrain: written
                .terrain
                .map(|terrain| decode(&TERRAINS, terrain, "terrain", &whose))
                .transpose()?,
            water: written.water.map(water_of),
            biome: written
                .biome
                .map(|biome| decode(&BIOMES, biome, "biome", &whose))
                .transpose()?,
            plant: written
                .plant
                .map(|plant| plant_of(plant, &whose))
                .transpose()?,
        };
        if edit.is_empty() {
            return Err(damaged(format!("{whose} sets nothing")));
        }
        map.edit(at, edit)
            .map_err(|_| damaged(format!("{whose} is off the map")))?;
    }
    Ok(map)
}

/// `packed` decompressed and decoded, or what's wrong, naming `what`.
fn unpack<T: for<'de> Deserialize<'de>>(packed: &[u8], what: &str) -> Result<T, MapFileError> {
    let bytes = zstd::stream::decode_all(packed)
        .map_err(|error| damaged(format!("{what} can't be decompressed: {error}")))?;
    postcard::from_bytes(&bytes)
        .map_err(|error| damaged(format!("{what} can't be decoded: {error}")))
}

/// The `count` tiles of the chunk at `at`, row by row, from its packed layers.
fn chunk_tiles(packed: &[u8], at: ChunkPos, count: usize) -> Result<Vec<Tile>, MapFileError> {
    let whose = format!("chunk ({}, {})", at.x, at.y);
    let layers: Layers = unpack(packed, &whose)?;
    let lengths = [
        ("elevation", layers.elevation.len()),
        ("level", layers.level.len()),
        ("terrain", layers.terrain.len()),
        ("water", layers.water.len()),
        ("biome", layers.biome.len()),
        ("plant", layers.plant.len()),
    ];
    if let Some((layer, length)) = lengths.iter().find(|(_, length)| *length != count) {
        return Err(damaged(format!(
            "{whose}'s {layer} has {length} tiles, not {count}"
        )));
    }
    let mut elevation = 0i64;
    (0..count)
        .map(|i| {
            elevation = elevation.wrapping_add(layers.elevation[i]);
            Ok(Tile {
                elevation: Fixed::from_hundredths(elevation),
                level: layers.level[i],
                terrain: decode(&TERRAINS, layers.terrain[i], "terrain", &whose)?,
                water: water_of(layers.water[i]),
                biome: decode(&BIOMES, layers.biome[i], "biome", &whose)?,
                plant: plant_of(layers.plant[i], &whose)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 70 × 2 tiles: two chunks, of 64 × 2 and 6 × 2 tiles.
    fn recipe() -> Recipe {
        Recipe {
            source: String::new(),
            region: Region {
                x: 0,
                y: 0,
                size: Size {
                    width: 70,
                    height: 2,
                },
            },
            scale: 1,
            seed: 7,
            detail: Detail::default(),
            coast: Coast::default(),
            levels: Levels::default(),
        }
    }

    /// The layers of `count` tiles of flat grass on Grassland, with nothing on them.
    fn layers(count: usize) -> Layers {
        Layers {
            elevation: vec![0; count],
            level: vec![0; count],
            terrain: vec![3; count],
            water: vec![FileWater::None; count],
            biome: vec![4; count],
            plant: vec![None; count],
        }
    }

    /// An override of tile (`x`, `y`) to sand.
    fn sand(x: u32, y: u32) -> FileOverride {
        FileOverride {
            x,
            y,
            elevation: None,
            level: None,
            terrain: Some(2),
            water: None,
            biome: None,
            plant: None,
        }
    }

    /// What reading the map with `first` and `second` as its chunks' layers, and
    /// `overrides`, says is damaged.
    fn damage(first: Layers, second: Layers, overrides: Vec<FileOverride>) -> String {
        let chunks = [first, second].map(|layers| pack(&postcard_of(&layers)));
        let file = file_of(&recipe(), &chunks, &pack(&postcard_of(&overrides)));
        read_map(&file).expect_err("the map is refused").to_string()
    }

    fn says(what: &str) -> String {
        format!("this map's tiles are damaged: {what}")
    }

    #[test]
    fn crafted_layers_read_as_their_tiles() {
        let mut second = layers(12);
        second.level[5] = 7;
        let chunks = [layers(128), second].map(|layers| pack(&postcard_of(&layers)));
        let mut overrides: Vec<FileOverride> = vec![sand(0, 1), sand(69, 1)];
        overrides[1].level = Some(3);
        let file = file_of(&recipe(), &chunks, &pack(&postcard_of(&overrides)));
        let map = read_map(&file).expect("the map reads");
        let grass = Tile {
            elevation: Fixed::ZERO,
            level: 7,
            terrain: Terrain::Grass,
            water: None,
            biome: Biome::Grassland,
            plant: None,
        };
        assert_eq!(map.tile(TilePos { x: 69, y: 0 }), Some(grass));
        let sand = Tile {
            level: 3,
            terrain: Terrain::Sand,
            ..grass
        };
        assert_eq!(map.tile(TilePos { x: 69, y: 1 }), Some(sand));
        assert_eq!(map.overrides().len(), 2);
    }

    #[test]
    fn a_missing_chunk_is_damage() {
        let chunks = [pack(&postcard_of(&layers(128)))];
        let file = file_of(
            &recipe(),
            &chunks,
            &pack(&postcard_of(&Vec::<FileOverride>::new())),
        );
        assert_eq!(
            read_map(&file).map_err(|error| error.to_string()),
            Err(says("there are 1 chunks, not 2"))
        );
    }

    #[test]
    fn tiles_that_arent_postcard_are_damage() {
        // A real map's header, its tiles swapped for one byte that isn't postcard, with
        // that byte's length and fingerprint.
        let file = file_of(&recipe(), &[], &[]);
        let end = file.iter().position(|byte| *byte == 0).expect("a header");
        let header = std::str::from_utf8(&file[..end]).expect("text");
        let header = header.split("[tiles]").next().expect("a header");
        let mut file = format!(
            "{header}[tiles]\nbytes = 1\nfingerprint = \"{}\"\n",
            fingerprint_of(&[0xff])
        )
        .into_bytes();
        file.extend_from_slice(&[0, 0xff]);
        let refused = read_map(&file).expect_err("refused").to_string();
        assert!(
            refused.starts_with(&says("they can't be decoded: ")),
            "{refused}"
        );
    }

    #[test]
    fn a_chunk_that_isnt_compressed_or_isnt_postcard_is_damage() {
        let chunks = [b"not zstd".to_vec(), pack(&postcard_of(&layers(12)))];
        let none = pack(&postcard_of(&Vec::<FileOverride>::new()));
        let refused = read_map(&file_of(&recipe(), &chunks, &none))
            .expect_err("refused")
            .to_string();
        assert!(
            refused.starts_with(&says("chunk (0, 0) can't be decompressed: ")),
            "{refused}"
        );
        let chunks = [pack(&postcard_of(&layers(128))), pack(&[0xff, 0xff])];
        let refused = read_map(&file_of(&recipe(), &chunks, &none))
            .expect_err("refused")
            .to_string();
        assert!(
            refused.starts_with(&says("chunk (1, 0) can't be decoded: ")),
            "{refused}"
        );
    }

    #[test]
    fn a_layer_of_the_wrong_length_is_damage_naming_it() {
        let mut short = layers(12);
        short.water.pop();
        assert_eq!(
            damage(layers(128), short, vec![]),
            says("chunk (1, 0)'s water has 11 tiles, not 12")
        );
        for (layer, cut) in [
            ("elevation", 0),
            ("level", 2),
            ("terrain", 1),
            ("biome", 3),
            ("plant", 4),
        ] {
            let mut short = layers(128);
            match cut {
                0 => drop(short.elevation.pop()),
                1 => drop(short.terrain.pop()),
                2 => drop(short.level.pop()),
                3 => drop(short.biome.pop()),
                _ => drop(short.plant.pop()),
            }
            assert_eq!(
                damage(short, layers(12), vec![]),
                says(&format!("chunk (0, 0)'s {layer} has 127 tiles, not 128"))
            );
        }
    }

    #[test]
    fn an_unknown_code_is_damage_naming_the_layer() {
        let mut first = layers(128);
        first.terrain[5] = 9;
        assert_eq!(
            damage(first, layers(12), vec![]),
            says("chunk (0, 0) has terrain code 9, which isn't one")
        );
        let mut first = layers(128);
        first.biome[5] = 13;
        assert_eq!(
            damage(first, layers(12), vec![]),
            says("chunk (0, 0) has biome code 13, which isn't one")
        );
        let mut first = layers(128);
        first.plant[5] = Some(FilePlant {
            kind: 3,
            offset: [0, 0],
        });
        assert_eq!(
            damage(first, layers(12), vec![]),
            says("chunk (0, 0) has plant code 3, which isn't one")
        );
        let mut biome = sand(4, 1);
        biome.terrain = None;
        biome.biome = Some(13);
        assert_eq!(
            damage(layers(128), layers(12), vec![biome]),
            says("the override of tile (4, 1) has biome code 13, which isn't one")
        );
    }

    #[test]
    fn overrides_out_of_order_twice_over_or_setting_nothing_are_damage() {
        assert_eq!(
            damage(layers(128), layers(12), vec![sand(0, 1), sand(69, 0)]),
            says("the override of tile (69, 0) is out of order")
        );
        assert_eq!(
            damage(layers(128), layers(12), vec![sand(3, 1), sand(3, 1)]),
            says("the override of tile (3, 1) is out of order")
        );
        let mut nothing = sand(3, 1);
        nothing.terrain = None;
        assert_eq!(
            damage(layers(128), layers(12), vec![nothing]),
            says("the override of tile (3, 1) sets nothing")
        );
        assert_eq!(
            damage(layers(128), layers(12), vec![sand(70, 0)]),
            says("the override of tile (70, 0) is off the map")
        );
    }

    #[test]
    fn overrides_that_cant_be_decoded_are_damage() {
        let chunks = [layers(128), layers(12)].map(|layers| pack(&postcard_of(&layers)));
        let refused = read_map(&file_of(&recipe(), &chunks, &pack(&[0xff])))
            .expect_err("refused")
            .to_string();
        assert!(
            refused.starts_with(&says("the overrides can't be decoded: ")),
            "{refused}"
        );
    }

    #[test]
    fn a_header_that_isnt_text_is_refused() {
        let mut file = FORMAT_LINE.as_bytes().to_vec();
        file.extend_from_slice(&[0xff, 0]);
        assert_eq!(
            read_map(&file),
            Err(MapFileError::Header("it isn't text".to_owned()))
        );
    }

    #[test]
    fn a_header_without_its_version_is_refused_naming_it() {
        let file = b"format = \"factional-map\"\n\0";
        let refused = read_map(file).expect_err("refused").to_string();
        assert!(refused.contains("version"), "{refused}");
    }
}

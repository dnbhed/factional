//! The map file (W1b, P-133): a readable header, then the tiles in postcard, each chunk and the
//! overrides compressed by zstd; written and read back exactly, and every damaged or foreign
//! file refused with why. Every map here is hand-made.

use factional_content::{MAP_VERSION, MapFileError, fingerprint_of, read_map, write_map};
use factional_core::Fixed;
use factional_worldgen::{
    Biome, Coast, Detail, Override, Plant, PlantKind, Recipe, Region, Size, Terrain, Tile, TilePos,
    Water, WorldMap,
};
use proptest::prelude::*;

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

/// The committed map, written by this build when `FACTIONAL_WRITE_FIXTURES` is set.
const COMMITTED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/maps/hand-made.map"
);

fn at(x: u32, y: u32) -> TilePos {
    TilePos { x, y }
}

fn recipe(x: u32, y: u32, width: u32, height: u32, scale: u32) -> Recipe {
    Recipe {
        source: String::new(),
        region: Region {
            x,
            y,
            size: Size { width, height },
        },
        scale,
        seed: 7,
        detail: Detail::default(),
        coast: Coast::default(),
    }
}

/// A tile with every layer in use somewhere: elevations above and below zero, each terrain
/// and biome, rivers and lakes, and each kind of plant at varied offsets.
fn varied(pos: TilePos) -> Tile {
    let (x, y) = (pos.x, pos.y);
    Tile {
        elevation: Fixed::from_hundredths((i64::from(x) * 37 - i64::from(y) * 101) * 3),
        terrain: TERRAINS[((x + y) % 9) as usize],
        water: match (x + y) % 7 {
            0 => Some(Water::River { flow: x * 10 + y }),
            3 => Some(Water::Lake),
            _ => None,
        },
        biome: BIOMES[((x * 3 + y) % 13) as usize],
        plant: match (x * y) % 5 {
            kind @ 0..=2 => Some(Plant {
                kind: KINDS[kind as usize],
                offset: [(x * 3) as u8, (y * 5) as u8],
            }),
            _ => None,
        },
    }
}

/// 70 × 66 tiles, so 2 × 2 chunks with partial ones at the right and bottom, from source
/// pixels (3, 4) to (37, 36) at 2 tiles a pixel, with three overrides that set every layer
/// between them, water and a plant cleared among them.
fn hand_made() -> WorldMap {
    let mut map = WorldMap::new(recipe(3, 4, 35, 33, 2), varied).expect("the recipe makes a map");
    let edits = [
        (
            at(0, 0),
            Override {
                elevation: Some(Fixed::from_hundredths(-250_00)),
                terrain: Some(Terrain::Ice),
                ..Override::default()
            },
        ),
        (
            at(65, 3),
            Override {
                water: Some(None),
                plant: Some(None),
                ..Override::default()
            },
        ),
        (
            at(69, 65),
            Override {
                biome: Some(Biome::Wetland),
                water: Some(Some(Water::River { flow: 5 })),
                plant: Some(Some(Plant {
                    kind: PlantKind::Grass,
                    offset: [1, 2],
                })),
                ..Override::default()
            },
        ),
    ];
    for (pos, edit) in edits {
        map.edit(pos, edit).expect("on the map");
    }
    map
}

/// The header's text and the tiles after it, split at the first zero byte.
fn split(bytes: &[u8]) -> (&str, &[u8]) {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .expect("a zero byte ends the header");
    let header = std::str::from_utf8(&bytes[..end]).expect("the header is text");
    (header, &bytes[end + 1..])
}

/// `bytes` with the header's `from` replaced by `to`.
fn with_header(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let (header, tiles) = split(bytes);
    assert!(header.contains(from), "the header has {from}");
    let mut changed = header.replacen(from, to, 1).into_bytes();
    changed.push(0);
    changed.extend_from_slice(tiles);
    changed
}

fn refusal(bytes: &[u8]) -> String {
    read_map(bytes).expect_err("the map is refused").to_string()
}

#[test]
fn a_map_reads_back_exactly_as_written() {
    let map = hand_made();
    assert_eq!(read_map(&write_map(&map)), Ok(map));
}

#[test]
fn the_same_map_always_writes_the_same_bytes() {
    assert_eq!(write_map(&hand_made()), write_map(&hand_made()));
}

#[test]
fn the_header_is_readable_text_naming_the_tiles_length_and_fingerprint() {
    let bytes = write_map(&hand_made());
    let (header, tiles) = split(&bytes);
    let expected = format!(
        r#"format = "factional-map"
version = 3

[recipe]
source = ""
region = {{ x = 3, y = 4, width = 35, height = 33 }}
scale = 2
seed = 7
detail = {{ strength = 0.25, wavelength = 8.00, octaves = 4 }}
coast = {{ shallows = 10.00, specks = 4 }}

[tiles]
bytes = {}
fingerprint = "{}"
"#,
        tiles.len(),
        fingerprint_of(tiles)
    );
    assert_eq!(header, expected);
    assert_eq!(MAP_VERSION, 3);
}

#[test]
fn a_source_fingerprint_and_a_large_seed_are_kept() {
    let mut recipe = recipe(0, 0, 3, 2, 1);
    recipe.source = "fnv1a64:00ff00ff00ff00ff".to_owned();
    recipe.seed = u32::MAX;
    let map = WorldMap::new(recipe, varied).expect("the recipe makes a map");
    let bytes = write_map(&map);
    let (header, _) = split(&bytes);
    assert!(header.contains("source = \"fnv1a64:00ff00ff00ff00ff\"\n"));
    assert!(header.contains("seed = 4294967295\n"));
    assert_eq!(read_map(&bytes), Ok(map));
}

#[test]
fn elevations_at_either_extreme_side_by_side_read_back_exactly() {
    let extreme = |pos: TilePos| Tile {
        elevation: Fixed::from_hundredths(match (pos.x + pos.y) % 2 {
            0 => i64::MAX,
            _ => i64::MIN,
        }),
        ..varied(pos)
    };
    let map = WorldMap::new(recipe(0, 0, 3, 2, 1), extreme).expect("the recipe makes a map");
    assert_eq!(read_map(&write_map(&map)), Ok(map));
}

#[test]
fn a_committed_map_still_reads_as_the_same_map() {
    if std::env::var_os("FACTIONAL_WRITE_FIXTURES").is_some() {
        std::fs::write(COMMITTED, write_map(&hand_made())).expect("the fixture is written");
    }
    let bytes = std::fs::read(COMMITTED).unwrap_or_default();
    assert_eq!(read_map(&bytes), Ok(hand_made()));
}

#[test]
fn a_file_that_doesnt_start_with_the_format_isnt_a_map() {
    let not_a_map = "this isn't a Factional map";
    assert_eq!(refusal(b"name = \"The Watch\"\n"), not_a_map);
    assert_eq!(refusal(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), not_a_map);
    assert_eq!(refusal(b""), not_a_map);
    // The format alone, with no zero byte ending the header, is no map either.
    assert_eq!(
        refusal(b"format = \"factional-map\"\nversion = 1\n"),
        not_a_map
    );
    assert_eq!(read_map(b""), Err(MapFileError::NotAMap));
}

#[test]
fn another_version_is_refused_naming_both() {
    for found in [2, 4] {
        let bytes = with_header(
            &write_map(&hand_made()),
            "version = 3",
            &format!("version = {found}"),
        );
        assert_eq!(
            refusal(&bytes),
            format!("this map is version {found}, but this build reads version 3")
        );
    }
}

#[test]
fn the_recipes_detail_is_kept() {
    let mut recipe = recipe(0, 0, 3, 2, 1);
    recipe.detail = Detail {
        strength: Fixed::ONE,
        wavelength: Fixed::from_hundredths(12_34),
        octaves: 8,
    };
    let map = WorldMap::new(recipe, varied).expect("the recipe makes a map");
    let bytes = write_map(&map);
    let (header, _) = split(&bytes);
    assert!(
        header.contains("detail = { strength = 1.00, wavelength = 12.34, octaves = 8 }\n"),
        "{header}"
    );
    assert_eq!(read_map(&bytes), Ok(map));
}

#[test]
fn detail_outside_its_ranges_is_refused_with_why() {
    let bytes = with_header(&write_map(&hand_made()), "octaves = 4", "octaves = 9");
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the detail needs 1 to 8 octaves, not 9"
    );
    let bytes = with_header(
        &write_map(&hand_made()),
        "strength = 0.25",
        "strength = 1.5",
    );
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the detail's strength must be 0.00 to 1.00, not 1.50"
    );
    let bytes = with_header(&write_map(&hand_made()), "strength = 0.25, ", "");
    let refused = refusal(&bytes);
    assert!(refused.contains("strength"), "{refused}");
}

#[test]
fn the_recipes_coast_is_kept() {
    let mut recipe = recipe(0, 0, 3, 2, 1);
    recipe.coast = Coast {
        shallows: Fixed::from_hundredths(1000_00),
        specks: 4096,
    };
    let map = WorldMap::new(recipe, varied).expect("the recipe makes a map");
    let bytes = write_map(&map);
    let (header, _) = split(&bytes);
    assert!(
        header.contains("coast = { shallows = 1000.00, specks = 4096 }\n"),
        "{header}"
    );
    assert_eq!(read_map(&bytes), Ok(map));
}

#[test]
fn a_coast_outside_its_ranges_is_refused_with_why() {
    let bytes = with_header(&write_map(&hand_made()), "specks = 4", "specks = 4097");
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the coast's specks must be 0 to 4096 tiles, not 4097"
    );
    let bytes = with_header(
        &write_map(&hand_made()),
        "shallows = 10.00",
        "shallows = -1",
    );
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the coast's shallows must be 0.00 to 1000.00 metres, not -1.00"
    );
    let bytes = with_header(&write_map(&hand_made()), "shallows = 10.00, ", "");
    let refused = refusal(&bytes);
    assert!(refused.contains("shallows"), "{refused}");
}

#[test]
fn a_header_with_a_misspelt_key_is_refused_naming_it() {
    let bytes = with_header(&write_map(&hand_made()), "scale = 2", "scael = 2");
    let refused = refusal(&bytes);
    assert!(
        refused.starts_with("this map's header can't be read: "),
        "{refused}"
    );
    assert!(refused.contains("scael"), "{refused}");
}

#[test]
fn a_header_with_a_missing_key_is_refused_naming_it() {
    let bytes = with_header(&write_map(&hand_made()), "seed = 7\n", "");
    let refused = refusal(&bytes);
    assert!(
        refused.starts_with("this map's header can't be read: "),
        "{refused}"
    );
    assert!(refused.contains("seed"), "{refused}");
}

#[test]
fn a_recipe_that_cant_make_a_map_is_refused_with_why() {
    let bytes = with_header(&write_map(&hand_made()), "scale = 2", "scale = 0");
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the scale must be at least 1 tile a source pixel"
    );
}

#[test]
fn tiles_of_another_length_are_refused() {
    let bytes = write_map(&hand_made());
    let length = split(&bytes).1.len();
    assert_eq!(
        refusal(&bytes[..bytes.len() - 10]),
        format!(
            "this map's tiles are {} bytes, but its header says {length}; the file is damaged",
            length - 10
        )
    );
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(
        refusal(&longer),
        format!(
            "this map's tiles are {} bytes, but its header says {length}; the file is damaged",
            length + 1
        )
    );
}

#[test]
fn tiles_that_dont_match_their_fingerprint_are_refused() {
    let mut bytes = write_map(&hand_made());
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert_eq!(
        refusal(&bytes),
        "this map's tiles don't match their fingerprint; the file is damaged"
    );
}

/// A small map's tile from `seed`: any value in every layer.
fn any_tile(seed: u64, pos: TilePos) -> Tile {
    let mut bits = seed ^ (u64::from(pos.x) << 32 | u64::from(pos.y));
    let mut next = |below: u64| {
        bits = bits
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .rotate_left(29)
            .wrapping_add(1);
        bits % below
    };
    Tile {
        elevation: Fixed::from_hundredths(next(u64::MAX) as i64),
        terrain: TERRAINS[next(9) as usize],
        water: match next(3) {
            0 => None,
            1 => Some(Water::Lake),
            _ => Some(Water::River {
                flow: next(u64::from(u32::MAX)) as u32,
            }),
        },
        biome: BIOMES[next(13) as usize],
        plant: match next(4) {
            3 => None,
            kind => Some(Plant {
                kind: KINDS[kind as usize],
                offset: [next(256) as u8, next(256) as u8],
            }),
        },
    }
}

proptest! {
    #[test]
    fn any_small_map_reads_back_exactly_as_written(
        width in 1u32..80,
        height in 1u32..80,
        seed in any::<u64>(),
        edits in proptest::collection::vec((any::<u32>(), any::<u32>(), any::<u64>()), 0..12),
    ) {
        let mut map = WorldMap::new(recipe(0, 0, width, height, 1), |pos| any_tile(seed, pos))
            .expect("the recipe makes a map");
        for (x, y, bits) in edits {
            let pos = at(x % width, y % height);
            let tile = any_tile(bits, pos);
            let edit = Override {
                elevation: (bits & 1 != 0).then_some(tile.elevation),
                terrain: (bits & 2 != 0).then_some(tile.terrain),
                water: (bits & 4 != 0).then_some(tile.water),
                biome: (bits & 8 != 0).then_some(tile.biome),
                plant: (bits & 16 != 0).then_some(tile.plant),
            };
            map.edit(pos, edit).expect("on the map");
        }
        prop_assert_eq!(read_map(&write_map(&map)), Ok(map));
    }
}

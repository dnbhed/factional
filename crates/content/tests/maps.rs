//! The map file (W1b, P-133): a readable header, then the tiles in postcard, each chunk and the
//! overrides compressed by zstd; written and read back exactly, and every damaged or foreign
//! file refused with why. Every map here is hand-made.

use factional_content::{MAP_VERSION, MapFileError, fingerprint_of, read_map, write_map};
use factional_core::Fixed;
use factional_worldgen::{
    Biome, Buildings, Coast, Culture, Detail, Extent, Foliage, Levels, Override, PerKind, Placed,
    Plant, PlantKind, PointOfInterest, Recipe, Region, Religion, ReligionKind, Route, RouteKind,
    Settlement, SettlementKind, Size, Stance, State, StateForm, Suggestions, Terrain, Tile,
    TilePos, Water, WorldMap,
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
        levels: Levels::default(),
        foliage: Foliage::default(),
    }
}

/// A tile with every layer in use somewhere: elevations above and below zero, levels from 0
/// to 20, each terrain and biome, temperatures from −30 to 29 °C and precipitation from 0 to
/// 3900 mm, rivers and lakes, and each kind of plant at varied offsets.
fn varied(pos: TilePos) -> Tile {
    let (x, y) = (pos.x, pos.y);
    Tile {
        elevation: Fixed::from_hundredths((i64::from(x) * 37 - i64::from(y) * 101) * 3),
        level: ((x / 4 + y / 3) % 21) as u8,
        terrain: TERRAINS[((x + y) % 9) as usize],
        water: match (x + y) % 7 {
            0 => Some(Water::River { flow: x * 10 + y }),
            3 => Some(Water::Lake),
            _ => None,
        },
        biome: BIOMES[((x * 3 + y) % 13) as usize],
        // Climate in bands, below freezing and above, as Azgaar's grid gives it.
        temperature: ((x / 5 + y / 4) % 60) as i8 - 30,
        precipitation: ((x / 6 + y / 5) % 40) as u16 * 100,
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
                level: Some(255),
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
    map.suggest(hand_made_suggestions()).expect("on the map");
    // Its point of interest dismissed, and route_7, not on it.
    map.keep_dismissed(hand_made_dismissals())
        .expect("valid ids");
    map
}

fn hand_made_dismissals() -> std::collections::BTreeSet<String> {
    ["poi_0".to_owned(), "route_7".to_owned()].into()
}

fn placed(x: u32, y: u32, offset: [u8; 2]) -> Placed {
    Placed {
        tile: at(x, y),
        offset,
    }
}

/// A settlement, a route of three points and a point of interest, on the hand-made map; and a
/// culture, a religion and a state (W6e), the state's culture not on the map.
fn hand_made_suggestions() -> Suggestions {
    Suggestions {
        settlements: vec![Settlement {
            id: "settlement_9".to_owned(),
            name: "Riverhold".to_owned(),
            at: placed(12, 40, [0, 255]),
            people: 24_129,
            kind: SettlementKind::River,
            capital: true,
            port: true,
            buildings: Buildings {
                citadel: true,
                walls: false,
                plaza: true,
                temple: false,
                shanty: true,
            },
        }],
        routes: vec![Route {
            id: "route_4_2".to_owned(),
            name: Some("Ferry way".to_owned()),
            kind: RouteKind::SeaRoute,
            points: vec![
                placed(0, 0, [0, 0]),
                placed(69, 65, [255, 255]),
                placed(12, 40, [0, 255]),
            ],
        }],
        points: vec![PointOfInterest {
            id: "poi_0".to_owned(),
            kind: "ruins".to_owned(),
            icon: "glyph-1f3f0".to_owned(),
            name: None,
            note: Some("Walls of an older keep.".to_owned()),
            at: placed(33, 7, [128, 64]),
        }],
        cultures: vec![Culture {
            id: "culture_2".to_owned(),
            name: "Riverfolk".to_owned(),
            kind: SettlementKind::River,
            extent: Extent {
                cells: 40,
                people: 61_234,
            },
            tiles: 1200,
        }],
        religions: vec![Religion {
            id: "religion_5".to_owned(),
            name: "Ferry Saints".to_owned(),
            kind: ReligionKind::Heresy,
            form: "Sect".to_owned(),
            deity: None,
            culture: Some("culture_2".to_owned()),
            extent: Extent {
                cells: 12,
                people: 9001,
            },
            tiles: 300,
        }],
        states: vec![State {
            id: "state_3".to_owned(),
            name: "Riverhold".to_owned(),
            full_name: "Free City of Riverhold".to_owned(),
            form: StateForm::Union,
            culture: Some("culture_7".to_owned()),
            capital: Some("settlement_9".to_owned()),
            stances: vec![
                ("state_1".to_owned(), Stance::Vassal),
                ("state_4".to_owned(), Stance::Suzerain),
            ],
            extent: Extent {
                cells: 40,
                people: 61_234,
            },
            tiles: 4620,
        }],
    }
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
version = 9

[recipe]
source = ""
region = {{ x = 3, y = 4, width = 35, height = 33 }}
scale = 2
seed = 7
detail = {{ strength = 0.25, wavelength = 8.00, octaves = 4 }}
coast = {{ shallows = 10.00, specks = 4 }}
levels = {{ spacing = 5.00, specks = 4 }}

[recipe.foliage]
scale = 1.00
spacing = {{ tree = 2.00, shrub = 1.50, grass = 1.00 }}

[recipe.foliage.chances]
marine = {{ tree = 0.00, shrub = 0.00, grass = 0.00 }}
hot_desert = {{ tree = 0.00, shrub = 0.05, grass = 0.02 }}
cold_desert = {{ tree = 0.00, shrub = 0.05, grass = 0.05 }}
savanna = {{ tree = 0.05, shrub = 0.10, grass = 0.40 }}
grassland = {{ tree = 0.02, shrub = 0.05, grass = 0.50 }}
tropical_seasonal_forest = {{ tree = 0.40, shrub = 0.15, grass = 0.10 }}
temperate_deciduous_forest = {{ tree = 0.45, shrub = 0.15, grass = 0.10 }}
tropical_rainforest = {{ tree = 0.60, shrub = 0.20, grass = 0.05 }}
temperate_rainforest = {{ tree = 0.55, shrub = 0.20, grass = 0.05 }}
taiga = {{ tree = 0.45, shrub = 0.10, grass = 0.05 }}
tundra = {{ tree = 0.00, shrub = 0.05, grass = 0.15 }}
glacier = {{ tree = 0.00, shrub = 0.00, grass = 0.00 }}
wetland = {{ tree = 0.10, shrub = 0.15, grass = 0.40 }}

[tiles]
bytes = {}
fingerprint = "{}"
"#,
        tiles.len(),
        fingerprint_of(tiles)
    );
    assert_eq!(header, expected);
    assert_eq!(MAP_VERSION, 9);
}

#[test]
fn the_foliage_is_kept_every_biome_and_kind_its_own() {
    let mut recipe = recipe(0, 0, 3, 2, 1);
    let foliage = &mut recipe.foliage;
    foliage.scale = Fixed::from_hundredths(2_50);
    foliage.spacing = PerKind {
        tree: Fixed::from_hundredths(3_00),
        shrub: Fixed::from_hundredths(2_25),
        grass: Fixed::from_hundredths(75),
    };
    // Every biome's chances differ from every other's, and each kind's from the others'.
    for (i, chances) in foliage.chances.iter_mut().enumerate() {
        let i = i as i64;
        *chances = PerKind {
            tree: Fixed::from_hundredths(i),
            shrub: Fixed::from_hundredths(20 + i),
            grass: Fixed::from_hundredths(40 + i),
        };
    }
    let map = WorldMap::new(recipe, varied).expect("the recipe makes a map");
    let bytes = write_map(&map);
    let (header, _) = split(&bytes);
    assert!(
        header.contains("scale = 2.50\nspacing = { tree = 3.00, shrub = 2.25, grass = 0.75 }\n")
    );
    assert!(header.contains("\ntaiga = { tree = 0.09, shrub = 0.29, grass = 0.49 }\n"));
    assert_eq!(read_map(&bytes), Ok(map));
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
fn a_maps_suggestions_read_back_exactly() {
    let read = read_map(&write_map(&hand_made())).expect("it reads");
    assert_eq!(read.suggestions(), &hand_made_suggestions());
}

#[test]
fn a_maps_dismissals_read_back_even_those_not_on_it() {
    let read = read_map(&write_map(&hand_made())).expect("it reads");
    assert_eq!(read.dismissed(), &hand_made_dismissals());
    assert_eq!(read.dismissed_by("poi_0"), Some("poi_0"));
    assert_eq!(read.not_on_the_map(), ["route_7"]);
}

#[test]
fn every_kind_and_figure_of_a_suggestion_reads_back() {
    let kinds = [
        SettlementKind::Generic,
        SettlementKind::Naval,
        SettlementKind::Lake,
        SettlementKind::Highland,
        SettlementKind::River,
        SettlementKind::Nomadic,
        SettlementKind::Hunting,
    ];
    let settlements = (0u8..32)
        .map(|bits| Settlement {
            id: format!("settlement_{bits}"),
            name: format!("Place {bits}"),
            at: placed(u32::from(bits), u32::from(bits) * 2, [bits, 255 - bits]),
            people: u64::from(bits) * 1_000_003,
            kind: kinds[usize::from(bits) % 7],
            capital: bits & 1 != 0,
            port: bits & 2 == 0,
            buildings: Buildings {
                citadel: bits & 1 != 0,
                walls: bits & 2 != 0,
                plaza: bits & 4 != 0,
                temple: bits & 8 != 0,
                shanty: bits & 16 != 0,
            },
        })
        .collect();
    let routes = [RouteKind::Road, RouteKind::Trail, RouteKind::SeaRoute]
        .into_iter()
        .zip(0u32..)
        .map(|(kind, i)| Route {
            id: format!("route_{i}"),
            name: (i != 1).then(|| format!("Way {i}")),
            kind,
            points: vec![
                placed(i, 0, [1, 2]),
                placed(0, i, [3, 4]),
                placed(i, i, [5, 6]),
            ],
        })
        .collect();
    let points = (0u32..3)
        .map(|i| PointOfInterest {
            id: format!("poi_{i}"),
            kind: ["volcanoes", "ruins", "bridges"][i as usize].to_owned(),
            icon: format!("glyph-{i}"),
            name: (i != 0).then(|| "Ash Peak".to_owned()),
            note: (i != 2).then(|| "Dormant.".to_owned()),
            at: placed(69, 65 - i, [255, 0]),
        })
        .collect();
    // W6e: every culture type, religion type, state form and stance, and links and deities
    // present and not.
    let cultures = kinds
        .into_iter()
        .zip(1u32..)
        .map(|(kind, i)| Culture {
            id: format!("culture_{i}"),
            name: format!("Folk {i}"),
            kind,
            extent: Extent {
                cells: i * 7,
                people: u64::from(i) * 1_000_003,
            },
            tiles: i,
        })
        .collect();
    let religions = [
        ReligionKind::Folk,
        ReligionKind::Organized,
        ReligionKind::Cult,
        ReligionKind::Heresy,
    ]
    .into_iter()
    .zip(1u32..)
    .map(|(kind, i)| Religion {
        id: format!("religion_{i}"),
        name: format!("Faith {i}"),
        kind,
        form: format!("Form {i}"),
        deity: (i % 2 == 0).then(|| format!("God {i}")),
        culture: (i != 3).then(|| format!("culture_{i}")),
        extent: Extent {
            cells: i,
            people: u64::from(i) * 17,
        },
        tiles: 100 + i,
    })
    .collect();
    let stances = [
        Stance::Ally,
        Stance::Friendly,
        Stance::Neutral,
        Stance::Suspicion,
        Stance::Enemy,
        Stance::Unknown,
        Stance::Rival,
        Stance::Vassal,
        Stance::Suzerain,
    ];
    let states = [
        StateForm::Monarchy,
        StateForm::Republic,
        StateForm::Union,
        StateForm::Theocracy,
        StateForm::Anarchy,
    ]
    .into_iter()
    .zip(1u32..)
    .map(|(form, i)| State {
        id: format!("state_{i}"),
        name: format!("Realm {i}"),
        full_name: format!("The Realm of {i}"),
        form,
        culture: (i != 2).then(|| format!("culture_{i}")),
        capital: (i != 4).then(|| format!("settlement_{i}")),
        stances: (1u32..=5)
            .filter(|&other| other != i)
            .map(|other| {
                let stance = stances[((i + other) % 9) as usize];
                (format!("state_{other}"), stance)
            })
            .chain((i == 1).then(|| ("state_9".to_owned(), Stance::Suzerain)))
            .chain((i == 2).then(|| ("state_8".to_owned(), Stance::Ally)))
            .chain((i == 3).then(|| ("state_7".to_owned(), Stance::Friendly)))
            .collect(),
        extent: Extent {
            cells: i * 3,
            people: u64::from(i) * 101,
        },
        tiles: 1000 + i,
    })
    .collect();
    let suggestions = Suggestions {
        settlements,
        routes,
        points,
        cultures,
        religions,
        states,
    };
    let mut map = WorldMap::new(recipe(3, 4, 35, 33, 2), varied).expect("a map");
    map.suggest(suggestions.clone()).expect("on the map");
    let read = read_map(&write_map(&map)).expect("it reads");
    assert_eq!(read.suggestions(), &suggestions);
    assert_eq!(read, map);
}

#[test]
fn suggestions_the_map_refuses_are_damaged_naming_why() {
    // The hand-made map's settlement renamed, byte for byte, to an id that isn't valid.
    let bytes = write_map(&hand_made());
    let (header, tiles) = split(&bytes);
    let (chunks, overrides, suggestions, dismissed): (Vec<Vec<u8>>, Vec<u8>, Vec<u8>, Vec<u8>) =
        postcard::from_bytes(tiles).expect("the body");
    let mut unpacked = zstd::stream::decode_all(suggestions.as_slice()).expect("unpacked");
    let id = b"settlement_9";
    let found = unpacked
        .windows(id.len())
        .position(|window| window == id)
        .expect("the settlement's id is written");
    unpacked[found] = b'S';
    let repacked = zstd::stream::encode_all(unpacked.as_slice(), 3).expect("packed");
    let body = postcard::to_allocvec(&(chunks, overrides, repacked, dismissed)).expect("the body");
    let header = header
        .replace(
            &format!("bytes = {}", tiles.len()),
            &format!("bytes = {}", body.len()),
        )
        .replace(&fingerprint_of(tiles), &fingerprint_of(&body));
    let mut damaged = header.into_bytes();
    damaged.push(0);
    damaged.extend_from_slice(&body);
    assert_eq!(
        refusal(&damaged),
        "this map's tiles are damaged: the suggestions: the suggestion id \"Settlement_9\" isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores"
    );
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
    for found in [8, 10] {
        let bytes = with_header(
            &write_map(&hand_made()),
            "version = 9",
            &format!("version = {found}"),
        );
        assert_eq!(
            refusal(&bytes),
            format!("this map is version {found}, but this build reads version 9")
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
fn the_recipes_levels_are_kept() {
    let mut recipe = recipe(0, 0, 3, 2, 1);
    recipe.levels = Levels {
        spacing: Fixed::from_hundredths(80_00),
        specks: 0,
    };
    let map = WorldMap::new(recipe, varied).expect("the recipe makes a map");
    let bytes = write_map(&map);
    let (header, _) = split(&bytes);
    assert!(
        header.contains("levels = { spacing = 80.00, specks = 0 }\n"),
        "{header}"
    );
    assert_eq!(read_map(&bytes), Ok(map));
}

#[test]
fn levels_outside_their_ranges_are_refused_with_why() {
    let bytes = with_header(&write_map(&hand_made()), "spacing = 5.00", "spacing = 0.99");
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the levels' spacing must be 1.00 to 80.00 of Azgaar's heights, not 0.99"
    );
    let bytes = with_header(
        &write_map(&hand_made()),
        "spacing = 5.00, specks = 4",
        "spacing = 5.00, specks = 4097",
    );
    assert_eq!(
        refusal(&bytes),
        "this map's recipe can't make a map: the levels' specks must be 0 to 4096 tiles, not 4097"
    );
    let bytes = with_header(&write_map(&hand_made()), "spacing = 5.00, ", "");
    let refused = refusal(&bytes);
    assert!(refused.contains("spacing"), "{refused}");
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
        level: next(256) as u8,
        terrain: TERRAINS[next(9) as usize],
        water: match next(3) {
            0 => None,
            1 => Some(Water::Lake),
            _ => Some(Water::River {
                flow: next(u64::from(u32::MAX)) as u32,
            }),
        },
        biome: BIOMES[next(13) as usize],
        temperature: next(256) as u8 as i8,
        precipitation: next(65_536) as u16,
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
        places in proptest::collection::vec((any::<u32>(), any::<u32>(), any::<u8>()), 0..6),
    ) {
        let mut map = WorldMap::new(recipe(0, 0, width, height, 1), |pos| any_tile(seed, pos))
            .expect("the recipe makes a map");
        for (x, y, bits) in edits {
            let pos = at(x % width, y % height);
            let tile = any_tile(bits, pos);
            let edit = Override {
                elevation: (bits & 1 != 0).then_some(tile.elevation),
                level: (bits & 32 != 0).then_some(tile.level),
                terrain: (bits & 2 != 0).then_some(tile.terrain),
                water: (bits & 4 != 0).then_some(tile.water),
                biome: (bits & 8 != 0).then_some(tile.biome),
                temperature: (bits & 64 != 0).then_some(tile.temperature),
                precipitation: (bits & 128 != 0).then_some(tile.precipitation),
                plant: (bits & 16 != 0).then_some(tile.plant),
            };
            map.edit(pos, edit).expect("on the map");
        }
        // Each place a settlement, a point of interest, and a route's point.
        let at = |(x, y, offset): (u32, u32, u8)| placed(x % width, y % height, [offset, !offset]);
        let suggestions = Suggestions {
            settlements: places.iter().zip(0u64..).map(|(&place, i)| Settlement {
                id: format!("settlement_{i}"),
                name: format!("Place {i}"),
                at: at(place),
                people: i * 7919,
                kind: SettlementKind::Naval,
                capital: place.2 & 1 != 0,
                port: place.2 & 2 != 0,
                buildings: Buildings::default(),
            }).collect(),
            routes: (places.len() >= 2).then(|| Route {
                id: "route_0".to_owned(),
                name: None,
                kind: RouteKind::Road,
                points: places.iter().map(|&place| at(place)).collect(),
            }).into_iter().collect(),
            points: places.iter().zip(0u64..).map(|(&place, i)| PointOfInterest {
                id: format!("poi_{i}"),
                kind: "ruins".to_owned(),
                icon: String::new(),
                name: None,
                note: Some(format!("Note {i}")),
                at: at(place),
            }).collect(),
            // W6e: a culture for each place, covering some of the map's tiles.
            cultures: places.iter().zip(0u64..).map(|(&place, i)| Culture {
                id: format!("culture_{i}"),
                name: format!("Folk {i}"),
                kind: SettlementKind::Lake,
                extent: Extent { cells: place.0, people: i * 104_729 },
                tiles: place.1 % (width * height) + 1,
            }).collect(),
            ..Suggestions::default()
        };
        map.suggest(suggestions).expect("on the map");
        prop_assert_eq!(read_map(&write_map(&map)), Ok(map));
    }
}

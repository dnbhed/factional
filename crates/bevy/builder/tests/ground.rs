//! The terrain in the builder (W7a): the map `builder.toml` names, read at start, drawn on a
//! dual grid in layers by terrain, its middle at the plane's origin. Every expected value is
//! worked by hand from PLAN.md's acceptance.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::math::Vec2;
use bevy::time::TimeUpdateStrategy;
use factional_bevy::FactionalPlugin;
use factional_builder::{
    BuilderPlugin, BuilderProblems, Ground, MapChoice, Patch, TILE, colour, patches, place,
    read_ground, shape_pixels, tileset,
};
use factional_core::Fixed;
use factional_reputation::CharacterId;
use factional_worldgen::{
    Biome, Coast, Detail, Foliage, Levels, Recipe, Region, Size, Terrain, Tile, TilePos, WorldMap,
};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn player() -> Vec<CharacterId> {
    vec![CharacterId::new("player").expect("an id")]
}

/// A map of `width` × `height` tiles, each the terrain `terrain` gives it.
fn map(width: u32, height: u32, terrain: impl Fn(TilePos) -> Terrain) -> WorldMap {
    let recipe = Recipe {
        source: String::new(),
        region: Region {
            x: 0,
            y: 0,
            size: Size { width, height },
        },
        scale: 1,
        seed: 0,
        detail: Detail::default(),
        coast: Coast::default(),
        levels: Levels::default(),
        foliage: Foliage::default(),
    };
    WorldMap::new(recipe, |at| Tile {
        elevation: Fixed::ZERO,
        level: 0,
        terrain: terrain(at),
        water: None,
        biome: Biome::Grassland,
        temperature: 10,
        precipitation: 1000,
        plant: None,
        culture: None,
        religion: None,
        state: None,
    })
    .expect("a map")
}

/// The shape a patch shows at drawn tile (`i`, `j`) of its chunk, counted from the top left.
fn shape(patch: &Patch, i: usize, j: usize) -> Option<u8> {
    patch.shapes[j * 64 + i]
}

/// Every drawn tile a patch shows, as (i, j, shape), row by row.
fn shown(patch: &Patch) -> Vec<(usize, usize, u8)> {
    (0..64)
        .flat_map(|j| (0..64).map(move |i| (i, j)))
        .filter_map(|(i, j)| shape(patch, i, j).map(|shape| (i, j, shape)))
        .collect()
}

#[test]
fn builder_toml_names_the_map_and_how_many_points_a_tile_is() {
    let placed = place(
        Some("[map]\nfile = \"world/riverhold.map\"\ntile = 16.0\n"),
        &player(),
    );
    assert_eq!(
        placed.map,
        MapChoice {
            file: Some("world/riverhold.map".to_owned()),
            tile: 16.0,
        }
    );
    assert!(placed.problems.is_empty(), "{:?}", placed.problems);
}

#[test]
fn with_no_map_named_there_is_none_and_a_tile_is_32_points() {
    let placed = place(Some("[map]\nfile = \"world/riverhold.map\"\n"), &player());
    assert_eq!(placed.map.tile, 32.0);
    assert_eq!(TILE, 32.0);
    let placed = place(Some("[positions]\n"), &player());
    assert_eq!(
        placed.map,
        MapChoice {
            file: None,
            tile: 32.0
        }
    );
    assert_eq!(place(None, &player()).map.file, None);
}

#[test]
fn a_tile_of_zero_or_less_is_said_and_32_points_used() {
    for tile in ["0.0", "-4.0"] {
        let placed = place(
            Some(&format!("[map]\nfile = \"world/x.map\"\ntile = {tile}\n")),
            &player(),
        );
        assert_eq!(placed.map.tile, 32.0);
        assert_eq!(placed.map.file.as_deref(), Some("world/x.map"));
        assert_eq!(
            placed.problems,
            ["builder.toml: map.tile must be above 0, so it's 32"]
        );
    }
}

#[test]
fn a_map_file_that_isnt_there_is_said_and_no_map_drawn() {
    let choice = MapChoice {
        file: Some("world/nope.map".to_owned()),
        tile: 32.0,
    };
    assert_eq!(
        read_ground(&sample(), &choice),
        Err(
            "builder.toml: map.file names world/nope.map, which isn't there, so no map is drawn"
                .to_owned()
        )
    );
}

#[test]
fn a_file_that_isnt_a_map_is_said_in_the_maps_own_words() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ground-not-a-map");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("world")).expect("a directory");
    fs::write(dir.join("world/bad.map"), "not a map").expect("written");
    let choice = MapChoice {
        file: Some("world/bad.map".to_owned()),
        tile: 32.0,
    };
    assert_eq!(
        read_ground(&dir, &choice),
        Err("world/bad.map: this isn't a Factional map, so no map is drawn".to_owned())
    );
}

#[test]
fn a_map_file_that_cant_be_read_is_said_with_why() {
    // The sample's `world` is a directory: there, but not a file that can be read.
    let choice = MapChoice {
        file: Some("world".to_owned()),
        tile: 32.0,
    };
    let said = read_ground(&sample(), &choice).expect_err("a directory isn't a map");
    assert!(
        said.starts_with("builder.toml: map.file world can't be read: "),
        "{said}"
    );
    assert!(said.ends_with(", so no map is drawn"), "{said}");
}

#[test]
fn no_map_named_is_no_map() {
    assert_eq!(read_ground(&sample(), &MapChoice::default()), Ok(None));
}

/// The builder on `dir`, a second of game time a frame.
fn app(dir: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: dir.to_owned(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: dir.to_owned(),
            player: "player".to_owned(),
        });
    app.update();
    app
}

#[test]
fn the_samples_map_is_read_at_start_80_by_80_tiles_at_32_points() {
    let app = app(&sample());
    let ground = app.world().get_resource::<Ground>().expect("the map");
    assert_eq!(
        ground.map.size(),
        Size {
            width: 80,
            height: 80
        }
    );
    assert_eq!(ground.tile, 32.0);
    assert!(app.world().resource::<BuilderProblems>().0.is_empty());
}

#[test]
fn a_map_that_cant_be_read_is_said_and_the_tokens_drawn_without_it() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ground-missing-map");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    for entry in fs::read_dir(sample()).expect("the sample") {
        let path = entry.expect("an entry").path();
        if path.is_file() {
            fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
        }
    }
    let mut app = app(&dir);
    assert!(app.world().get_resource::<Ground>().is_none());
    assert_eq!(
        app.world().resource::<BuilderProblems>().0,
        ["builder.toml: map.file names world/riverhold.map, which isn't there, so no map is drawn"]
    );
    let tokens = app
        .world_mut()
        .query::<&factional_builder::Token>()
        .iter(app.world())
        .count();
    assert_eq!(tokens, 6);
}

#[test]
fn each_drawn_tile_shows_the_shape_of_each_terrain_at_its_corners_in_layers() {
    use Terrain::*;
    let rows = [[Grass, Sand, DeepWater], [Grass, Grass, Rock]];
    let ground = Ground {
        map: map(3, 2, |at| rows[at.y as usize][at.x as usize]),
        tile: 32.0,
    };
    let patches = patches(&ground);
    let terrains: Vec<Terrain> = patches.iter().map(|patch| patch.terrain).collect();
    assert_eq!(terrains, [DeepWater, Sand, Grass, Rock]);
    for patch in &patches {
        assert_eq!(patch.centre, Vec2::new(960.0, -976.0));
        assert_eq!(patch.shapes.len(), 64 * 64);
    }
    assert_eq!(
        shown(&patches[0]),
        [(2, 0, 15), (3, 0, 15), (2, 1, 15), (3, 1, 15)]
    );
    assert_eq!(
        shown(&patches[1]),
        [(1, 0, 15), (2, 0, 5), (1, 1, 15), (2, 1, 13)]
    );
    assert_eq!(
        shown(&patches[2]),
        [
            (0, 0, 15),
            (1, 0, 5),
            (0, 1, 15),
            (1, 1, 13),
            (2, 1, 12),
            (0, 2, 15),
            (1, 2, 15),
            (2, 2, 15)
        ]
    );
    assert_eq!(
        shown(&patches[3]),
        [(2, 1, 8), (3, 1, 12), (2, 2, 10), (3, 2, 15)]
    );
}

#[test]
fn drawn_tiles_are_chunked_64_a_side_each_chunk_centred_where_its_tiles_lie() {
    // 100 × 70 tiles are drawn as 101 × 71, in 2 × 2 chunks; drawn tile (i, j) is centred at
    // ((i − 50) × 32, (35 − j) × 32), so a chunk's middle is at its 31.5th tile each way.
    let ground = Ground {
        map: map(100, 70, |_| Terrain::Grass),
        tile: 32.0,
    };
    let patches = patches(&ground);
    let centres: Vec<Vec2> = patches.iter().map(|patch| patch.centre).collect();
    assert_eq!(
        centres,
        [
            Vec2::new(-592.0, 112.0),
            Vec2::new(1456.0, 112.0),
            Vec2::new(-592.0, -1936.0),
            Vec2::new(1456.0, -1936.0),
        ]
    );
    let counts: Vec<usize> = patches.iter().map(|patch| shown(patch).len()).collect();
    assert_eq!(counts, [64 * 64, 37 * 64, 64 * 7, 37 * 7]);
    assert!(
        patches
            .iter()
            .flat_map(shown)
            .all(|(_, _, shape)| shape == 15)
    );
    assert_eq!(shape(&patches[3], 36, 6), Some(15));
    assert_eq!(shape(&patches[3], 37, 6), None);
    assert_eq!(shape(&patches[3], 36, 7), None);
}

#[test]
fn a_tile_size_spaces_the_chunks() {
    let ground = Ground {
        map: map(3, 2, |_| Terrain::Sand),
        tile: 16.0,
    };
    assert_eq!(patches(&ground)[0].centre, Vec2::new(480.0, -488.0));
}

/// The pixels of a shape at 4 pixels a side, as (column, row).
fn covered(shape: u8) -> Vec<(u32, u32)> {
    let pixels = shape_pixels(shape, 4);
    assert_eq!(pixels.len(), 16);
    (0..16u32)
        .filter(|&p| pixels[p as usize])
        .map(|p| (p % 4, p / 4))
        .collect()
}

#[test]
fn each_shape_covers_the_pixels_whose_blend_of_its_corners_is_at_least_a_half() {
    assert_eq!(covered(0), []);
    assert_eq!(covered(15).len(), 16);
    assert_eq!(covered(1), [(0, 0), (1, 0), (0, 1)]);
    assert_eq!(covered(2), [(2, 0), (3, 0), (3, 1)]);
    assert_eq!(covered(4), [(0, 2), (0, 3), (1, 3)]);
    assert_eq!(covered(8), [(3, 2), (2, 3), (3, 3)]);
    assert_eq!(
        covered(3),
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1)
        ]
    );
    let saddle = covered(9);
    assert!(saddle.contains(&(1, 1)) && saddle.contains(&(2, 2)));
    assert!(!saddle.contains(&(2, 1)) && !saddle.contains(&(1, 2)));
    // Three corners: all but the pixels nearest the one left out.
    assert_eq!(covered(14).len(), 13);
    assert!(!covered(14).contains(&(0, 0)));
}

#[test]
fn the_tileset_is_the_16_shapes_white_where_covered_and_clear_elsewhere() {
    let side = 4;
    let set = tileset(side);
    assert_eq!(set.len(), 16 * 16 * 4);
    for shape in 0..16u8 {
        let pixels = shape_pixels(shape, side);
        for (p, covered) in pixels.iter().enumerate() {
            let at = (usize::from(shape) * 16 + p) * 4;
            let expected = if *covered {
                [255, 255, 255, 255]
            } else {
                [255, 255, 255, 0]
            };
            assert_eq!(set[at..at + 4], expected, "shape {shape}, pixel {p}");
        }
    }
}

#[test]
fn each_terrain_has_a_placeholder_colour_of_its_own() {
    let colours: Vec<[u8; 3]> = Terrain::ALL
        .iter()
        .map(|&terrain| colour(terrain))
        .collect();
    for (i, a) in colours.iter().enumerate() {
        for b in &colours[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

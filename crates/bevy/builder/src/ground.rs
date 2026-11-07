//! The map under the tokens (W7a, P-150, DESIGN.md §21): the map `builder.toml` names, read
//! at start, its middle at the plane's origin; drawn on a dual grid, each drawn tile on a
//! corner of the map's tiles, half a tile off, in layers by terrain, each layer's shape the
//! corners of that terrain or a later one. What's drawn is worked out here, headless; the app
//! hands it to Bevy's `TilemapChunk`.

use std::io::ErrorKind;
use std::path::Path;

use bevy::ecs::resource::Resource;
use bevy::math::Vec2;
use factional_content::read_map;
use factional_worldgen::{CHUNK, Terrain, TilePos, WorldMap};

/// How many points a tile is across, unless `builder.toml` says.
pub const TILE: f32 = 32.0;

/// How many pixels a side each of the 16 shapes is drawn in.
pub const SHAPE_SIDE: u32 = 16;

/// `builder.toml`'s `[map]`: the map's file, from the content directory, and how many points
/// a tile is across. Kept as a resource, the map the wizard saves set as its file (W8a).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MapChoice {
    pub file: Option<String>,
    pub tile: f32,
}

impl Default for MapChoice {
    fn default() -> Self {
        MapChoice {
            file: None,
            tile: TILE,
        }
    }
}

/// The map drawn under the tokens, and how many points a tile is across.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Ground {
    pub map: WorldMap,
    pub tile: f32,
}

/// The map `choice` names, read from `content`; none if it names none. What's wrong said, as
/// the loader would say it.
pub fn read_ground(content: &Path, choice: &MapChoice) -> Result<Option<Ground>, String> {
    let Some(file) = &choice.file else {
        return Ok(None);
    };
    let bytes = match std::fs::read(content.join(file)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "builder.toml: map.file names {file}, which isn't there, so no map is drawn"
            ));
        }
        Err(error) => {
            return Err(format!(
                "builder.toml: map.file {file} can't be read: {error}, so no map is drawn"
            ));
        }
    };
    let map = read_map(&bytes).map_err(|error| format!("{file}: {error}, so no map is drawn"))?;
    Ok(Some(Ground {
        map,
        tile: choice.tile,
    }))
}

/// One terrain's layer of one chunk of drawn tiles: where its middle is on the plane, and each
/// drawn tile's shape, if the layer shows there, row by row from the top, `CHUNK` × `CHUNK`.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub terrain: Terrain,
    pub centre: Vec2,
    pub shapes: Vec<Option<u8>>,
}

/// Every chunk's layers, chunk by chunk row by row, each chunk's in the terrain's order.
///
/// Drawn tile (i, j), for i from 0 to the map's width and j from 0 to its height, sits on the
/// corner its four map tiles share: (i − 1, j − 1), (i, j − 1), (i − 1, j) and (i, j), each
/// off the map being the nearest on it. A terrain's layer shows there when one of the four is
/// that terrain, its shape the corners of that terrain or a later one: top left 1, top right 2,
/// bottom left 4, bottom right 8.
pub fn patches(ground: &Ground) -> Vec<Patch> {
    let size = ground.map.size();
    let (across, down) = (size.width + 1, size.height + 1);
    // Each tile's terrain read once, overrides applied, row by row.
    let terrains: Vec<Option<Terrain>> = (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| TilePos { x, y }))
        .map(|at| ground.map.tile(at).map(|tile| tile.terrain))
        .collect();
    let terrain = |x: u32, y: u32| {
        let (x, y) = (x.min(size.width - 1), y.min(size.height - 1));
        terrains[(y * size.width + x) as usize]
    };
    let mut patches = Vec::new();
    for chunk_y in 0..down.div_ceil(CHUNK) {
        for chunk_x in 0..across.div_ceil(CHUNK) {
            let mut layers: Vec<Vec<Option<u8>>> =
                vec![vec![None; (CHUNK * CHUNK) as usize]; Terrain::ALL.len()];
            for row in 0..CHUNK {
                for column in 0..CHUNK {
                    let (i, j) = (chunk_x * CHUNK + column, chunk_y * CHUNK + row);
                    if i >= across || j >= down {
                        continue;
                    }
                    let (left, top) = (i.saturating_sub(1), j.saturating_sub(1));
                    let corners = [
                        terrain(left, top),
                        terrain(i, top),
                        terrain(left, j),
                        terrain(i, j),
                    ];
                    for (layer, shapes) in Terrain::ALL.iter().zip(&mut layers) {
                        if !corners.contains(&Some(*layer)) {
                            continue;
                        }
                        let shape = corners
                            .iter()
                            .zip([1, 2, 4, 8])
                            .filter(|(corner, _)| corner.is_some_and(|corner| corner >= *layer))
                            .map(|(_, bit)| bit)
                            .sum();
                        shapes[(row * CHUNK + column) as usize] = Some(shape);
                    }
                }
            }
            // The chunk's middle lies at its tiles' middle, CHUNK / 2 − ½ tiles in each way.
            let middle = |first: u32, tiles: u32| {
                first as f32 + (CHUNK as f32 - 1.0) / 2.0 - tiles as f32 / 2.0
            };
            let centre = Vec2::new(
                middle(chunk_x * CHUNK, size.width),
                -middle(chunk_y * CHUNK, size.height),
            ) * ground.tile;
            for (terrain, shapes) in Terrain::ALL.into_iter().zip(layers) {
                if shapes.iter().any(Option::is_some) {
                    patches.push(Patch {
                        terrain,
                        centre,
                        shapes,
                    });
                }
            }
        }
    }
    patches
}

/// Which of a `side` × `side` square's pixels `shape` covers, row by row from the top: those
/// whose centre's bilinear blend of the four corners, each 1 if in the shape, is at least ½.
/// Worked in whole numbers, each weight in half pixels.
pub fn shape_pixels(shape: u8, side: u32) -> Vec<bool> {
    let corner = |bit: u8| u32::from(shape & bit != 0);
    let (top_left, top_right, bottom_left, bottom_right) =
        (corner(1), corner(2), corner(4), corner(8));
    let whole = 2 * side;
    (0..side)
        .flat_map(|row| (0..side).map(move |column| (column, row)))
        .map(|(column, row)| {
            let (a, c) = (2 * column + 1, 2 * row + 1);
            let (b, d) = (whole - a, whole - c);
            let blend =
                top_left * b * d + top_right * a * d + bottom_left * b * c + bottom_right * a * c;
            2 * blend >= whole * whole
        })
        .collect()
}

/// The 16 shapes, each `side` × `side` pixels of RGBA, white where it covers and clear where
/// it doesn't, one after another.
pub fn tileset(side: u32) -> Vec<u8> {
    (0..16)
        .flat_map(|shape| shape_pixels(shape, side))
        .flat_map(|covered| [255, 255, 255, if covered { 255 } else { 0 }])
        .collect()
}

/// Each terrain's placeholder colour, in sRGB.
pub fn colour(terrain: Terrain) -> [u8; 3] {
    match terrain {
        Terrain::DeepWater => [32, 64, 128],
        Terrain::ShallowWater => [64, 128, 192],
        Terrain::Sand => [222, 204, 150],
        Terrain::Grass => [112, 168, 80],
        Terrain::ForestFloor => [56, 104, 56],
        Terrain::Marsh => [96, 120, 88],
        Terrain::Rock => [128, 120, 112],
        Terrain::Snow => [236, 240, 244],
        Terrain::Ice => [184, 216, 232],
    }
}

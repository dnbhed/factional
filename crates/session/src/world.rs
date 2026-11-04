//! `world info <file>` (W1b): what a map file holds, read whole by `factional-content`.

use std::path::Path;

use factional_content::read_map;

use crate::check::count;

/// What `world info <file>` says of the map in `file`, read from `base`: its size, chunks and
/// overrides, its length, and its recipe; or why it can't be read.
pub fn world_info(base: &Path, file: &str) -> Result<String, String> {
    let bytes =
        std::fs::read(base.join(file)).map_err(|error| format!("cannot read {file}: {error}"))?;
    let map = read_map(&bytes).map_err(|error| error.to_string())?;
    let name = Path::new(file)
        .file_name()
        .map_or(file.into(), |name| name.to_string_lossy());
    let (size, chunks) = (map.size(), map.chunk_count());
    let recipe = map.recipe();
    let region = recipe.region;
    let source = match recipe.source.as_str() {
        "" => "hand-made".to_owned(),
        source => format!("source {source}"),
    };
    Ok(format!(
        "{name}: {} × {} tiles in {} × {} chunks, {}; {} bytes\n\
         recipe: {source}, source pixels ({}, {}) to ({}, {}), {} a source pixel, seed {}",
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
    ))
}

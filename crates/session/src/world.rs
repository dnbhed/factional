//! `world info <file>` (W1b): what a map file holds, read whole by `factional-content`; and
//! `world source <file>` (W2a): what an Azgaar export holds, or every problem in it.

use std::path::Path;

use factional_content::{AzgaarError, fingerprint_of, read_azgaar, read_map};
use factional_worldgen::Feature;

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

/// What `world source <file>` says of the Azgaar export in `file`, read from `base`: its name,
/// version and size, its cells, vertices and features, and its fingerprint; or why it can't
/// be read, each problem on a line.
pub fn world_source(base: &Path, file: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(base.join(file))
        .map_err(|error| format!("cannot read {file}: {error}"))?;
    let name = Path::new(file)
        .file_name()
        .map_or(file.into(), |name| name.to_string_lossy());
    let source = read_azgaar(&text).map_err(|error| match error {
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
    })?;
    let features = |kind: fn(&Feature) -> bool| source.features.iter().filter(|f| kind(f)).count();
    Ok(format!(
        "{name}: {}, from Azgaar {}, {} × {} pixels\n\
         {}, {}; {}, {} and {}; fingerprint {}",
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
        fingerprint_of(text.as_bytes()),
    ))
}

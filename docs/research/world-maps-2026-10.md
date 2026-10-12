# World maps from generated worlds: research (2026-10-12)

How a designer could start a game's world from a map made in Azgaar's Fantasy Map Generator:
import it, polish it into a playable 2D tile map, dress it with foliage, and refine it by hand.
This is the research behind W0's design pass; what's decided will be in DECISIONS.md and
DESIGN.md, and where they differ from this note, they win.

The user's direction: the designer generates a map, downloads its JSON, and a wizard imports
the layers they choose, the most fundamental first (terrain, then water, climate and biomes,
later peoples and settlements). The application then polishes the terrain and adds foliage,
for the designer to edit: a quick canvas to work from. Settlements and trade routes are kept
as suggestions to build, not built. The goal is playable top-down 2D maps, as in the original
Zelda and Pokémon, and 3D maps later. The map's data is stored in the most efficient format
available. Reputation rules will refer to places.

## What an Azgaar export holds

Read from a real export, *Menayland* (generator version 1.154.0, 7 MB, "Full" JSON):

- **Two meshes of Voronoi cells.** `grid` is the raw heightmap: 9,990 cells on a jittered
  135 × 74 lattice, each with a height, temperature and precipitation. `pack` is the refined
  mesh everything else hangs on: 5,717 cells, deep ocean merged, coastlines finer.
- **Each `pack` cell** has its height `h` (0–100, sea level at 20), its neighbours `c`
  (usually five to seven), its polygon's corners `v`, its centre `p` and area, the feature
  it's in `f` (an ocean, lake or landmass), its distance from the coast in rings `t`
  (positive on land, negative at sea), water flux `fl`, river `r`, biome, and the culture,
  religion, settlement and route links laid on it. Vertices name their three neighbouring
  vertices and cells, so the mesh's dual graph is complete.
- **Height in real units** is a display formula, not data: (h − 18)² × 3.281 feet with the
  map's exponent of 2. The export's own volcano note (14,292 ft at h = 84) confirms it.
- **Rivers** are paths of cells with source, mouth, discharge, width, parent and basin;
  **features** are oceans, lakes (with a surface height) and landmasses; **routes** are
  trails and sea lanes as polylines over cells.
- **Peoples and places:** cultures, religions, states and provinces (Menayland has none),
  163 settlements with population, port, walls and temple, 47 markers (ruins, dungeons,
  monsters, inns), zones (plagues, famine, floods), and a newer trade layer of goods,
  markets and deals.
- **The scale:** 1,470 × 801 map pixels at 2 miles each, about 2,900 × 1,600 miles. A
  `grid` cell is about 22 miles across.
- **No geology:** no rock types or strata. The trade layer places goods by height
  expressions (iron where height ≥ 60), the nearest thing to it.
- **The format is the app's own.** The export menu offers Full, Minimal and Cells JSON,
  described as a replacement for an API; there's no published schema, and the source
  (the export module) is the reference. Fields change between versions. Arrays mix types in
  places: `features[0]` is `0`, `provinces` is `[0]`, some `origins` are `null`.

**Its licence:** MIT, © Max Haniyeu (Azgaar). The licence text says maps made with it, their
images, screenshots and videos, may be produced without restriction, and sold.

## What a playable 2D map needs

- **Scale is a choice.** The original Zelda's overworld is 16 × 8 screens of 16 × 11 tiles,
  256 × 88 tiles; a Pokémon region is a few hundred tiles across. Menayland at one tile per
  map pixel would be 1,470 × 801 tiles, about 1.2 million, some 90 × 50 Zelda screens:
  workable in chunks, but a designer usually wants a region of the map at a scale of their
  choosing.
- **Rasterising.** Each tile takes its height from the cells around it: interpolated across
  the triangle of cell centres it falls in, so that heights are smooth rather than stepped
  at cell borders. At this scale an Azgaar cell covers many tiles, so detail has to be added.
- **Detail.** Seeded noise at tile scale, its strength following the source (rougher in
  mountains, gentle on plains), keeps the large shapes and adds hills, inlets and rock.
- **Height in top-down 2D** is drawn as levels: ledges and cliffs where one level meets the
  next, as Pokémon and Zelda draw it. So height is quantised into a few levels after detail.
- **Water.** Rivers are paths of cells with a discharge; on tiles they need a width from it,
  carved into the terrain, continuous from source to mouth. Lakes have a surface height.
- **Autotiling.** Choosing the right edge tile where terrains meet. The 47-tile "blob" set
  looks at eight neighbours; the **dual grid**, credited to Oskar Stålberg, draws a second
  grid offset by half a tile, each drawn tile chosen by the four painted cells at its
  corners: 16 tiles per pair of terrains (as few as 6 drawn, if symmetrical), simpler logic
  and rounder corners. TileMapDual brings it to Godot.
- **Foliage** is scattered, not tiled: Poisson-disc sampling (Bridson's algorithm) gives
  even, natural spacing; density and kind follow the biome.
- **Editing over generation.** Repainting a coast, then changing the foliage density, must
  keep the coast. Generators that support this keep generated layers and the designer's
  overrides apart, and compose them.

## Drawing it in Bevy 0.20

- **Bevy has a tilemap of its own:** `TilemapChunk` in `bevy::sprite_render`, a chunk of
  tiles drawn in one draw call from an array texture, with per-tile index, colour,
  visibility and orientation. Its example uses 64 × 64-tile chunks.
- **The map crates aren't there yet:** `bevy_ecs_tilemap` (0.19), `bevy_ecs_tiled` and
  `bevy_ecs_ldtk` have no Bevy 0.20 release, as G0's research found. Bevy's own chunk
  means the builder needn't wait for them, or add a map crate (D-46).
- **3D later** is the same data: the height layer becomes a terrain mesh, foliage becomes
  instanced models. So the map's model should keep enough height to build terrain from.

## Same seed, same map

- Rust's float functions (`sin`, `exp` and the rest) call the platform's maths library, whose
  accuracy varies, so the same seed can give different maps on different machines. A pure
  Rust `libm` is the usual fix (`rand` relies on it for reproducible distributions); Rapier
  needs an opt-in feature and trait-routed trigonometry for cross-platform determinism.
- **Integer noise avoids the question:** value or gradient noise from integer hashes, with
  fixed-point interpolation, is exact everywhere, and matches the engine's `Fixed`.
- A golden-value test (a seed's map hashed and checked in) catches drift on any target.

## The most efficient format

- **Binary serde formats:** `bincode` is unmaintained (RUSTSEC-2025-0141, December 2025:
  development ceased permanently); the advisory suggests `postcard`, `bitcode`, `rkyv` or
  `wincode`. **Postcard** 1.x has a documented, stable wire format with varint integers,
  compact and serde-based, so the engine's existing serde types serialise unchanged.
  `rkyv` is zero-copy, fastest to load, larger and more constraining; `bitcode` is smallest,
  without a stable format promise.
- **Compression:** tile layers are long runs of the same values, so they compress well.
  `lz4_flex` is the fastest pure-Rust option, with a lower ratio; zstd compresses better,
  but its pure-Rust encoders (`ruzstd`, `structured-zstd`) trail the C library, which the
  `zstd` crate binds. Which wins on a real map is for measurement.
- **Readable first:** saves begin with `format` and `version` (P-54); a binary map should
  too, so a wrong file or version is named before anything is decoded. T5 plans a binary
  encoding for saves; one encoding for both is simpler than two.

## Sources

- [Azgaar's Fantasy Map Generator](https://github.com/Azgaar/Fantasy-Map-Generator) and [its licence](https://github.com/Azgaar/Fantasy-Map-Generator/blob/master/LICENSE); [its wiki](https://github.com/Azgaar/Fantasy-Map-Generator/wiki); the JSON export's Full, Minimal and Cells variants as its source shows them ([mirror](https://git.korz.dev/backups/Fantasy-Map-Generator/commit/7b475ee0bd7cf30ced8ac5da3f30495ddb7bab55)); *Menayland*, exported 2026-10-12, read directly
- [Bevy's tilemap chunk example](https://bevy.org/examples/2d-rendering/tilemap-chunk/) and [with orientations](https://bevy.org/examples/2d-rendering/tilemap-chunk-orientation/); [Bevy 0.20](https://bevy.org/news/bevy-0-20/); [bevy_ecs_tilemap](https://docs.rs/bevy_ecs_tilemap/)
- [The dual tilemap autotiling technique](https://excaliburjs.com/blog/Dual%20Tilemap%20Autotiling%20Technique); [TileMapDual](https://github.com/pablogila/TileMapDual)
- [Determinism in Rapier](https://rapier.rs/docs/user_guides/rust/determinism/); [the Rust Rand Book on portability](https://rust-random.github.io/book/portability.html); [libm and determinism, on the Rust forum](https://users.rust-lang.org/t/subtle-floating-point-differences-between-c-library-and-its-rust-re-write/82355?page=2)
- [RUSTSEC-2025-0141, bincode unmaintained](https://rustsec.org/advisories/RUSTSEC-2025-0141); [postcard](https://docs.rs/crate/postcard) and [its wire format](https://postcard.jamesmunns.com/)
- [lz4_flex](https://github.com/pseitz/lz4_flex); [ruzstd](https://docs.rs/ruzstd); [structured-zstd](https://docs.rs/crate/structured-zstd/latest)

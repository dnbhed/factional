//! Rasterising a source (W2b, P-135): a region at a scale made into tiles, each tile's height
//! interpolated across the triangle of cell centres it falls in, in metres by Azgaar's own
//! formula; and the sea, a lake or land under it (W3b's rules, tested in `coast.rs`). Every
//! expected value here is worked by hand from the formulas in DESIGN.md §21.

use factional_core::Fixed;
use factional_worldgen::{
    Biome, Buildings, Coast, Detail, Extent, Feature, Foliage, HEIGHT_EXPONENTS, Levels, Placed,
    Point, RasterError, Recipe, Region, ReligionKind, RouteKind, SettlementKind, Size, Source,
    SourceCell, SourceCulture, SourceMarker, SourceReligion, SourceRoute, SourceSettlement,
    SourceState, SourceVertex, Stance, StateForm, Terrain, TilePos, Water, cell_metres, rasterise,
};
use proptest::prelude::*;

fn hundredths(value: i64) -> Fixed {
    Fixed::from_hundredths(value)
}

fn point(x: i64, y: i64) -> Point {
    Point {
        x: hundredths(x),
        y: hundredths(y),
    }
}

fn cell(centre: Point, height: u8, feature: u32) -> SourceCell {
    SourceCell {
        centre,
        height,
        feature,
        neighbours: Vec::new(),
        corners: Vec::new(),
        flux: 0,
        // Azgaar's land cells have a land biome, its sea and lakes Marine.
        biome: match height {
            20.. => Biome::Grassland,
            _ => Biome::Marine,
        },
        temperature: 0,
        precipitation: 0,
        culture: None,
        religion: None,
        state: None,
    }
}

fn vertex(cells: [Option<u32>; 3]) -> SourceVertex {
    SourceVertex {
        at: point(0, 0),
        cells,
    }
}

/// 40 × 40 pixels: A (10.5, 10.5) at 10 in the ocean, B (30.5, 10.5) at 40, C (10.5, 30.5)
/// at 19 in a lake, D (30.5, 30.5) at 60 and E (20.5, 20.5) at 30, the last three on an
/// island; the triangles ABE, BDE and DCE, and ACE turning the other way; a vertex where A, E
/// and D meet in a line, which makes no triangle; and two vertices at the map's edge.
fn raster(exponent: i64) -> Source {
    Source {
        name: "Raster".to_owned(),
        version: "1.154.0".to_owned(),
        size: Size {
            width: 40,
            height: 40,
        },
        height_exponent: hundredths(exponent),
        cells: vec![
            cell(point(10_50, 10_50), 10, 0),
            cell(point(30_50, 10_50), 40, 2),
            cell(point(10_50, 30_50), 19, 1),
            cell(point(30_50, 30_50), 60, 2),
            cell(point(20_50, 20_50), 30, 2),
        ],
        vertices: vec![
            vertex([Some(0), Some(1), Some(4)]),
            vertex([Some(1), Some(3), Some(4)]),
            vertex([Some(3), Some(2), Some(4)]),
            vertex([Some(0), Some(2), Some(4)]),
            vertex([Some(0), Some(4), Some(3)]),
            vertex([Some(0), None, None]),
            vertex([Some(1), Some(3), None]),
        ],
        features: vec![
            Feature::Ocean,
            Feature::Lake {
                surface: hundredths(19_50),
            },
            Feature::Island,
        ],
        rivers: Vec::new(),
        settlements: Vec::new(),
        routes: Vec::new(),
        markers: Vec::new(),
        cultures: Vec::new(),
        religions: Vec::new(),
        states: Vec::new(),
    }
}

fn recipe(x: u32, y: u32, width: u32, height: u32, scale: u32) -> Recipe {
    Recipe {
        source: "fnv1a64:0000000000000000".to_owned(),
        region: Region {
            x,
            y,
            size: Size { width, height },
        },
        scale,
        seed: 0,
        // No detail, so heights are W2b's.
        detail: Detail {
            strength: Fixed::ZERO,
            ..Detail::default()
        },
        coast: Coast::default(),
        levels: Levels::default(),
        foliage: Foliage::default(),
    }
}

fn whole() -> Recipe {
    recipe(0, 0, 40, 40, 1)
}

fn elevation(source: &Source, recipe: Recipe, x: u32, y: u32) -> Fixed {
    rasterise(source, recipe)
        .expect("the recipe makes a map")
        .tile(TilePos { x, y })
        .expect("on the map")
        .elevation
}

/// `raster` with raster.json's places (W6b): Highmoor, Lakeside and Pinewatch; the High road,
/// an unnamed trail and the Coast road; Ash Peak and Old Keep.
fn with_places() -> Source {
    let settlement = |id, name: &str, at, kind| SourceSettlement {
        id,
        name: name.to_owned(),
        at,
        cell: 4,
        people: u64::from(id) * 1000,
        kind,
        capital: id == 1,
        port: id == 2,
        buildings: Buildings {
            walls: id == 4,
            ..Buildings::default()
        },
    };
    let route = |id, name: Option<&str>, kind, points: Vec<Point>| SourceRoute {
        id,
        name: name.map(str::to_owned),
        kind,
        points,
    };
    let marker = |id, name: &str, at| SourceMarker {
        id,
        kind: "ruins".to_owned(),
        icon: "glyph".to_owned(),
        name: Some(name.to_owned()),
        note: None,
        at,
        cell: 3,
    };
    Source {
        settlements: vec![
            settlement(1, "Highmoor", point(30_25, 9_75), SettlementKind::Generic),
            settlement(2, "Lakeside", point(14_50, 26_75), SettlementKind::Lake),
            settlement(
                4,
                "Pinewatch",
                point(31_50, 31_00),
                SettlementKind::Highland,
            ),
        ],
        routes: vec![
            route(
                0,
                Some("High road"),
                RouteKind::Road,
                vec![
                    point(30_25, 9_75),
                    point(25_50, 15_50),
                    point(20_50, 20_50),
                    point(31_50, 31_00),
                ],
            ),
            route(
                1,
                None,
                RouteKind::Trail,
                vec![point(14_50, 26_75), point(20_50, 20_50)],
            ),
            route(
                2,
                Some("Coast road"),
                RouteKind::Road,
                vec![
                    point(12_00, 12_00),
                    point(35_00, 15_00),
                    point(25_00, 25_00),
                ],
            ),
        ],
        markers: vec![
            marker(0, "Ash Peak", point(33_30, 34_40)),
            marker(1, "Old Keep", point(20_00, 21_00)),
        ],
        ..raster(2_00)
    }
}

/// A place in 256ths each way, as its tile and offset.
fn placed(x: u32, y: u32) -> Placed {
    Placed {
        tile: TilePos {
            x: x / 256,
            y: y / 256,
        },
        offset: [(x % 256) as u8, (y % 256) as u8],
    }
}

/// Each suggestion's id and places, in order: settlements, routes, then points of interest.
fn places(source: &Source, recipe: Recipe) -> Vec<(String, Vec<Placed>)> {
    let map = rasterise(source, recipe).expect("the recipe makes a map");
    let suggestions = map.suggestions();
    suggestions
        .settlements
        .iter()
        .map(|s| (s.id.clone(), vec![s.at]))
        .chain(
            suggestions
                .routes
                .iter()
                .map(|r| (r.id.clone(), r.points.clone())),
        )
        .chain(
            suggestions
                .points
                .iter()
                .map(|p| (p.id.clone(), vec![p.at])),
        )
        .collect()
}

fn at(id: &str, points: &[[u32; 2]]) -> (String, Vec<Placed>) {
    (
        id.to_owned(),
        points.iter().map(|&[x, y]| placed(x, y)).collect(),
    )
}

/// `raster` with raster.json's peoples (W6e): cells 1 and 4 Highfolk's, cell 3 Pinefolk's;
/// Old Ways on cell 1 and Pine Faith on 3 and 4; Highmoor on 1 and 4 and Pinewatch on 3.
/// Seafolk is on no cell.
fn with_peoples() -> Source {
    let mut source = raster(2_00);
    for (cell, ids) in [(1, [1, 1, 1]), (3, [2, 2, 2]), (4, [1, 2, 1])] {
        let [culture, religion, state] = ids.map(Some);
        source.cells[cell] = SourceCell {
            culture,
            religion,
            state,
            ..source.cells[cell].clone()
        };
    }
    let extent = |cells: u32| Extent {
        cells,
        people: u64::from(cells) * 1000,
    };
    let culture = |id, name: &str, kind| SourceCulture {
        id,
        name: name.to_owned(),
        kind,
        extent: extent(id),
    };
    source.cultures = vec![
        culture(1, "Highfolk", SettlementKind::Highland),
        culture(2, "Pinefolk", SettlementKind::Hunting),
        culture(3, "Seafolk", SettlementKind::Naval),
    ];
    source.religions = vec![
        SourceReligion {
            id: 1,
            name: "Old Ways".to_owned(),
            kind: ReligionKind::Folk,
            form: "Shamanism".to_owned(),
            deity: Some("Ash".to_owned()),
            culture: Some(1),
            extent: extent(1),
        },
        SourceReligion {
            id: 2,
            name: "Pine Faith".to_owned(),
            kind: ReligionKind::Organized,
            form: "Monotheism".to_owned(),
            deity: None,
            culture: None,
            extent: extent(2),
        },
    ];
    let state = |id: u32, name: &str, form, toward: (u32, Stance)| SourceState {
        id,
        name: name.to_owned(),
        full_name: format!("The {name}"),
        form,
        culture: Some(id),
        capital: (id == 1).then_some(4),
        stances: vec![toward],
        extent: extent(id),
    };
    source.states = vec![
        state(1, "Highmoor", StateForm::Monarchy, (2, Stance::Rival)),
        state(2, "Pinewatch", StateForm::Republic, (1, Stance::Enemy)),
    ];
    source
}

/// Each people's id and tiles on the map, in order.
fn peoples(source: &Source, recipe: Recipe) -> Vec<(String, u32)> {
    let map = rasterise(source, recipe).expect("the recipe makes a map");
    let suggestions = map.suggestions();
    (suggestions.cultures.iter().map(|c| (c.id.clone(), c.tiles)))
        .chain(
            suggestions
                .religions
                .iter()
                .map(|r| (r.id.clone(), r.tiles)),
        )
        .chain(suggestions.states.iter().map(|s| (s.id.clone(), s.tiles)))
        .collect()
}

#[test]
fn rasterising_suggests_each_people_a_tile_lies_in_counting_its_tiles() {
    // Worked by peoples_map_ref.py, apart from the code, as in factional-content's tests: each
    // tile lies in the cell nearest its centre, the first if several are as near.
    let named = |tiles: [u32; 6]| -> Vec<(String, u32)> {
        [
            "culture_1",
            "culture_2",
            "religion_1",
            "religion_2",
            "state_1",
            "state_2",
        ]
        .into_iter()
        .map(str::to_owned)
        .zip(tiles)
        .collect()
    };
    assert_eq!(
        peoples(&with_peoples(), whole()),
        named([535, 325, 354, 506, 535, 325])
    );
    assert_eq!(
        peoples(&with_peoples(), recipe(10, 10, 20, 20, 2)),
        named([969, 171, 209, 931, 969, 171])
    );
    assert_eq!(peoples(&with_peoples(), recipe(0, 0, 10, 10, 1)), []);
    // A region of cell 3's alone: only Pinefolk, Pine Faith and Pinewatch.
    assert_eq!(
        peoples(&with_peoples(), recipe(30, 30, 10, 10, 1)),
        [
            ("culture_2".to_owned(), 100),
            ("religion_2".to_owned(), 100),
            ("state_2".to_owned(), 100)
        ]
    );
    // What the source says of each, its links as suggestion ids.
    let map = rasterise(&with_peoples(), whole()).expect("a map");
    let suggestions = map.suggestions();
    let highfolk = &suggestions.cultures[0];
    assert_eq!(
        (highfolk.name.as_str(), highfolk.kind, highfolk.extent),
        (
            "Highfolk",
            SettlementKind::Highland,
            Extent {
                cells: 1,
                people: 1000
            }
        )
    );
    let old_ways = &suggestions.religions[0];
    assert_eq!(
        (
            old_ways.name.as_str(),
            old_ways.kind,
            old_ways.form.as_str(),
            old_ways.deity.as_deref(),
            old_ways.culture.as_deref(),
            old_ways.extent.cells
        ),
        (
            "Old Ways",
            ReligionKind::Folk,
            "Shamanism",
            Some("Ash"),
            Some("culture_1"),
            1
        )
    );
    assert_eq!(suggestions.religions[1].culture, None);
    let highmoor = &suggestions.states[0];
    assert_eq!(
        (
            highmoor.name.as_str(),
            highmoor.full_name.as_str(),
            highmoor.form,
            highmoor.culture.as_deref(),
            highmoor.capital.as_deref(),
            highmoor.extent.people
        ),
        (
            "Highmoor",
            "The Highmoor",
            StateForm::Monarchy,
            Some("culture_1"),
            Some("settlement_4"),
            1000
        )
    );
    assert_eq!(highmoor.stances, [("state_2".to_owned(), Stance::Rival)]);
    let pinewatch = &suggestions.states[1];
    assert_eq!(
        (pinewatch.capital.as_deref(), pinewatch.culture.as_deref()),
        (None, Some("culture_2"))
    );
    assert_eq!(pinewatch.stances, [("state_1".to_owned(), Stance::Enemy)]);
}

#[test]
fn rasterising_suggests_the_sources_places_where_they_lie() {
    // Worked by suggest_ref.py, apart from the code, as in factional-content's tests.
    assert_eq!(
        places(&with_places(), whole()),
        vec![
            at("settlement_1", &[[7744, 2496]]),
            at("settlement_2", &[[3712, 6848]]),
            at("settlement_4", &[[8064, 7936]]),
            at(
                "route_0",
                &[[7744, 2496], [6528, 3968], [5248, 5248], [8064, 7936]]
            ),
            at("route_1", &[[3712, 6848], [5248, 5248]]),
            at("route_2", &[[3072, 3072], [8960, 3840], [6400, 6400]]),
            at("poi_0", &[[8525, 8806]]),
            at("poi_1", &[[5120, 5376]]),
        ]
    );
    let map = rasterise(&with_places(), whole()).expect("a map");
    let highmoor = &map.suggestions().settlements[0];
    assert_eq!(
        (
            highmoor.name.as_str(),
            highmoor.people,
            highmoor.kind,
            highmoor.capital,
            highmoor.port
        ),
        ("Highmoor", 1000, SettlementKind::Generic, true, false)
    );
    assert!(map.suggestions().settlements[2].buildings.walls);
    let trail = &map.suggestions().routes[1];
    assert_eq!(
        (trail.name.as_deref(), trail.kind),
        (None, RouteKind::Trail)
    );
    let keep = &map.suggestions().points[1];
    assert_eq!(
        (
            keep.kind.as_str(),
            keep.icon.as_str(),
            keep.name.as_deref(),
            keep.note.as_deref()
        ),
        ("ruins", "glyph", Some("Old Keep"), None)
    );
}

#[test]
fn a_regions_suggestions_drop_whats_outside_and_cut_routes_at_its_edge() {
    assert_eq!(
        places(&with_places(), recipe(10, 10, 20, 20, 2)),
        vec![
            at("settlement_2", &[[2304, 8576]]),
            at(
                "route_0",
                &[[10_239, 28], [7936, 2816], [5376, 5376], [10_239, 10_018]]
            ),
            at("route_1", &[[2304, 8576], [5376, 5376]]),
            at("route_2_1", &[[1024, 1024], [10_239, 2226]]),
            at("route_2_2", &[[10_239, 5121], [7680, 7680]]),
            at("poi_1", &[[5120, 5632]]),
        ]
    );
    // Not square: 20 × 10 pixels from (10, 20) at 1 tile a pixel, so 5120 × 2560 256ths.
    // Lakeside (4.5, 6.75) and Old Keep (10, 1) lie inside; Pinewatch, at x 21.5, doesn't.
    let part = places(&with_places(), recipe(10, 20, 20, 10, 1));
    let ids: Vec<&str> = part.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["settlement_2", "route_0", "route_1", "route_2", "poi_1"]
    );
    assert_eq!(part[0], at("settlement_2", &[[1152, 1728]]));
    assert_eq!(
        part[1],
        at("route_0", &[[2816, 0], [2688, 128], [5119, 2449]])
    );
    assert_eq!(part[2], at("route_1", &[[1152, 1728], [2688, 128]]));
    assert_eq!(part[3], at("route_2", &[[5119, 1], [3840, 1280]]));
    assert_eq!(part[4], at("poi_1", &[[2560, 256]]));
    // Tall: 20 × 40 pixels from (10, 0). Highmoor, Pinewatch and Ash Peak lie within it top to
    // bottom but not left to right, so none is suggested.
    let tall = places(&with_places(), recipe(10, 0, 20, 40, 1));
    let ids: Vec<&str> = tall.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "settlement_2",
            "route_0",
            "route_1",
            "route_2_1",
            "route_2_2",
            "poi_1"
        ]
    );
    assert_eq!(tall[0], at("settlement_2", &[[1152, 6848]]));
    assert_eq!(
        tall[1],
        at(
            "route_0",
            &[[5119, 2575], [3968, 3968], [2688, 5248], [5119, 7569]]
        )
    );
    assert_eq!(tall[5], at("poi_1", &[[2560, 5376]]));
}

#[test]
fn metres_follow_azgaars_formula_with_a_whole_exponent() {
    let two = hundredths(2_00);
    assert_eq!(cell_metres(10, two), hundredths(-50_00));
    assert_eq!(cell_metres(40, two), hundredths(484_00));
    assert_eq!(cell_metres(19, two), hundredths(-2_63));
    assert_eq!(cell_metres(60, two), hundredths(1764_00));
    assert_eq!(cell_metres(30, two), hundredths(144_00));
    assert_eq!(cell_metres(20, two), hundredths(4_00));
    assert_eq!(cell_metres(1, two), hundredths(-950_00));
    assert_eq!(cell_metres(0, two), hundredths(-990_00));
}

#[test]
fn a_fractional_exponent_is_computed_within_a_centimetre() {
    let one_and_a_half = hundredths(1_50);
    assert_eq!(cell_metres(40, one_and_a_half), hundredths(103_19));
    assert_eq!(cell_metres(60, one_and_a_half), hundredths(272_19));
    assert_eq!(cell_metres(30, one_and_a_half), hundredths(41_57));
    assert_eq!(cell_metres(20, one_and_a_half), hundredths(2_83));
    assert_eq!(cell_metres(40, hundredths(2_20)), hundredths(898_11));
    assert_eq!(cell_metres(100, hundredths(2_20)), hundredths(16232_69));
    // Under water, the exponent plays no part.
    assert_eq!(cell_metres(10, hundredths(2_20)), hundredths(-50_00));
}

#[test]
fn an_exponent_outside_azgaars_range_is_held_to_it() {
    assert_eq!(cell_metres(40, hundredths(3_00)), hundredths(898_11));
    assert_eq!(cell_metres(40, hundredths(1_00)), hundredths(103_19));
    assert_eq!(HEIGHT_EXPONENTS, [hundredths(1_50), hundredths(2_20)]);
}

#[test]
fn a_tile_at_a_cells_centre_has_its_height() {
    let source = raster(2_00);
    assert_eq!(elevation(&source, whole(), 20, 20), hundredths(144_00));
    assert_eq!(elevation(&source, whole(), 10, 30), hundredths(-2_63));
    assert_eq!(elevation(&source, whole(), 30, 30), hundredths(1764_00));
}

#[test]
fn a_tile_inside_a_triangle_is_weighted_by_its_barycentric_coordinates() {
    // (20.5, 15.5) in ABE: a quarter of A and of B, half of E.
    assert_eq!(
        elevation(&raster(2_00), whole(), 20, 15),
        hundredths(180_50)
    );
}

#[test]
fn a_tile_on_an_edge_is_weighted_along_it() {
    // (15.5, 15.5), halfway along AE, which ABE and ACE share: half of A and half of E,
    // where the nearest cell, A, would give -50.00.
    assert_eq!(elevation(&raster(2_00), whole(), 15, 15), hundredths(47_00));
}

#[test]
fn a_triangle_turning_either_way_is_weighted_alike() {
    // (15.5, 20.5) in ACE: a quarter of A and of C, half of E.
    assert_eq!(elevation(&raster(2_00), whole(), 15, 20), hundredths(58_84));
}

#[test]
fn a_tile_outside_every_triangle_takes_the_nearest_cells_height() {
    let source = raster(2_00);
    assert_eq!(elevation(&source, whole(), 0, 0), hundredths(-50_00));
    assert_eq!(elevation(&source, whole(), 39, 39), hundredths(1764_00));
}

#[test]
fn a_region_at_a_scale_is_rasterised_at_its_tiles_centres() {
    let map = rasterise(&raster(2_00), recipe(10, 10, 20, 20, 2)).expect("a map");
    assert_eq!(
        map.size(),
        Size {
            width: 40,
            height: 40
        }
    );
    // (20.25, 19.25) in ABE: 0.075 of A, 0.05 of B and 0.875 of E.
    let tile = map.tile(TilePos { x: 20, y: 18 }).expect("on the map");
    assert_eq!(tile.elevation, hundredths(146_45));
}

#[test]
fn sea_lake_and_land_have_their_layers() {
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    let at = |x, y| map.tile(TilePos { x, y }).expect("on the map");
    let sea = at(0, 0);
    assert_eq!(
        (sea.terrain, sea.water, sea.biome, sea.plant),
        (Terrain::DeepWater, None, Biome::Marine, None)
    );
    let lake = at(10, 30);
    assert_eq!(
        (lake.terrain, lake.water, lake.biome, lake.plant),
        (
            Terrain::ShallowWater,
            Some(Water::Lake),
            Biome::Marine,
            None
        )
    );
    // Land's plant is the foliage's, tested in climate.rs (W5).
    let land = at(20, 15);
    assert_eq!(
        (land.terrain, land.water, land.biome),
        (Terrain::Grass, None, Biome::Grassland)
    );
}

#[test]
fn a_tie_for_nearest_goes_to_the_first_cell() {
    // (5.5, 20.5) is outside every triangle, and as far from A as from C: A's height and its
    // ocean win, deep sea at −50.00 m, where C's would be lake at −2.63.
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    let tile = map.tile(TilePos { x: 5, y: 20 }).expect("on the map");
    assert_eq!(
        (tile.terrain, tile.water, tile.elevation),
        (Terrain::DeepWater, None, hundredths(-50_00))
    );
}

#[test]
fn a_region_off_the_source_is_refused() {
    let refused = rasterise(&raster(2_00), recipe(30, 30, 20, 20, 1)).expect_err("refused");
    assert_eq!(
        refused.to_string(),
        "the region runs off the source map: pixels (30, 30) to (49, 49), and the map is 40 × 40 pixels"
    );
    for (x, y, width, height) in [(1, 0, 40, 40), (0, 1, 40, 40)] {
        assert!(matches!(
            rasterise(&raster(2_00), recipe(x, y, width, height, 1)),
            Err(RasterError::OffSource { .. })
        ));
    }
    assert!(matches!(
        rasterise(&raster(2_00), recipe(0, 0, 40, 40, 0)),
        Err(RasterError::Map(_))
    ));
}

#[test]
fn a_source_without_cells_is_refused() {
    let mut source = raster(2_00);
    source.cells.clear();
    source.vertices.clear();
    let refused = rasterise(&source, whole()).expect_err("refused");
    assert_eq!(refused, RasterError::NoCells);
    assert_eq!(refused.to_string(), "the source map has no cells");
}

#[test]
fn the_recipe_is_kept() {
    let map = rasterise(&raster(2_00), whole()).expect("a map");
    assert_eq!(*map.recipe(), whole());
}

proptest! {
    #[test]
    fn every_tile_lies_between_the_lowest_and_highest_cells(
        x in 0u32..39,
        y in 0u32..39,
        width in 1u32..20,
        height in 1u32..20,
        scale in 1u32..4,
        exponent in 1_50i64..=2_20,
    ) {
        let source = raster(exponent);
        let (width, height) = (width.min(40 - x), height.min(40 - y));
        let metres: Vec<Fixed> = source
            .cells
            .iter()
            .map(|cell| cell_metres(cell.height, source.height_exponent))
            .collect();
        let (lowest, highest) = (
            *metres.iter().min().expect("cells"),
            *metres.iter().max().expect("cells"),
        );
        let map = rasterise(&source, recipe(x, y, width, height, scale)).expect("a map");
        for (_, chunk) in map.chunks() {
            let size = chunk.size();
            for (tx, ty) in (0..size.height).flat_map(|ty| (0..size.width).map(move |tx| (tx, ty))) {
                let tile = chunk.tile(tx, ty).expect("in the chunk");
                prop_assert!(lowest <= tile.elevation && tile.elevation <= highest);
            }
        }
        prop_assert_eq!(rasterise(&source, recipe(x, y, width, height, scale)), Ok(map));
    }
}

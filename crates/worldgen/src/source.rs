//! A source map (W2a, P-119): what Factional keeps of an Azgaar export, read and checked by
//! `factional-content`, so every reference here resolves.

use factional_core::Fixed;

use crate::{Biome, Size};

/// A point on the source map, in its pixels, to the hundredth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: Fixed,
    pub y: Fixed,
}

/// One of the source's cells: its centre, its height (0 to 100, sea level at 20, as Azgaar
/// has it), the feature it's in, its neighbouring cells and the vertices at its corners,
/// each by its place in the source's lists, and its flux, the water flowing through it in
/// Azgaar's cubic metres a second; its biome, and the climate of its grid cell, its yearly
/// temperature in whole °C and precipitation in mm; and the culture, religion and state it
/// belongs to, by id, if any (W6d).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCell {
    pub centre: Point,
    pub height: u8,
    pub feature: u32,
    pub neighbours: Vec<u32>,
    pub corners: Vec<u32>,
    pub flux: u32,
    pub biome: Biome,
    pub temperature: i8,
    pub precipitation: u16,
    pub culture: Option<u32>,
    pub religion: Option<u32>,
    pub state: Option<u32>,
}

/// One of the source's vertices: where it is, and the three cells that meet there, `None`
/// where one is past the map's edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVertex {
    pub at: Point,
    pub cells: [Option<u32>; 3],
}

/// A feature of the source: an ocean, a lake with its surface height on Azgaar's scale, or
/// an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Ocean,
    Lake { surface: Fixed },
    Island,
}

/// One of the source's rivers (W3d, P-139): its cells from its source to where it ends, the
/// water it flows into or the river it joins, or past the map's edge if `off_map`; and the
/// width Azgaar draws it at, its factor and its half-width at the source in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRiver {
    pub cells: Vec<u32>,
    pub off_map: bool,
    pub width_factor: Fixed,
    pub source_width: Fixed,
}

/// One of the source's settlements, Azgaar's burgs (W6a, P-144): its id, name, point and cell;
/// its people, as Azgaar shows them; its type; whether it's a capital and a port; and its
/// buildings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSettlement {
    pub id: u32,
    pub name: String,
    pub at: Point,
    pub cell: u32,
    pub people: u64,
    pub kind: SettlementKind,
    pub capital: bool,
    pub port: bool,
    pub buildings: Buildings,
}

/// A settlement's type, as Azgaar has it; a culture's too, the same seven (W6d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettlementKind {
    Generic,
    Naval,
    Lake,
    Highland,
    River,
    Nomadic,
    Hunting,
}

/// The buildings Azgaar gives a settlement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buildings {
    pub citadel: bool,
    pub walls: bool,
    pub plaza: bool,
    pub temple: bool,
    pub shanty: bool,
}

/// One of the source's routes (W6a, P-144): its id, its name if it has one, its kind, and its
/// points, at least two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRoute {
    pub id: u32,
    pub name: Option<String>,
    pub kind: RouteKind,
    pub points: Vec<Point>,
}

/// A route's kind: Azgaar's roads, trails and sea routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteKind {
    Road,
    Trail,
    SeaRoute,
}

/// One of the source's points of interest, Azgaar's markers (W6a, P-144): its id, its type and
/// icon as Azgaar names them, its name and note if it has them, and its point and cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMarker {
    pub id: u32,
    pub kind: String,
    pub icon: String,
    pub name: Option<String>,
    pub note: Option<String>,
    pub at: Point,
    pub cell: u32,
}

/// How far a culture, religion or state reaches (W6d, P-147): the cells naming it, and its
/// people, its cells' and its settlements' as Azgaar counts them, rounded once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Extent {
    pub cells: u32,
    pub people: u64,
}

/// One of the source's cultures (W6d, P-147): its id, name and type, and its extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCulture {
    pub id: u32,
    pub name: String,
    pub kind: SettlementKind,
    pub extent: Extent,
}

/// One of the source's religions (W6d, P-147): its id, name and type, its form as Azgaar words
/// it, its deity if it has one, the culture it belongs to if any, and its extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReligion {
    pub id: u32,
    pub name: String,
    pub kind: ReligionKind,
    pub form: String,
    pub deity: Option<String>,
    pub culture: Option<u32>,
    pub extent: Extent,
}

/// A religion's type, as Azgaar has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReligionKind {
    Folk,
    Organized,
    Cult,
    Heresy,
}

/// One of the source's states (W6d, P-147): its id, name and full name, its form, its culture
/// and capital settlement if any, its stance toward each other state by that state's id, in
/// order, and its extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceState {
    pub id: u32,
    pub name: String,
    pub full_name: String,
    pub form: StateForm,
    pub culture: Option<u32>,
    pub capital: Option<u32>,
    pub stances: Vec<(u32, Stance)>,
    pub extent: Extent,
}

/// A state's form of government, as Azgaar has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StateForm {
    Monarchy,
    Republic,
    Union,
    Theocracy,
    Anarchy,
}

/// A state's stance toward another, Azgaar's diplomacy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stance {
    Ally,
    Friendly,
    Neutral,
    Suspicion,
    Enemy,
    Unknown,
    Rival,
    Vassal,
    Suzerain,
}

/// A source map: its name, the generator's version that made it, its size in pixels, the
/// exponent its heights are raised to in metres, and its cells, vertices, features and
/// rivers; its settlements, routes and points of interest; and its cultures, religions and
/// states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub version: String,
    pub size: Size,
    pub height_exponent: Fixed,
    pub cells: Vec<SourceCell>,
    pub vertices: Vec<SourceVertex>,
    pub features: Vec<Feature>,
    pub rivers: Vec<SourceRiver>,
    pub settlements: Vec<SourceSettlement>,
    pub routes: Vec<SourceRoute>,
    pub markers: Vec<SourceMarker>,
    pub cultures: Vec<SourceCulture>,
    pub religions: Vec<SourceReligion>,
    pub states: Vec<SourceState>,
}

//! Arranging the tokens (G9, P-131): a mode of the builder where tokens are dragged where the
//! designer wants them, each drag undoable, and the layout saved to `builder.toml`.

use std::collections::BTreeMap;

use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, Query, Res, ResMut};
use bevy::math::Vec2;
use bevy::transform::components::Transform;
use bevy_egui::egui;
use factional_reputation::CharacterId;
use toml_edit::{DocumentMut, InlineTable, Item, Table, value};

use crate::{BuilderFile, ContentDir, Token};

/// How far from a token's middle a press picks it, in points.
pub const TOKEN_RADIUS: f32 = 14.0;

/// Where the pointer is on the plane, and whether it's pressed; filled by the app from the
/// cursor and the camera, and `None` while egui has the pointer.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub at: Option<Vec2>,
    pub down: bool,
}

/// A token held by the pointer: whose, where it was gripped from its middle, and where the
/// drag began.
#[derive(Debug, Clone, PartialEq)]
struct Held {
    character: CharacterId,
    grip: Vec2,
    from: Vec2,
}

/// The arranging mode: whether it's on, the token held, each drag to undo, and where each
/// token was placed or last saved.
#[derive(Resource, Debug, Default)]
pub struct Arranging {
    pub on: bool,
    held: Option<Held>,
    undo: Vec<(CharacterId, Vec2)>,
    saved: BTreeMap<CharacterId, Vec2>,
    unsaved: bool,
    undo_asked: bool,
    save_asked: bool,
    note: Option<String>,
}

impl Arranging {
    /// The token held, if any.
    pub fn held(&self) -> Option<&CharacterId> {
        self.held.as_ref().map(|held| &held.character)
    }

    /// Whether there's a drag to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether any token stands somewhere other than where it was placed or last saved.
    pub fn unsaved(&self) -> bool {
        self.unsaved
    }

    /// What became of the last save, if it failed.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Undoes the last drag, next frame.
    pub fn ask_undo(&mut self) {
        self.undo_asked = true;
    }

    /// Saves the layout, next frame.
    pub fn ask_save(&mut self) {
        self.save_asked = true;
    }
}

/// The token a press at `at` picks, of `tokens`: the nearest within [`TOKEN_RADIUS`], the
/// first by id if as near.
pub fn pick(at: Vec2, tokens: &[(CharacterId, Vec2)]) -> Option<CharacterId> {
    tokens
        .iter()
        .map(|(character, there)| (at.distance(*there), character))
        .filter(|(away, _)| *away <= TOKEN_RADIUS)
        .min_by(|(one, here), (other, there)| one.total_cmp(other).then_with(|| here.cmp(there)))
        .map(|(_, character)| character.clone())
}

/// `builder.toml`'s text, `file` if there is one, with `[positions]` set to `positions`,
/// rounded to whole points: those there changed in place, the rest added in order, and
/// everything else kept. A file that can't be read is refused.
pub fn write_positions(
    file: Option<&str>,
    positions: &[(CharacterId, Vec2)],
) -> Result<String, String> {
    let refused = |why: &str| {
        let why = why.trim().trim_end_matches('.');
        format!("builder.toml can't be read, so the layout isn't saved: {why}")
    };
    let text = file.unwrap_or_default();
    toml::from_str::<BuilderFile>(text).map_err(|error| refused(error.message()))?;
    let mut document: DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| refused(error.message()))?;
    if !document.contains_key("positions") {
        let mut table = Table::new();
        if !document.is_empty() {
            table.decor_mut().set_prefix("\n");
        }
        document.insert("positions", Item::Table(table));
    }
    let table = document["positions"]
        .as_table_like_mut()
        .ok_or_else(|| refused("positions isn't a table"))?;
    for (character, at) in positions {
        let mut point = InlineTable::new();
        point.insert("x", whole(at.x).into());
        point.insert("y", whole(at.y).into());
        match table.get_mut(character.as_str()) {
            Some(there) => *there = value(point),
            None => {
                table.insert(character.as_str(), value(point));
            }
        }
    }
    Ok(document.to_string())
}

/// `along` to the nearest whole point, half away from zero, never `-0.0`.
fn whole(along: f32) -> f64 {
    f64::from(along.round()) + 0.0
}

/// "Arrange", and while arranging, Undo and Save layout, and why the last save failed.
pub fn arrange_ui(ui: &mut egui::Ui, arranging: &mut Arranging) {
    ui.horizontal(|ui| {
        ui.toggle_value(&mut arranging.on, "Arrange");
        if arranging.on {
            let undo = egui::Button::new("Undo");
            if ui.add_enabled(arranging.can_undo(), undo).clicked() {
                arranging.ask_undo();
            }
            let save = egui::Button::new("Save layout");
            if ui.add_enabled(arranging.unsaved(), save).clicked() {
                arranging.ask_save();
            }
        }
    });
    if let Some(note) = arranging.note() {
        ui.label(note);
    }
}

/// While arranging, a press on a token holds it, the pointer drags it from where it was
/// gripped, and letting go drops it where it's let go, a step to undo if it moved; undo and save, as asked; and
/// whether anyone stands somewhere other than where they were placed or last saved.
pub(crate) fn arrange(
    pointer: Res<Pointer>,
    mut was_down: Local<bool>,
    mut arranging: ResMut<Arranging>,
    dir: Res<ContentDir>,
    mut tokens: Query<(&Token, &mut Transform)>,
) {
    let mut placed: Vec<(CharacterId, Vec2)> = tokens
        .iter()
        .map(|(token, at)| (token.character.clone(), at.translation.truncate()))
        .collect();
    placed.sort_by(|a, b| a.0.cmp(&b.0));
    if arranging.saved.is_empty() {
        arranging.saved = placed.iter().cloned().collect();
    }
    let mut moves: Vec<(CharacterId, Vec2)> = Vec::new();
    if std::mem::take(&mut arranging.undo_asked) {
        arranging.held = None;
        moves.extend(arranging.undo.pop());
    }
    let pressed = pointer.down && !*was_down;
    *was_down = pointer.down;
    match (arranging.on, pointer.at) {
        (false, _) => arranging.held = None,
        (true, Some(at)) if pressed => {
            arranging.held = pick(at, &placed).map(|character| {
                let from = placed
                    .iter()
                    .find(|(one, _)| *one == character)
                    .map_or(at, |(_, there)| *there);
                Held {
                    character,
                    grip: from - at,
                    from,
                }
            });
        }
        (true, Some(at)) if pointer.down => {
            if let Some(held) = &arranging.held {
                moves.push((held.character.clone(), at + held.grip));
            }
        }
        (true, at) if !pointer.down => {
            if let Some(held) = arranging.held.take() {
                // Dropped where it's let go.
                let now = at.map(|at| at + held.grip).or_else(|| {
                    let found = placed.iter().find(|(one, _)| *one == held.character);
                    found.map(|(_, there)| *there)
                });
                if let Some(now) = now.filter(|now| *now != held.from) {
                    moves.push((held.character.clone(), now));
                    arranging.undo.push((held.character, held.from));
                }
            }
        }
        _ => {}
    }
    for (character, to) in &moves {
        if let Some(entry) = placed.iter_mut().find(|(one, _)| one == character) {
            entry.1 = *to;
        }
    }
    if std::mem::take(&mut arranging.save_asked) {
        let file = dir.0.join("builder.toml");
        let text = std::fs::read_to_string(&file).ok();
        let saved = write_positions(text.as_deref(), &placed).and_then(|written| {
            std::fs::write(&file, written)
                .map_err(|error| format!("couldn't save builder.toml: {error}"))
        });
        match saved {
            Ok(()) => {
                for entry in &mut placed {
                    entry.1 = Vec2::new(whole(entry.1.x) as f32, whole(entry.1.y) as f32);
                    moves.push(entry.clone());
                }
                arranging.saved = placed.iter().cloned().collect();
                arranging.note = None;
            }
            Err(said) => arranging.note = Some(said),
        }
    }
    for (token, mut at) in &mut tokens {
        if let Some((_, to)) = moves.iter().rev().find(|(one, _)| *one == token.character) {
            at.translation = to.extend(at.translation.z);
        }
    }
    let saved = &arranging.saved;
    let unsaved = placed
        .iter()
        .any(|(character, there)| saved.get(character) != Some(there));
    arranging.unsaved = unsaved;
}

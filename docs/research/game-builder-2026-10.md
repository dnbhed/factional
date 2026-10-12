# A game builder on Bevy: research (2026-10-12)

What a game builder on Bevy could be, now that the plugin exists (E1), and what others built
on Bevy, or for faction and reputation games, do. This is the research behind G0's design
pass; what's decided will be in DECISIONS.md and DESIGN.md, and where they differ from this
note, they win.

The user's answers so far: a separate application from the egui content editor, each able to
open the other on the same content directory; its design pass before the rest of the editor's
plan. What it is at its core is for this note to inform.

## Where things stand

- **Factional** is content (TOML, checked complete at load, D-20), an engine of commands,
  events and queries (`factional-reputation`, `factional-quests`), a session of every
  command (`factional-session`), an egui content editor (`factional-editor`, egui 0.36), and
  now a Bevy plugin (`factional-bevy`, Bevy 0.20.0): the world and quests as a resource,
  commands in and events out as messages, time at a rate the game sets.
- **Nothing in the module knows where anyone is.** Witnesses are named in each command
  (`Witnesses::Everyone`, a list, or none); contacts carry news between characters (K2). A
  game decides who sees what. A builder is where that decision would first be made
  concretely.

## What exists on Bevy

- **No official editor yet.** Bevy 0.20 (8 October 2026) builds toward one: BSN, its scene
  format, gained `@` references and a `Ready` event when a scene has spawned; Feathers, its
  editor-centric widget set on `bevy_ui`, gained colour, list, dropdown and menu widgets.
  The official prototypes repository was archived in April 2026, with experiments moved to
  **jackdaw**, a community 3D level editor (brushes, CSG, gizmos, undo, its own `.jsn`
  scenes, BSN on a branch), run standalone or as a plugin in a game; 0.19 is in release
  candidates.
- **Games as their own editors.** Yoleck makes a game its own level editor: plain structs,
  saved as JSON, populate systems to spawn entities and egui systems to edit them, the same
  executable in game or editor mode. Blenvy uses Blender as the level editor, components set
  on glTF objects.
- **Inspectors.** `bevy-inspector-egui` (0.37, for Bevy 0.19) shows a running app's
  entities, resources and assets, editable; `aalo` ports it to `bevy_ui`. The Bevy Remote
  Protocol lets outside tools query and change a running app; `bevy_brp_mcp` lets coding
  assistants drive one.
- **Maps.** `bevy_ecs_tilemap` (0.19) draws tile maps; `bevy_ecs_tiled` (0.13, for Bevy
  0.19) loads Tiled maps, `bevy_ecs_ldtk` (0.15) LDtk's. None is published for Bevy 0.20 yet.
- **UI.** `bevy_egui` 0.43.1 (10 October 2026) supports Bevy 0.20 with egui 0.36, the
  content editor's egui: its widgets and layout code could be drawn inside a Bevy app as
  they are. Feathers is Bevy's own direction, and doesn't share code with the editor.

## What faction and reputation games give their makers

- **RimWorld's development mode** is both the developers' debugger and a cheat menu: debug
  actions such as "Set Faction Relations", forcing raids and visits, killing a faction's
  leader, all from inside the running game.
- **Crusader Kings III's debug console and character menu** change opinions and relations
  between characters (`add_opinion`, `add_relation`) and show why someone thinks what they
  think, on hover, inside the game.
- **Inky, Yarn Spinner and Arcweave** (from the editor's research) keep a play pane beside
  the writing; Arcweave's debugger shows variables changing as you play.
- **RPG Maker** playtests from the editor in one click, its events placed on a map; what an
  NPC does is tied to where it stands and what the player touches.

The pattern: the maker plays the game with the system laid bare, and changes it from there.
A reputation system is hard to judge from numbers alone; who saw what, and how far news
travelled, is spatial.

## What the builder could be

Three shapes, not exclusive:

1. **A runtime inspector, as a plugin** (`factional-bevy-inspector`): panels any game using
   the plugin can turn on, as RimWorld's development mode is inside RimWorld: the world's
   characters, factions, standings and relations, the event log, quests, and a console of
   commands. It ships in games, so it's useful beyond the builder. With `bevy_egui`, its
   panels could reuse the editor's drawing code and Play's (U15b, U15c).
2. **A playable sandbox, as an app** (`factional-builder`): a small 2D world where the
   content's characters stand, the player moves among them and acts (steal, help, murder),
   witnesses chosen by who's near, news spreading along contacts (K2) drawn as it goes, each
   character's disposition toward the player shown on them; time paused, stepped or run;
   the inspector beside it. It's where content is played as a game would play it, and the
   reference for how a game wires the plugin.
3. **A world editor, as a mode of the sandbox:** placing the characters, the places they
   gather and their contacts on the map, saved beside the content. Jackdaw, Yoleck and
   Blenvy show that this grows into a level editor, which the module doesn't need to own.

**What stays as it is:** the content and its checks. Where characters stand is a game's
business, not the module's (nothing a game could reach is left undefined, D-20, but
positions aren't the module's to check); a sandbox's layout would be its own file, read only
by the builder.

**Launching each other:** the editor's "Play in the builder" opens the builder on the same
directory, with the texts as saved; the builder's "Edit in the editor" opens the editor at
the entry chosen. Two processes, the directory between them; unsaved edits don't cross.

## Questions for the design pass

- **O-9 · What the builder is first:** the inspector, the sandbox, or both, the inspector
  first as the sandbox's panels.
- **O-10 · Its UI:** `bevy_egui`, reusing the editor's egui code, or Bevy's own `bevy_ui`
  with Feathers, the engine's direction but nothing shared.
- **O-11 · Its world:** 2D with placeholder shapes, 2D tile maps (waiting for the map
  crates' Bevy 0.20 releases), or 3D.
- **O-12 · Where its layout lives:** a builder file beside the content (who stands where,
  places), read only by the builder; or nothing saved, characters placed afresh each run.

## Sources

- [Bevy 0.20](https://bevy.org/news/bevy-0-20/) and [its migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/); [Bevy 0.20 at GameFromScratch](https://gamefromscratch.com/bevy-0-20-rust-powered-game-engine-released/); [byteiota on Bevy 0.20](https://byteiota.com/bevy-0-20-solari-macos-wesl-shaders/)
- [bevy_editor_prototypes (archived)](https://github.com/bevyengine/bevy_editor_prototypes); [jackdaw](https://docs.rs/crate/jackdaw/latest); [Bevy's fifth birthday](https://bevy.org/news/bevys-fifth-birthday/)
- [Yoleck](https://docs.rs/bevy-yoleck); [Blenvy](https://github.com/kaosat-dev/Blenvy)
- [bevy-inspector-egui](https://lib.rs/crates/bevy-inspector-egui-derive); [aalo](https://lib.rs/crates/aalo); [bevy_brp_mcp](https://docs.rs/crate/bevy_brp_mcp/0.22.3)
- [bevy_ecs_tiled](https://github.com/adrien-bon/bevy_ecs_tiled); [bevy_ecs_ldtk](https://docs.rs/bevy_ecs_ldtk); bevy_egui 0.43.1, bevy_ecs_tilemap 0.19.0, bevy_ecs_ldtk 0.15.0 and bevy_ecs_tiled 0.13.4 as crates.io lists them on 2026-10-12
- [RimWorld development mode](https://rimworldwiki.com/wiki/Development_mode); [Crusader Kings III console commands](https://pcgamer.com/crusader-kings-3-ck3-console-commands-cheats)
- [RPG Maker and Godot compared](https://gamedesignskills.com/game-development/rpg-maker-vs-godot/); the editor's research, [usability-2026-10.md](usability-2026-10.md), for Inky, Yarn Spinner and Arcweave

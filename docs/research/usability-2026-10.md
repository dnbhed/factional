# Usability pass: research (2026-10-10, updated after U8 on 2026-10-11)

A look at how comparable tools handle the four things raised after U6g, and what else they do that the editor could learn from. Brought up to date with U8 (#62), which routed the quest graphs' lines around their boxes and kept the graphs read-only (D-36). This is the research behind U9's design pass. What was decided is in DECISIONS.md (D-37 to D-40, P-91 to P-97) and DESIGN.md §18; where those differ from this note, they win.

The comparable tools: articy:draft (narrative and quest flow), Bethesda's Creation Kit (factions), Pixel Crushers' Dialogue System and Quest Machine (Unity quests), Yarn Spinner and Inky (dialogue, with live play), Arcweave (boards with a play-mode debugger), Twine, RPG Maker MZ's database, CastleDB (typed spreadsheets for game data), and the Unreal, Odin (Unity) and Godot inspectors for editing nested data.

## What the editor shows today

The form, as `form` prints it and the editor draws it, labels every value by its full key path:

```
Ranks
  ranks[1].id = sergeant
  ranks[1].requires.standing = 30.0
Stages
  stages[0].choices[0].effects.standing.factions.lantern_guild = -5.0
Steps
  steps[3].requires.rank_at_least.city_watch = sergeant
```

Relations are entries called `relation[0]` to `relation[6]`, each `between[0]`, `between[1]` and `value`, or `from`, `to` and `value`. A pair of factions can be written as one `between` or two `from`/`to` entries. Nothing on a faction's page shows how it regards anyone.

A questline's steps hold quest ids. Making a quest for a step means going to `quests.toml`, adding it there, then coming back to add its id to the step. The Quests tab is read-only (P-80, D-36), with "Edit in Content".

The REPL is `factional-cli`'s `Session`. Nothing may depend on `cli`, so the editor can't use it as things stand.

## 1. Questlines add their own quests

**What others do.**

- **articy:draft** nests: every flow node can be entered ("submerged into", with Enter) to show and edit what's inside it, and the Navigator indents the deeper level under its parent. A chapter holds its quests, a quest its steps, down to single lines. Their own guidance says one flat layer stops scaling as a project grows: too much panning to find anything.
- **Dialogue System for Unity** adds a quest's entries (its subtasks) inside the quest itself: tick "Has Entries" and add numbered entries in place, each with its own description and state.
- **Twine and Yarn Spinner** create a node where you are in the map and link it in one go.
- **RPG Maker's troops** are edited where they're used: a troop holds up to eight enemies picked and placed in its own panel, with an Auto-name button.

**What this suggests for Factional.**

- A questline's page lists its steps in order, each a card with its quests, as the graph already draws them.
- Each step has **"+ New quest"**, which adds the quest to `quests.toml` (an id typed in, the name, and the stage and choice a quest must have) and its id to the step. It's one change across two files and one Undo.
- Each step also has **"+ Existing quest…"**, which lists only quests in no questline, since a quest is in at most one, at one step.
- Moving a quest between steps, and reordering steps, are menu actions with drag as a shortcut. articy offers both a drag and a menu for everything it lets you drag.
- Choosing a quest opens it nested under the questline, with a breadcrumb such as *A Life in the Watch › Step 2 › Night Patrol*. A quest's page does the same for its stages, and a stage's page for its choices.
- **This is structured editing, not a free canvas,** so it fits D-36. The graph stays a read-only view, with U8's routed lines; the editing happens in the nested pages beside it. Choosing a node in the graph opens its page there, in place of today's "Edit in Content".

## 2. Dot notation and list indexes

**What others do.**

- **Unreal** has the same problem. A list of structs shows "Element 0, Element 1…" with no context until each is expanded. Its fix is `TitleProperty`, which names each item by one of its own fields.
- **Odin Inspector** (Unity) does the same with `ListElementLabelName`, and `TableList` draws a list of objects as a table.
- **CastleDB** gives each sheet a display column, so a reference shows a name and an icon instead of a raw id.
- **articy** groups properties into named features on a property sheet; nobody sees a path.
- **Godot's inspector** nests: each sub-resource is a fold-out under its parent's heading.

**What this suggests for Factional.**

- **Nest, don't flatten.** A table becomes an indented section under a heading, so the path is said once, as structure, not repeated on every row. *Alignment*, then *Law 70* and *Good 20*.
- **Name list items by what's in them, not their place.** The schema marks each list item's title key, as `TitleProperty` does (`x-title`: `id` for stages, choices and ranks; the faction pair for relations). That gives *Stage "inspect" › Choice "seize"*, *Rank: Sergeant*, *City Watch ↔ Temple*. Where a list has no title, items are numbered from 1, as the tables already are (the labels' `[0]` disagrees with them today).
- **Labels in words.** Each key gets a short `title` in the schema, such as *Member tolerance* for `member_tolerance`. The key itself shows in Plex Mono on hover, and is still the AccessKit label, so tests and anyone switching to the TOML can find it.
- **A table keyed by ids is a small table.** `standing.factions.city_watch = 5.0` becomes a *Standing* table with a row for each faction (*City Watch +5, Lantern Guild −5*) and "+ Faction…" to add one.
- **A pair is a pair.** `between = ["a", "b"]` is two pickers with ↔ between them, not `between[0]` and `between[1]`.
- **Requirements and effects read as sentences,** the words the quest graph already uses (P-80 moved them to the content crate). *Needs standing 40 with City Watch* and *Rank Sergeant or higher in City Watch*, each with its fields inline, and "+ Requirement…" listing the kinds.
- **Problems use the same words.** A problem at `stages[0].choices[1].next` reads *Stage "inspect", choice "wave_through": next…*. It still links to the field, and the raw key is still there on hover.
- **The CLI keeps paths.** `form` keeps printing key paths, since that's what a CLI user types. The labels are an extra column from the same `entry_form`, so scenarios can cover them.

## 3. Relations, one per pair, on each faction's page

**What others do.**

- **The Creation Kit** edits relations in each faction's own record, in an Interfaction Relations section. It lists the other factions, each with a reaction (Neutral, Ally, Enemy or Friend), and any faction not listed is neutral. There's no separate relations file to keep in step.
- **CastleDB and other spreadsheet tools** suggest the overview: a faction-by-faction matrix, like the disposition matrix already in Previews.

**What this suggests for Factional.**

- **One relation per pair of factions,** checked at load: a pair written twice is a problem, with the second place named. That's checkable (D-20) and makes "1:1" a rule rather than a habit.
- **Each faction's page gets a *Relations* group** listing every other faction. Each row has the value it holds toward them, the value they hold back, the band's name (from `relations.bands`), and a mark where the pair is in conflict. A faction with no relation written shows *neutral, 0* with Set, like any default. A new faction appears on every other faction's page at once.
- **One place written, two places shown.** Editing the Temple's row on the City Watch's page writes the same relation the Temple's page shows. The pages are views; the text stays in one place.
- **Where the text lives is open (O-5 below).** Keeping `relations.toml` (one `[[relation]]` per pair) means each relation has exactly one home. Moving relations into `factions.toml`, as the Creation Kit does, puts them where they're read, but a pair that's the same both ways is then either written twice or owned by one side.
- **Whether a grudge can be one-way is also open (O-6).** Riverhold uses one: the Watch regards the Free Company at −30, and they regard it at −10. DESIGN.md §9.4, `ShiftRelation` and quests' `from`/`to` effects all allow it. A row can keep both values with a "same both ways" link, linked by default.
- **A Relations matrix in Previews,** each cell tinted by its band like the disposition matrix, shows the whole web at once and links to each pair.

## 4. A REPL in the editor

**What others do.**

- **Inky** has its play pane beside the script. As you type it recompiles and fast-forwards to the point you'd played to, keeping your choices.
- **Yarn Spinner's Live Preview** runs dialogue in the real runtime inside VS Code, so you can step through choices and watch variables change.
- **Arcweave's Play Mode debugger** shows each variable's value before and after the current element. You can edit them mid-play to test a branch without replaying, changes are coloured by what caused them (script or you), and "Undo last choice" steps back.
- **Godot's Evaluator** is a REPL in the debugger. Its weakness is that it only works while the game is stopped at a breakpoint.
- **RPG Maker's Battle Test** and **Twine's "Test from here"** start play from where the designer is (a troop, a passage) instead of from the beginning.

**What this suggests for Factional.**

- **A Play console in the editor**, running the REPL's own commands against a world built from the content in memory, unsaved edits included, as Previews are (D-35). It has history, completion of commands and of every id from the outline, and output in the CLI's words with ids as links to their entries.
- **Content changes replay the session** with `reload`'s rules (P-52): the journal re-runs on the new content. Like Inky's fast-forward, the designer stays where they were, told which command first comes out differently, if any.
- **Watches**, pinned queries such as `disposition player city_watch` or `standing player city_watch`, are re-run after every command, each showing the change it just saw. This is Arcweave's debugger for Factional. The engine's explanations come with them, as `--explain` gives them, never worked out by the editor.
- **"Save as scenario"** writes the session's commands to `scenarios/<name>.scenario`, ready for asserts, so exploring in the editor turns into a test.
- **Play from here.** A quest's page offers "Can the player start this?" and "Start it in Play". A character's page offers "Act as…". Each is a REPL command typed into the console, so it's visible and repeatable.
- **What it needs underneath (O-7).** `Session` moves out of `factional-cli` into a library crate the CLI and the editor both use (say `factional-session`, beside `content`), with loading from texts in memory as well as from a directory. The other way, the editor running `factional repl` as a child process, needs no move, but can't see unsaved edits and can't link ids in its output.
- **Where it sits (O-8).** Either a fourth tab, Play, which amends D-34, or a drawer along the bottom of every tab, as the problems panel is, so it can sit under the form being edited.

## Other improvements the research points to

Roughly in order of value for effort.

1. **Rename an id everywhere.** `references` already finds every place an id is named (U6a), so renaming a faction can offer to update all of them as one undoable change. Every IDE has this. Today a rename leaves a trail of "did you mean" problems to fix one by one.
2. **Pickers that search and show names.** Today the `form` output lists all eleven choices inline for a giver. Instead, a searchable picker shows *City Watch* · `city_watch`, and only what fits there: articy's pickers list only the objects a slot allows.
3. **Make what's missing from where you are.** Typing an id that doesn't exist into a picker offers "+ New faction 'x'…", as Twine and Yarn make a node by naming it.
4. **Quick open (⌘K).** Jump to any entry by name or id, across files: articy's Navigator search, or an IDE's Go to Symbol.
5. **Back and forward,** with a breadcrumb, now that links between entries (Referenced by, problems, ids in Play) move you around.
6. **Previews where you're working.** A faction's page shows its alignment map, and a character's page shows how each faction regards them. These are the same engine queries as the Previews tab, next to what changes them.
7. **Reorder with arrows and drag.** Ranks, stages, steps and choices are ordered, and order means something. articy's features have up and down arrows; Godot has no built-in reordering, and its users ask for it.
8. **Duplicate an entry,** under a new id, for the second of anything.
9. **Keyboard:** ⌘S, ⌘Z and ⇧⌘Z, ⌘K, Esc to close a menu.

## What it would take

How it might be split, smallest first, for the design pass to settle. Each is one concept. U8 is taken by the graph routing (#62), so these start at U9. E1, the Bevy plugin, is Next in PLAN.md; whether these go before it or after is the user's call.

| Increment | Builds | Needs |
| --- | --- | --- |
| U9 | Design pass: this note into DESIGN.md §18, with the decisions below | the user's answers to O-5 to O-8 |
| U10 | Labels and nesting: `x-title` and `title` in the schema, `entry_form` rows with labels and a nested structure, sections in place of paths, pairs and id-keyed tables drawn as such; `form` gains the labels | U9 |
| U11 | Requirements and effects as sentences, with "+ Requirement…" and "+ Effect…" | U10 |
| U12 | Relations: one per pair, checked at load; the faction page's Relations group; the matrix in Previews | U9 (O-5, O-6) |
| U13 | Questlines edit their quests: steps as cards, "+ New quest" and "+ Existing quest…", moving and reordering, nested pages with a breadcrumb; changes across two files as one Undo | U10 |
| U14 | `factional-session`: `Session` out of the CLI, loading from texts in memory | U9 (O-7) |
| U15 | The Play console: commands, history, completion, ids as links, replay on change | U14 |
| U16 | Watches and "Save as scenario" | U15 |
| U17 | Rename everywhere, searchable pickers, "+ New … " from a picker | U10 |

Back and forward, quick open, reordering and duplicating can join whichever of these first needs them.

## Decisions, as settled in U9

The user's answers on 2026-10-11: O-5 went the other way from the recommendation, and O-6 to O-8 as recommended.


- **O-5 · Where relations are written: in `factions.toml` (D-37).** Keep `relations.toml`, one `[[relation]]` per pair, with each faction's page a view of it (recommended: one home per relation). Or move them into each faction in `factions.toml`, as the Creation Kit does.
- **O-6 · What "1:1" means: a value each way, linked by default (D-38).** One relation per pair that can still hold a different value each way, linked "same both ways" by default (recommended: keeps §9.4's grudges, Riverhold's Watch and Free Company, and `from`/`to` effects). Or one value per pair, the same both ways, which drops one-way grudges from content and from quests' effects.
- **O-7 · How the editor gets the REPL: `factional-session` (D-39).** A new `factional-session` crate shared by the CLI and the editor (recommended: sees unsaved edits, links ids, tested once). Or the editor runs `factional repl` as a child process.
- **O-8 · Where Play sits: a drawer along the bottom (D-40).** A drawer along the bottom of every tab (recommended: play beside the form you're changing). Or a fourth tab, amending D-34.

X-6 is settled (D-36): the graphs stay read-only. §1's nested pages keep to that, so it needn't be reopened.

## Sources

- articy:draft: [the Flow](https://www.articy.com/help/UI_View_Flow.html), [nesting](https://www.articy.com/en/improve-project-structure-with-nesting/), [slots and strips](https://www.articy.com/help/adx/Templates_Reference.html), [template properties](https://www.articy.com/help/adx/Templates_Properties.html)
- Creation Kit: [Faction](https://ck.uesp.net/wiki/Faction), [Factions tab](https://ck.uesp.net/wiki/Factions_Tab)
- Dialogue System for Unity: [quests](https://www.pixelcrushers.com/dialogue_system/manual2x/html/quests.html)
- Yarn Spinner: [for VS Code](https://yarnspinner.dev/docs/yarn-spinner-editor/), [the editor](https://yarnspinner.dev/editor/)
- Inky: [inkle/inky](https://github.com/inkle/inky)
- Arcweave: [Play Mode debugger](https://docs.arcweave.com/project-tools/play-mode/debugger), [Play Mode tutorial](https://docs.arcweave.com/tutorials/basic/pt-4)
- Twine: [testing](https://twinery.org/reference/en/publishing/testing.html)
- RPG Maker MZ: [database](https://rpgmakerofficial.com/product/MZ_help-en/01_08.html)
- CastleDB: [castle](https://lib.haxe.org/p/castle), [tutorial](https://gamefromscratch.com/?p=6895)
- Unreal: [TitleProperty](https://www.tomlooman.com/unreal-engine-uproperty-meta-specifier-title-property/), [details panel customisation](https://dev.epicgames.com/documentation/unreal-engine/details-panel-customizations-in-unreal-engine)
- Odin Inspector: [ListDrawerSettings](https://odininspector.com/attributes/list-drawer-settings-attribute), [TableList](https://odininspector.com/documentation/sirenix.odininspector.tablelistattribute)
- Godot: [debugger panel and Evaluator](https://docs.godotengine.org/en/4.5/tutorials/scripting/debug/debugger_panel.html), [custom list reordering](https://forum.godotengine.org/t/how-to-display-a-custom-list-similar-to-array-in-editing/101126)

# Factional — plan

This is the queue of increments for the reputation & factions module.

- Design: [DESIGN.md](DESIGN.md)
- Decisions: [DECISIONS.md](DECISIONS.md)
- Workflow: [CLAUDE.md](../CLAUDE.md)

## Priorities

| Priority | Covers |
| --- | --- |
| **P0** | The core loop from the brief: alignment moved by actions, disposition, joining by similarity, relations, standing, rank, joining an enemy. |
| **P1** | Refinements already asked for: inertia, target-aware morality, drift, spillover, conflicts, modifiers, designer tooling. |
| **P2** | Wanted later: knowledge ripple, double agents, saves, visual tooling. |
| **P3** | After this module: embedding in a host engine. |

## Statuses

| Status | Meaning |
| --- | --- |
| **Next** | The increment to pick up now. |
| **In progress** | Being built on a branch. |
| **Done** | Shipped; compressed to one line under Done. |
| **Blocked (O-n)** | Waiting on an open decision. |
| **Deferred (X-n)** | Waiting on a decision the user has chosen to leave until later. |
| **Outline** | Detail and acceptance examples get written when the increment nears the front of the queue. |

## Order

The order below is the source of truth. Sections further down are grouped by phase for easy scanning, not in delivery order.

`F0 → F1 → F2 → A1 → A2 → A3 → D1 → D2 → M1 → M2 → M3 → M4 → M5 → M7 → A4 → A5 → D3 → M6 → M8 → M11 → M9 → M10 → T1 → T2 → T3 → T4 → K0 → K1 → K2 → K3 → K4 → K5 → E0 → Q0 → Q1 → Q2 → Q3 → Q4 → Q5 → Q6 → Q7 → U0 → U1 → U2 → U3 → T6 → U4 → U5 → U7 → U6a → U6b → U6c → U6d → U6f → U6e → U6g → U8 → U9 → U10a → U10b → U10c → U10d → U11a → U11b → U12a → U12b → U12c → U13a → U13b → U13c → U13d → U14a → U14b → U14c → U15a → U15b → U15c → E1 → G0 → W0 → U19 → G1 → G2 → G3 → G4 → G5 → G6 → G7 → G8 → G9 → W1 → W2 → W3 → W4 → W5 → W6 → W7 → W8 → W9 → W10 → W11 → U15d → U16a → U16b → U20 → U17a → U17b → U17c → U17d → U17e → U17f → U22 → U18 → U21 → T5`

## Done

- F0 — workspace of four crates with lints and a pinned toolchain, CI with mutation testing on PRs, the `factional` CLI (`repl`, `run`), and the scenario harness; done 2026-10-04 (#1)
- F1 — `Fixed`, the two-decimal number every rule uses: exact parsing and display, arithmetic rounded half away from zero, checked overflow, TOML input; plus the `calc` CLI command; done 2026-10-04 (#2)
- F2 — `Curve`, the piecewise-linear shape of most tuning knobs: validation, evaluation rounded once, a bounds check, TOML input; plus the `curve <curve> at <x>` CLI command; done 2026-10-04 (#3)
- A1 — characters with a two-axis alignment and its nine-box label; loading `balance.toml` and `characters.toml` with every problem reported at once, by file and key path, with "did you mean" hints; Riverhold's sample characters; `load`, `characters`, `show character`; done 2026-10-04 (#4)
- A2 — the command-and-event core: `World::execute` (refused commands change nothing), numbered and time-stamped events, `World::replay`, the journal, `AdvanceTime`; `advance`, `time`, `events`, `journal`; done 2026-10-04 (#7)
- A3 — actions move alignment: `actions.toml` (alignment deltas), `PerformAction` with scale and witnesses, `ActionPerformed` and `AlignmentChanged`, clamping at the ends of each axis, refusals with "did you mean"; `actions`, `act`; done 2026-10-04 (#8)
- D1 — weights and distance: `factions.toml` (name, alignment, weights), character weights, `alignment.metric` and `alignment.default_weights`; an exact `distance(observer, subject)` query with its working; one id namespace for factions and characters, checked by `World::new` (P-32's mechanism, arriving early); `factions`, `show faction`, `distance [--explain]`; done 2026-10-05 (#9)
- D2 — disposition from affinity: `disposition.affinity` (a curve, checked within ±100 by `World::new`) and `disposition.bands` (validated: valid unique names, increasing `up_to`, only the last open-ended); a `disposition(observer, subject)` query with score, band and working; `disposition [--explain]`; plain "expected a number" errors; done 2026-10-05 (#10)
- M1 — factions' `tolerance` and `member_tolerance`, starting `memberships`; `JoinFaction` (refused with every failing check) and `LeaveFaction`; `assess_join` and membership queries; load warnings, starting with members outside member tolerance; `can-join [--explain]`, `join`, `leave`, memberships in `show`; done 2026-10-05 (#11)
- M2 — relations between factions: `relations.toml` (`between` and `from`/`to`), relation bands and `conflict_threshold`; `SetRelation` and `ShiftRelation` with `RelationChanged`; enemy exclusion in `assess_join`; invariant 6 kept by refusing a war between a character's own factions until M9; `relations`, `relate`; done 2026-10-05 (#12)
- M3 — standing: a store of how factions and characters regard each character; starting standing, action `standing` effects (target, target factions, named), `outcomes.toml`, `ApplyOutcome`, `ApplyEffects` and `StandingChanged`; the `awareness` seam (1.00); `leave_standing_change`; `standing`, `outcomes`, `outcome`; done 2026-10-05 (#13)
- M4 — the full disposition: affinity, standing, kinship (with `same_faction`), faction opinion and modifiers (0 until M10), each clamped to ±100, weighted by `disposition.weights` and summed; `disposition --explain` shows DESIGN.md §8.2's table, with where kinship and faction opinion came from; done 2026-10-05 (#14)
- M5 — rank ladders: `[[<faction>.ranks]]` with standing requirements and stricter tolerances, starting ranks, new members on the lowest rung; `Promote` (only on request, refused with every unmet requirement) and `Demote`, with `RankChanged`; `assess_promotion`; rank load checks and warnings; `ranks`, `promote [--explain]`, `demote`; done 2026-10-05 (#16)
- M7 — joining an enemy: `membership.defectors` and `membership.deserters` rule tables, and a faction's own, which may name its ranks; built in, defectors refuse everyone and deserters release everyone (D-4); load checks for conditions, outcomes, rungs, rank ids and a last rule that always decides; `assess_join` reports every rule tried in each table; defecting emits `LeftFaction(defected)` with the deserter cost, then `JoinedFaction` with the defector cost; `can-join --explain` shows each table; done 2026-10-05 (#18)
- A4 — inertia: `[inertia]` profiles (up to four curves per profile, a curve left out is 1.0, `steady` always there) and `default_profile`, a character's `inertia`; each shift is base × scale × inertia, computed exactly and rounded once (`Ratio` in core, `Curve::exact_at`), for actions, outcomes and effects; load checks for unknown profiles and negative curves; a property test for invariant 8; a `shift` query; `act --explain`; done 2026-10-05 (#19)
- A5 — target-aware effects: an action's `by_target.law`, `.good` and `.relation` curves (the most hostile relation from the actor's factions toward the target's), joining inertia in one exactly computed product rounded once; load check for negative curves; an `action_shift` query that `decide` uses; `act --explain` shows each target multiplier and where it came from; a property test for invariant 8 with every multiplier; done 2026-10-05 (#20)
- D3 — watched subjects: `Watch` and `Unwatch` (the starting bands travel in `Watched`, so replay runs no rules); after every accepted command, `DispositionBandChanged` for each observer whose view of a watched subject left its band, with `disposition.hysteresis`; `Bands::band_after`; a property test that with no hysteresis the remembered band is always the current one; `watch`, `unwatch`, `watching`; done 2026-10-06 (#21)
- M6 — standing spillover: `standing.spillover` (a curve within −1…1, on by default); every standing change with a faction spills one hop to every other faction by how it regards the first, from the change as applied, rounded once, adding up with direct changes; `StandingChanged` carries what spilled and from where, and the CLI says so; done 2026-10-06 (#22)
- M8 — drift policies: a faction's `drift` (`ignore`, `flag`, `demote`, `expel`), `membership.default_drift` (built in, `flag`) and `expel_standing_change` (−20.00, spilling like any standing change); members whose alignment a command moved are reviewed against each faction, before band changes; `MemberOutOfTolerance`, `MemberBackInTolerance`, `LeftFaction(expelled)`; demotion steps down to the highest rung that allows them; drift in faction listings; split from M11; done 2026-10-06 (#23)
- M11 — probation and runtime faction alignment: the `probation` policy (`grace_ticks`, then `demote` or `expel`) with `ProbationStarted`, `ProbationCleared` and `ProbationExpired`, checked when time advances; `SetFactionAlignment` and `ShiftFactionAlignment` with `FactionAlignmentChanged`, reviewing every member; faction alignment is now state, used by distance, disposition and joining; `faction-align`, `faction-shift`; property tests for probation; done 2026-10-06 (#24)
- M9 — war between your own factions: a relation change that puts two of a character's factions in conflict is accepted and opens a `MembershipConflict` (ending with `MembershipConflictEnded` if they make peace first), replacing M2's refusal; `ResolveConflict` leaves the other side at its `leave_standing_change`, which spills; `membership.conflict` (`ask`, `ask` with `auto_after_ticks`, or `auto`) keeps the higher rung, then standing, then service, then the lower id; invariant 6 allows open conflicts; `resolve`, wars in `show character`; done 2026-10-06 (#25)
- M10 — disposition modifiers: `AddModifier` (for everyone, a faction and its members, or one character; within ±100; with an optional expiry) and `RemoveModifier`, with `ModifierAdded`, `ModifierRemoved` and `ModifierExpired`; expiry checked after every command; the modifiers component adds up those that apply, clamped to ±100, and `--explain` names each; `modify`, `unmodify`, `modifiers`; done 2026-10-06 (#26)
- T1 — `factional validate <dir>` (every problem and warning as `load` gives them, a summary line, exit 1 if it wouldn't load) and `validate <dir>` in the REPL; the warning for a faction no one starts within tolerance of, naming the nearest; "a rank no one can reach" dropped (P-51); `factional schema [<file>]`, a JSON Schema per content file built from the engine's own keys, enumerations, ranges and defaults, checked in under `schema/` and tested both ways against the sample, the complete example and the fixtures; `content/README.md`; done 2026-10-06 (#27)
- T2 — `factional compare <scenario> --content A --against B`: the scenario's one load swapped, asserts run unchecked, then each character's changed alignment, standings and memberships and watchers' changed dispositions as `A → B`, and the first event where the runs diverge; `reload` in the REPL, replaying the whole journal on the re-read content and changing nothing unless every command comes out as before (P-52); done 2026-10-07 (#28)
- T3 — `map <faction>` (the alignment plane, 21 by 21 cells: the faction's tolerance region, the faction and each character, with a key of distances; `World::distance_to_point`), `matrix [<subject>...] [--csv]` (every observer's disposition score toward each subject), and `curve <knob> [at <x>]` for the world's named curves, or any curve, as a table or at a point (`Curve::points`) (P-53); done 2026-10-07 (#29)
- T4 — saves: `save <file>` and `restore <file>` in the REPL; a JSON file with its format and version first, the content directory with each file's FNV-1a fingerprint, the journal as commands with their event counts, and the events; `World::saved_journal` and `World::restore` (events replayed without rules, refusals decided again); exact serde for `Fixed`, `Ratio`, ids and alignments; restoring refuses changed content, other versions and saves that don't add up; invariant 11 (P-54); done 2026-10-07 (#30)
- K0 — design pass for knowledge (DESIGN.md §10): who learns firsthand, ripple through contacts and membership with news in flight, perceived alignment used by every judgement, secret membership and exposure tables; the user's choices D-21 to D-24 (settling X-1), with P-55 to P-58; invariants 12 and 13; Riverhold's knowledge settings in the complete example; done 2026-10-07 (#31)
- K1 — witnessed acts: `knowledge.model` (`omniscient`, `witnessed`), with Riverhold on `witnessed`; only parties that learn firsthand (witnesses, the parties an act names, and the factions of the characters among them, never through the actor) change their standing; a `reach` query; `act --seen-by <id>,… | --unseen`, and `act --explain` saying who learns; invariant 12's property test as far as K1 goes (P-59); done 2026-10-07 (#32)
- K2 — ripple: `knowledge.model = "ripple"`, with `[knowledge.ripple]` `strength` (a list per hop, the user's choice over decay and threshold, P-56) and `hop_ticks`, and characters' `contacts`; `NewsSent` and `NewsArrived`, news in flight delivered as time advances, standing scaled by each hop's awareness; Riverhold on `ripple` with DESIGN.md §13's contacts; the `news` query and command; content checks and a warning; invariant 13 and replay, restore and invariant 12 under ripple as property tests (P-60); done 2026-10-07 (#33)
- K3 — perceived alignment (D-21): everyone judges by their picture of a character, the truth less the shifts they haven't heard of; `ShiftWitnessed`, and the shift carried by news at each hop's strength; `distance` measures the picture, so disposition, joining, promotion and drift follow, and each rule table uses its own faction's picture; drift reviewed as news arrives; `witnesses` on outcomes and effects; the `perceived` query and command, explanations and `map` showing picture against truth; the journal now writes who saw each act and outcome; property tests that everyone pictures the truth under `omniscient` or when everyone saw everything (P-61); done 2026-10-07 (#34)
- K4 — secret membership (D-23): factions' `secret_members` (needs `witnessed` or `ripple`), secret starting memberships and `JoinFaction { secretly }` with `assess_join_secretly`; a secret membership is known only to the faction, its members and the character, so kinship, joining and wars count only the memberships each side knows of, and a double agent keeps both; invariant 6 amended, with a property test under ripple; `join`/`can-join --secretly`; the Guild and the Circle allow secret members; split from K5 (P-62); done 2026-10-07 (#35)
- K5 — exposure (D-24): `Expose { character, faction, witnesses }` and `MembershipExposed`, the news of it rippling; the `exposed` rule table (`keep`, `demote`, `expel`; built in, expel) judged by each faction that learns and is at war with the secret one, `LeftFaction { Exposed }`, and wars opening between memberships each side knows of; `assess_exposure`; `expose … [--seen-by] [--explain]`; Riverhold's exposed table, with the double agent made in play rather than at the start (P-63); done 2026-10-07 (#36)
- E0 — host engine: the user chose not yet, so X-2 stays deferred and the module stays engine-agnostic; E1 and T5, which need a host, wait at the end of the queue, and Q0 is next; done 2026-10-07 (#37)
- Q0 — design pass for quests (DESIGN.md §17): quests of stages with choices, gated by requirements such as rank or standing; questlines of steps, each a group of quests done in any order, `need` of them (possibly 0) to move on, leftovers kept or closed as the designer chooses; given by a faction, a character or no one; reconciling as every lockout declared in `locks`, found by conservative bounds checked choice by stage and gate, never combinations; the user's choices D-25 to D-29, settling X-4, with P-64 to P-66; Q1 to Q5 outlined; done 2026-10-07 (#38, #39)
- Q1 — quest content: a new `factional-quests` crate (between reputation and content; ids from a macro now in core); `quests.toml` (name, giver, gate, stages with requirements and choices with an outcome or inline effects and `next`) and `questlines.toml` (giver, steps with `quests`, `need`, `requires`, `leftovers`); every reference and range checked with a "did you mean", once every file reads cleanly; a warning for leftovers that can't be left; content with quests doesn't load until Q4; `quests <dir> [<quest>]`; the two JSON Schemas; Riverhold's quests in the complete example (P-67); done 2026-10-08 (#40)
- Q2 — relation effects (D-30: quests never change memberships or ranks; joining, leaving and promotion are the character's own actions): `relations` shifts on outcomes and choices' inline effects, `between` or `from`/`to` with `by` (−200…200), applied after standing, each direction stopping at ±100 and shifted at most once; checked at load and by `ApplyEffects`; wars open as for `relate`; `outcomes` and `quests` show them; Riverhold's `sowed_discord`; DESIGN.md §17's effects and lockout table without joins and promotions (P-68); done 2026-10-08 (#41)
- Q3 — reachable content: within each quest, every stage reached by a choice, no gate needing `member` or a rank and `not_member` of one faction, and `done` on its own progress only where it can have happened; then a generous run of what `done` and questline order allow, with held runs for steps that close leftovers and stages needing another quest, reporting quests that can never start and stages that can never be reached, each error certain; leftovers close on starting any later step (P-69); `scenarios/reachable.scenario`; done 2026-10-08 (#42)
- Q4 — the lockout check (D-26, D-27): `locks` on choices (`quest` for its gate, `quest.stage`), checked like references; once per world, the standing some action raises directly, the ways some action moves each axis with no inertia profile stopping it, and each relation's reach; per choice, the parties it can lower directly or by one hop of spillover, the ways it moves each axis, and the wars it can start; each choice against every other quest's gate and stages, requirement by requirement, skipping quests certainly finished first; probation undone where every axis has a way back; undeclared lockouts errors, stale locks warnings; `quests` shows locks; Riverhold's Ashen Rite, with its war and the Long Winter's spilt standing declared; split from loading, now Q5 (P-70); `scenarios/lockouts.scenario`; done 2026-10-08 (#43)
- Q5 — worlds with quests load (D-20): the gate comes off; one load reads content and quests, refusing both on any problem; the session keeps the quests for Q6, read again by `reload` and `restore`, and `quests` alone lists them; quest warnings come with the world's; `validate` counts quests and questlines; saves fingerprint the quest files, a save from before reading as having none; Riverhold's quests join `content/sample` with `turned_in_vex` and `took_a_bribe`, and the complete example loads (P-71); done 2026-10-08 (#44)
- Q6 — playing quests: a quest log in `factional-quests` keeping every character's progress, changed only by `StartQuest` and `MakeChoice` through their events (`QuestStarted`, `StageReached`, `ChoiceMade`, `QuestFinished`, `QuestClosed`); starting needs its gate and every step before complete, and closes earlier steps' untouched leftovers; a character waits at a stage until its requirements hold (D-31); effects go to the world in the same command, so a refused command changes nothing; `assess_start` and refusals say what's missing, with the numbers; save and reload refuse quest progress until Q7; `can-start`, `start`, `choose`, `progress`; invariants 14 and 15 with a property test (P-72); `scenarios/play.scenario`; done 2026-10-08 (#45)
- Q7 — saving quest progress: the quest log keeps a journal (each command, where it came in the world's, what it sent and produced); saves, now version 2, hold it and the quest events, restored without running rules once checked to fit, and version 1 saves restore with no progress; `reload` re-runs quest commands where they came among the world's, stopping on any change of acceptance and reporting changed progress; `save` and `restore` count quest commands and events; invariant 11 extended, with a property test (P-73); `scenarios/quest_saves.scenario`; done 2026-10-08 (#46)
- U0 — editor and host engine design pass: the user chose egui for the editor (D-32, settling X-5) and Bevy as the host engine (D-33, settling X-2); DESIGN.md §18 and §19, with P-74 and P-75; U1 to U4 outlined, E1 and T5 no longer deferred; done 2026-10-08 (#47)
- U1 — the editor shell: `factional-editor <dir>`, an eframe app showing each content file, its entries, and every problem and warning at its entry, with the selected entry's TOML and a button to read again; the outline it shows is `factional-content`'s `outline(dir)`, also printed by `outline <dir>` in the REPL; UI tested headless with `egui_kittest` (P-76); `scenarios/outline.scenario`; done 2026-10-09 (#48)
- U2 — editing values: a format-preserving writer in `factional-content` (`set_value`, `entry_fields`, through `toml_edit`), changing one value read as the kind already there and nothing else; the outline made from texts in memory and showing each entry as written; the editor's fields for an entry's values, with problems updating on every change, undo, save and discarding; split from adding and removing, now U3 (P-77); done 2026-10-09 (#49)
- U3 — adding and removing: the writer's `additions` (from the schema: keys not yet there, in its order; an id where it takes any; a list item while there's room), `add` (each new value at its starting value: the default, else the least the schema allows, with the keys and items it must have; tables with their own headers at the top and among headed tables, inline elsewhere) and `remove` (with the comments directly above; those set apart by a blank line stay); `file_places` and `entry_places`; the editor's buttons for each, new ids typed in, files not there started by adding to them, the new entry selected, all undoable; a round trip over every place in the sample (P-78); done 2026-10-09 (#50)
- T6 — nothing reported only because something else couldn't be read: a faction or character with a problem keeps its id for the checks across files, a file that doesn't parse hides no reference to what it might hold, and a `balance.toml`, profile or `knowledge.model` that can't be read judges nothing by its default; the Watch's tolerance set above its member tolerance reported fourteen problems and an unclosed `[temple` thirty-four, now one each; `scenarios/unread.scenario` (P-79); the editor's workspaces are tabs (D-34); done 2026-10-10 (#51)
- U4 — quests in the editor: tabs along the top (D-34), Content and Quests; `QuestGraph` in `factional-content`, from the quests as far as they read, with edges (`needs`, `locks`), each questline's outsiders one hop away, and every quest problem and warning at its questline, step, quest, stage or choice; the quest wording moved there from the CLI; the Quests tab draws questlines as steps in columns and quests as stages, marks nodes by their problems, lists edges, and shows the chosen quest with "Edit in Content"; drawn with egui's painter rather than `egui-snarl` (P-80); `graph <dir> <questline|quest>`, `scenarios/graph.scenario`; U6 outlined for the rest of the design canvas; done 2026-10-10 (#52)
- U5 — previews: `World::alignment_map` and `World::disposition_matrix`, which the CLI's `map` and `matrix` now render from, unchanged; the editor's Previews tab with the matrix, each score tinted by its band, and a chosen faction's map with its key; previews from the last world that loaded, marked while the content doesn't load (D-35, the user's choice); curves and quests split off as U7; snarl's revisit deferred as U8 (X-6) (P-81); done 2026-10-10 (#53)
- U7 — previews of curves and quests: `World::named_curves`, which the CLI's `curve` now renders from, unchanged; in the Previews tab, a chosen knob plotted through its points with each listed in words (or left out, 1.00 everywhere), and whether a chosen character can start a chosen quest at the start of play, in the engine's words (P-82); done 2026-10-10 (#54)
- U6a — what the Content tab needs from the content crate: the schema marks what each id names (`x-names`, `x-names-keys`); `references`, `referenced_by` and `defined_in` read the text with it, held by a test to exactly the places the loader checks in every directory that loads; `key_info` gives a key's description and default; outline entries carry their `name`; `references <dir> <id>`, `scenarios/references.scenario`; U6 split into U6a to U6c (P-83); done 2026-10-10 (#55)
- U6b — the Content tab's form, to board 1: the schema groups each entry's keys (`x-groups`, `x-group`); `entry_form` gives an entry's values group by group, each with the comments above it and after it, the defaults of what's left out and its choices (the ids that exist, an enumeration, or true and false); the tab shows the form under the entry's name with Set for a default and a choice menu, and beside it what the loader says, what refers to the entry (each a link to it) and the entry as written; the browser shows names; the window opens at the canvas's 1440 by 900; `form <dir> <file> <entry>`, `scenarios/form.scenario`; board 2 split off as U6d (P-84); done 2026-10-10 (#56)
- U6c — problems at their keys, with the loader's fixes, to board 3: `Suggestion` in `factional-core`, offered by every engine error and content reader that says "did you mean" and carried on each `Diagnostic`; `fix_for` finds the change in the text (a misspelt key renamed, a misspelt word or part of a quest path set), `apply_fix` makes it through the writer, which gains `rename_key`; the outline lists `every_problem` and `every_warning`; the Content tab's problems panel, each with its key as a link and "Use 'right'", each problem also under its field, and the entries narrowed to those with problems; `fixes <dir>`, `scenarios/fixes.scenario`, fixture `typos`; board 6's look split off as U6e (P-85); done 2026-10-10 (#57)
- U6d — adding and removing, to board 2: `KeyInfo::kind` says what a key is in words (text, true or false, a number or whole number with its range, one of an enumeration, the id of what it names, a table, a list); each place's additions under one "Add to <place>…" menu with each key's kind, default and description; × to remove; an entry that something names is removed only after "Remove anyway"; the last change and the problems it brought, with "Show it" and "Undo it"; `ValuePath::then` public; `keys <dir> <file> [<path>]`, `scenarios/keys.scenario`; lists of tables and new entries split off as U6f (P-86); done 2026-10-10 (#58)
- U6f — lists of tables as tables, to boards 1 and 2: `layout::blocks` works out which of a group's fields make a table (a list whose items are flat tables, such as `ranks` or `memberships`; a list of values or with lists inside stays rows); each drawn as a row an item and a column a key, each cell its field labelled with its key, its choices, problems or default, its comment under the row, and × and "Add to <item>…" for each item; board 2's separate new-entry panel not built (P-87); done 2026-10-10 (#59)
- U6e — the visual language, to board 6 without its fonts: `theme::visuals()`, egui's dark visuals with the canvas's colours, set as the editor draws, with the editor's own (loads, locks, comment green, a problem's fill); comments in comment green; the summary as a pill; a value changed since the last save marked amber beside its key and around its field, with its file "edited", until Undo or Save; a refused value or one with a problem bordered red; a quest node with a problem filled red; a table row's "Add to" shown as "+…"; IBM Plex split off as U6g (P-88); done 2026-10-10 (#60)
- U6g — IBM Plex in the editor, to board 6: Plex Sans Regular and SemiBold and Plex Mono Regular in `crates/editor/fonts/` with `OFL.txt` (from IBM's `@ibm/plex-sans@1.1.0` and `@ibm/plex-mono@2.5.0` releases), built into the editor; `theme::fonts()` puts Plex first in each family with egui's faces after; `theme::text_styles()` gives board 6's sizes; the form's keys and values in Plex Mono; fonts, sizes and visuals set once as the editor first draws (P-89); done 2026-10-10 (#61)
- U8 — quest graphs that don't cross their boxes: every line routed through the gaps between columns, or along a corridor below the boxes when it goes further than the next column, each with its own lane and its own point on a box's side, its turns rounded as drawn; a questline's outside quests in a column after its last step; a quest's end as tall as its tallest stage; a test holds every graph in the repository to no line through a box; settles X-6 as the user chose, without egui-snarl (D-36, P-90); done 2026-10-10 (#62)
- U9 — the usability pass's design, from research into comparable tools (`docs/research/usability-2026-10.md`): the user chose relations written in each faction in `factions.toml` (D-37), one relation per pair with a value each way, linked by default (D-38), the REPL's session as a library (D-39) and Play as a drawer along the bottom (D-40); DESIGN.md §18, with P-91 to P-97; U10 to U17 outlined, before E1; done 2026-10-11 (#63)
- U10a — the form in words (P-92, P-98): a title for every key of every schema, and on lists `x-item` and `x-title`, so a named item stands in for its list (*Rank "sergeant"*) and any other is numbered from 1 (*Step 4*); ids said by the names of what they name; `place_words`, `words_at`, `Word`, `FormRow::words` and `KeyInfo::title`; the Content tab's values under headings in place of key paths, labelled by their last words, a table's columns in words, and problems in the panel and beside the form in words, keys on hover; `form` prints each row's words, and `words <dir> <file> <key>`; `key_info` reads into a curve's points; U10 split, with pairs, tables keyed by ids and the places' "Add to" now U10b; `scenarios/words.scenario`; done 2026-10-11 (#64)
- U10b — each place's adding and removing where it is, at the user's direction (D-41, P-99): `form_layout` puts each place's × and "+" on the first heading that says it, a list of named items' beside what holds it (or its group's name), a table's "+" at its end, and the entry's beside its name; a cell's × removes what it's in within its item; an empty table or list is a row of its own (`form` says `empty`); "+ <item>" adds a list's item at once (`KeyInfo::item`); a table keyed by ids offers those that exist and aren't there yet, by name (`id_choices`, `IdChoice`); Referenced by in words; `keys` says the same of a list's items and an id table's choices; U10b split, with pairs now U10c; done 2026-10-11 (#65)
- U10c — pairs as pairs (P-100): `KeyInfo::pair` for a list of exactly two ids, such as `between`; `Line::Pair` draws its two fields on one line either side of ↔, under the pair's word, with the pair's × beside them; half a pair stays a heading and a row; done 2026-10-11 (#66)
- U10d — every section, with its adding in it, at the user's direction (D-42, P-101): every group of an entry's schema in its form, even empty, with what it offers (`FormGroup::offers`, `Offer`, `Offered`): a list's item, a pick from a list of ids, an id in a table keyed by ids or each within one, or a key not there with no default, none the entry's form rules out; `make_offer` makes one in one change, writing what it's within first; each section ends with its "+" buttons, a place's own "+" not drawn where an offer covers it, the entry's "+…" and the list after the form gone; `form` prints the offers; contacts are "Contact"s; done 2026-10-11 (#67)
- U11a — requirements and effects as sentences (P-93, P-102): the schema says each one's sentence and how many of its words it says (`x-sentence`), such as "Needs standing {value} with {key}" or "Standing with {key} changes by {value}", and a character's standing where it starts; `FormRow::sentence` (`Sentence`) gives the words before and after the value; the Content tab draws it on one line under the headings it doesn't say; `form` prints it with its value; U11 split, with adding by kind now U11b; done 2026-10-11 (#68)
- U11b — requirements and effects added by kind (P-93, P-103): the schema marks tables of kinds (`x-kinds`), a quest's, stage's or step's `requires` and a choice's `effects`; `Offered::Kinds` and `place_kinds` give each kind they take, in words, with each id's choices; the Requirements section offers "+ Requirement…" and a stage's requirements or a choice's effects have it as their "+", each kind by id a submenu; made in one change with what it's within; done 2026-10-12 (#69)
- U12a — relations in each faction (D-37, P-91, P-104): `factions.toml`'s `relations`, keyed by faction id, read one way each, with its checks at `<faction>.relations.<other>`, the engine's warning for one side written alone, and the problem for a `relations.toml` left in a directory; `relations.toml` gone from the sample, the complete example, the fixtures, the schema and the file lists; each faction's Relations section in the form; no entry offered its own id; U12 split, with the faction's pair rows now U12b and the matrix U12c; done 2026-10-12 (#70)
- U12b — each faction's relations both ways (D-38, P-105): the schema marks a table read both ways (`x-both-ways`); `relation_rows` gives every other faction with each way, its band, the conflict and whether the two are linked, read from the text; `set_relation` writes one way or both in one change; the Relations section draws them, with "same both ways", 0 with Set, and the conflict mark, and keeps the comment above them; `form` prints the same; done 2026-10-12 (#71)
- U12c — the relations matrix (P-106): the engine's `relations_matrix`, each faction toward each other with its band and conflict; the Previews tab draws it, tinted by band and marked where a pair is in conflict; `relations --matrix [--csv]`; done 2026-10-12 (#72)
- U13a — a questline's page (P-94, P-107): the Quests tab's right-hand panel shows a questline's steps in order, each a card of what it needs and its quests by name, each opening its own with a way back; "+ Existing quest…" offers the quests in no questline and adds one to the step in one change; U13 split into U13a to U13d; done 2026-10-12 (#73)
- U13b — "+ New quest" (P-94, P-108): each step's card takes a new quest's id; the quest is made in `quests.toml`, started so it reads, and added to the step, as one change to both files, one Undo (`change_files`); a blank or taken id is refused; the new quest opens; done 2026-10-12 (#74)
- U13c — moving quests and steps (P-94, P-109): each step's card moves it up or down and each of its quests to another step, and sets its `need` and `leftovers`, each as one change; `swap_items` in the writer swaps two items of a list, a `[[table]]` with its comments; done 2026-10-12 (#75)
- U13d — nested pages (P-94, P-110): a breadcrumb, *A Life in the Watch › Step 2 › Night Patrol › Stage "walk"*, each part going back there; a quest's page its stages, each counting its choices' problems, and a stage's its choices with theirs; done 2026-10-12 (#76)
- U14a — the session as a library (D-39, P-95, P-111): `factional-session`, the REPL's session and every command module moved out of the CLI unchanged, the scenarios and snapshots the proof; the CLI keeps its binary, line editing and scenario runner; CLAUDE.md's and DESIGN.md §15's dependency direction; U14 split into U14a to U14c; done 2026-10-12 (#77)
- U14b — the session from texts (P-81, P-95, P-112): `Session::load_texts` and `Session::reload_texts` load and reload the world written in texts in memory, unsaved edits included, with `load`'s and `reload`'s checks and output; a typed `reload` reloads what it was loaded from; `load_texts_fingerprinted` in the content crate; done 2026-10-12 (#78)
- U14c — marked ids (P-95, P-113): `Session::marks` gives each id of the loaded world in what the session said, with where it is and what it names, for the editor to link; done 2026-10-12 (#79)
- U15a — the drawer (D-40, P-96, P-114): along the bottom of every tab, with the problems moved into it from the Content tab, each going to its entry from any tab; it closes to a strip and opens again, keeping its height; U15 split into U15a to U15c; done 2026-10-12 (#80)
- U15b — Play (D-40, P-96, P-115): beside Problems in the drawer, the REPL on a `factional-session` session of the content as it stands, loaded from the texts in memory and reloaded with `reload`'s rules each time they load, saying what that says; the last world that loaded while they don't; history by arrow; done 2026-10-12 (#81)
- U15c — Play's links and completion (P-96, P-116): each id Play says a link to its entry; Tab completing the last word typed, from the commands or the world's ids (`Session::complete`), listing them where there's more than one; U15c split, with pages handing commands now U15d; E1 moved next, ahead of U15d to U17, at the user's direction; done 2026-10-12 (#82)
- E1 — the Bevy plugin (D-33, P-75, P-117): `factional-bevy`, in a workspace of its own with its own CI job; the world and quest log as a resource loaded at startup, stopping the app with the loader's problems if it doesn't load; commands in and events out as messages, refusals included, played after the game's systems each frame; time by `AdvanceTime` at a rate the game sets, in Bevy's game time; a headless example on Riverhold; Bevy 0.20.0; done 2026-10-12 (#83)
- G0 — the game builder's design pass (D-43 to D-47, P-118, DESIGN.md §20): research ([research/game-builder-2026-10.md](research/game-builder-2026-10.md)); the user's choices: an app of its own, opening and opened by the editor, an inspector first, then a 2D sandbox, then a world editor, `bevy_egui`, placeholder shapes, a builder file; G1 to G9 outlined; the editor's remaining increments planned, the editor's screenshots first and the builder's increments next, at the user's direction; done 2026-10-12 (#84)
- W0 — world maps from generated worlds, the design pass (D-48 to D-53, P-119, DESIGN.md §21): research ([research/world-maps-2026-10.md](research/world-maps-2026-10.md)); the user's direction: an Azgaar export imported by a wizard, the most fundamental layers first, polished, dressed with foliage and edited by hand; settlements, routes and points of interest as suggestions, not built; 2D tile maps first and 3D later; the most efficient format; places known to the engine, the map the builder's, amending D-47 and P-118; a region at a scale first; W1 to W11 outlined after G9, at the user's direction; done 2026-10-12 (#85)

---

## Phase 1 — Characters and alignment

## Phase 2 — Perception

## Phase 3 — Factions, standing and rank

## Phase 4 — Designer tooling and persistence

### T5 · Binary saves — P3 · Outline

The user's choice for a finished game (P-54): a compact binary encoding of the same save, beside JSON, with the format and version still readable first. After E1, once the Bevy plugin shows what a game needs (D-33). The encoding is the world maps' (P-119).

## Phase 5 — Knowledge and rumour

## Phase 6 — Embedding

## Beyond this module

### U19 · Editor screenshots — P2 · Next

Image snapshots of the editor's key pages, rendered headless and reviewed like the scenario snapshots, so a UI change is seen in its PR, not only driven through AccessKit; the pages: a faction's form and Relations, a questline's page, Play, the Previews tab.

### G1 · The inspector — P3 · Outline

`factional-bevy-inspector`, a plugin over `factional-bevy`, drawn with `bevy_egui`: a window of the world's characters, factions and event log, shown and hidden by a key; the Bevy workspace's members (D-44, D-45, P-118, DESIGN.md §20).

### G2 · The inspector's console — P3 · Outline

The session's commands run on the game's world, with the engine's explanations; Play's log and line drawn by a crate both the editor and the inspector use (P-118).

### G3 · The inspector's quests and relations — P3 · Outline

Each character's quest progress, and the relations matrix, in the inspector.

### G4 · The builder's shell — P3 · Outline

`factional-builder`, opened on a content directory: character tokens and places on a 2D plane, from `builder.toml` (or laid out when there's none), the inspector beside it, time paused, stepped or run (D-46, D-47).

### G5 · Acting in the builder — P3 · Outline

The player moved among the tokens, acting on the nearest from the content's actions, the characters near enough witnessing it.

### G6 · What the builder shows — P3 · Outline

Each token's disposition toward the player, by band; news spreading along contacts (K2), drawn as it goes.

### G7 · Quests in the builder — P3 · Outline

Quest givers marked; quests started and choices made from a panel.

### G8 · The editor and the builder open each other — P3 · Outline

"Play in the builder" from the editor and "Edit in the editor" from the builder, on the same directory, at the entry or character in view (D-43).

### G9 · The world editor — P3 · Outline

A mode of the builder: tokens and places placed, moved and named, saved to `builder.toml` (D-44, D-47).

### W1 · The world map and its file — P3 · Outline

`factional-worldgen` and the map: tiles in 64 × 64 chunks, in layers, with the recipe and the designer's overrides; the map file (a readable header, then postcard, compressed by `lz4_flex` or zstd as measured on a real map), written and read back exactly by `factional-content`; `factional world info <file>`; tested on hand-made maps (D-52, P-119, DESIGN.md §21).

### W2 · Importing Azgaar's terrain — P3 · Outline

Azgaar's Full JSON read, its version checked and its problems named by key path; cells, heights and features; a region at a scale (D-53) rasterised to tiles, heights interpolated across cell centres; land, sea and lake; `factional world import`; fixtures generated at Azgaar's smallest size (P-119).

### W3 · Polishing the terrain — P3 · Outline

Detail from seeded integer noise, following the source's roughness; elevation levels with ledges and cliffs; coasts smoothed; rivers widened from their discharge and carved, lakes at their surface height.

### W4 · Climate and biomes — P3 · Outline

Each tile's temperature, precipitation and biome from the source, and its terrain from those.

### W5 · Foliage — P3 · Outline

Trees, shrubs and grass scattered by Poisson-disc sampling, their kind and density by biome, seeded; each density an option with its default.

### W6 · Suggestions — P3 · Outline

Settlements, routes, points of interest, and cultures and religions as candidate factions, kept as markers with what the source says of them; nothing built from them (D-49).

### W7 · The map in the builder — P3 · Outline

The map drawn with Bevy's `TilemapChunk`, chunk by chunk, in placeholder colours, autotiled on a dual grid; panned and zoomed; suggestions shown. After G4's shell.

### W8 · The wizard — P3 · Outline

In the builder: a file opened and summarised, layers chosen with what they need, a region and scale on a preview, a seed and options, then generated, previewed and saved (D-48).

### W9 · Editing the map — P3 · Outline

Brushes for terrain, elevation, water and foliage in G9's world editor, saved as overrides that survive regenerating; reset to generated in an area; undo.

### W10 · Places in the engine — P3 · Outline

A design pass: what a place is in the content, how places come from the map, and which reputation rules use them (D-51), the user's choices settling what D-47 and P-118 leave to it.

### W11 · The map in 3D — P3 · Outline

A terrain mesh from the elevation layer, foliage as instanced models (D-50).

### U15d · Pages hand commands to Play — P3 · Outline

A quest's "Can the player start this?" and "Start it in Play", and a character's "Act as…", each typing its command into Play, opening it, so it's seen and can be run again (P-96).

### U16a · Watches — P3 · Outline

Commands pinned in Play, run again after every command, each showing what changed, with the engine's explanations (`--explain`) (P-96).

### U16b · Save as scenario — P3 · Outline

Play's commands written to `scenarios/<name>.scenario`, after its `load`, for the designer to add asserts to (P-96).

### U20 · Completion in the CLI's REPL — P3 · Outline

Tab completion in `factional repl`, through rustyline, from `Session::complete` (P-116).

### U17a · Renaming everywhere — P3 · Outline

Renaming an id renames every place that names it, as one change across files, offered with the count of places (P-97).

### U17b · Pickers — P3 · Outline

Id fields picked from a searchable list showing names, *The City Watch* · `city_watch`, offering only what fits there (P-97).

### U17c · Making what's missing — P3 · Outline

Typing an id that doesn't exist into a picker offers "+ New faction 'x'…", made and named in one change (P-97).

### U17d · Quick open and keys — P3 · Outline

⌘K to any entry by name or id, across files; ⌘S, ⌘Z, ⇧⌘Z (redo) and Esc (P-97).

### U17e · Back and forward — P3 · Outline

Through what's been chosen, entries, pages and links alike (P-97).

### U17f · Reordering and duplicating — P3 · Outline

↑ and ↓ for every ordered list (ranks, stages, choices), as steps have (P-109); duplicating an entry under a new id (P-97).

### U22 · Drag to reorder — P3 · Outline

Dragging as a shortcut beside the ↑ and ↓ and menus of U13c and U17f (P-94, P-109).

### U18 · Previews where you work — P3 · Outline

A faction's page shows its alignment map, and a character's how each faction regards them: the Previews tab's engine queries beside what changes them.

### U21 · Problems under relation rows — P3 · Outline

A problem at a faction's `relations`, such as a misspelt id, shown beside its row, as a field's is (P-105).

---

## Adding an increment

Use the same shape as above:

1. The id, title, priority and status.
2. **Why.**
3. **Scope.**
4. **Acceptance** examples. Work their numbers out by hand from DESIGN.md's formulas; never take them from the code's output.
5. The CLI commands it ships.
6. Any knobs it adds. These go in DESIGN.md §12 too.

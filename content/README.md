# Content

A world is a folder of TOML files. `sample/` is Riverhold, the world the tests and scenarios use (DESIGN.md §13). [`docs/examples/riverhold`](../docs/examples/riverhold) shows every setting the design describes, including some the engine doesn't read yet.

## The files

Every file is optional. A missing file means the defaults, or none of that kind.

| File | What's in it |
| --- | --- |
| `balance.toml` | World-wide rules and defaults: alignment labels and distance, inertia profiles, disposition, standing spillover, relations, and the membership rules for drift, wars, defectors and deserters. |
| `factions.toml` | Each faction, keyed by its id: name, alignment, weights, tolerances, rank ladder, drift policy, and any rule tables of its own. |
| `characters.toml` | Everyone in the world, the player included, keyed by id: name, alignment, weights, inertia, starting memberships and standing. |
| `relations.toml` | How factions regard each other, as `[[relation]]` entries. |
| `actions.toml` | What characters can do: each act's effect on alignment and standing, and how its target changes it. |
| `outcomes.toml` | Named bundles of effects, such as a quest's result: alignment, standing, and shifts in how factions regard each other. |
| `quests.toml` | Quests, keyed by id: who gives each, what it needs to start, and its stages, each with requirements and choices. |
| `questlines.toml` | Questlines, keyed by id: who gives each, and its steps, each a group of quests done in any order. |

`sample/` has Riverhold's quests. They're checked with everything else, and once a world is loaded `quests` lists them and `start`, `choose` and `progress` play them; `quests <dir>` reads a directory's without loading it. A choice that can shut another quest off for good must say so in its `locks`, or loading refuses it (DESIGN.md §17.2). Riverhold's are in [`docs/examples/riverhold`](../docs/examples/riverhold).

Each file's comments explain its settings. [DESIGN.md §12](../docs/DESIGN.md) lists every setting, with its default and its checks.

## Checking a world

```bash
cargo run -q -p factional-cli -- validate content/sample
```

`validate` checks a folder exactly as `load` would, without starting a session:

- It lists every problem, each with its file and key path, such as `characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)`.
- It lists every warning, for things that are allowed but probably not meant, such as a faction no one starts close enough to join.
- It ends with a summary line, such as `content/sample loads: 6 characters, 5 factions, 6 actions, 7 relations and 4 outcomes`.

It exits with 0 if the world would load, warnings or not, and 1 if it wouldn't. A world with any problem never loads (DESIGN.md §12.2). Inside the REPL, `validate <dir>` does the same without replacing the loaded world.

## Help in your editor

There's a JSON Schema for each file in [`schema/`](../schema). An editor that understands it completes keys as you type, shows what each one means, and underlines a misspelt key, a number out of range or an unknown policy.

- **VS Code:** install the Even Better TOML extension (`tamasfe.even-better-toml`).
- **Point each file at its schema** with a first line such as `#:schema ../../schema/factions.schema.json`, relative to the file. The files in `sample/` already have one.
- **Or print a schema** with `cargo run -q -p factional-cli -- schema factions`. Without a file name, `schema` lists the files.

The schema checks one value at a time. Checks across values, such as whether a faction named in `characters.toml` exists, are `validate`'s.

## The editor

`cargo run -q -p factional-editor -- content/sample` opens a folder in the editor: every file and its entries on the left, with how many problems and warnings each has, and the entry you pick in the middle, with its TOML and everything the loader says about it. Each of the entry's values is in a field: change one and press Enter, and the problems update at once. Undo steps back, Save writes the files, keeping every comment and space as you wrote them, and "Read again" picks up edits made in your text editor, discarding unsaved ones. Adding and removing keys and entries comes next (DESIGN.md §18). In the REPL, `outline <dir>` prints the same view as text.

## Trying things out

- `cargo run -q -p factional-cli -- repl`, then `load content/sample` and `help`.
- `--explain` on `act`, `disposition`, `can-join` and `promote` shows the working.
- `act <actor> <action> --seen-by <id>,…` (or `--unseen`) says who saw an act. At first only those who learn of it firsthand change their minds; `act --explain` lists who learns and why.
- Riverhold's `knowledge.model` is `ripple`: news of an act some didn't see travels on along characters' `contacts` and through factions, ten ticks a hop, weaker each time. `news` shows what's on its way; `advance` delivers it.
- Everyone judges by what they know: `perceived <observer> <subject>` shows where someone pictures a character and why, and `--explain` on `distance`, `disposition` and `can-join` says when that isn't where they truly are. `outcome <outcome> <character> --seen-by <id>,…` (or `--unseen`) says who saw a quest's result.
- A faction with `secret_members = true` can be joined in secret: `join <character> <faction> --secretly`. Only the faction and its members know; everyone else judges by the memberships they know of, so a double agent can belong to two factions at war. `expose <character> <faction> --seen-by <id>,…` reveals it; a faction that finds out it's been deceived decides by its `exposed` table, and `--explain` shows how.
- `map <faction>` draws who stands where on the alignment plane and the region within the faction's tolerance; `matrix [<subject>...] [--csv]` gives how everyone regards each subject; `curve <knob>`, such as `curve disposition.affinity`, prints a curve as a table.
- `save <file>` keeps your session; `restore <file>` brings it back, as long as the content hasn't changed since.
- After editing a file, `reload` in the REPL replays your session on the new content and shows what changed.
- `cargo run -q -p factional-cli -- compare <scenario> --content A --against B` runs a scenario on two versions of a world and shows what came out differently. The scenario must load exactly one world; that load is what's swapped.
- [`scenarios/`](../scenarios) holds scripted sessions to replay with `factional run`.

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
| `outcomes.toml` | Named bundles of effects, such as a quest's result. |

Each file's comments explain its settings. [DESIGN.md §12](../docs/DESIGN.md) lists every setting, with its default and its checks.

## Checking a world

```bash
cargo run -q -p factional-cli -- validate content/sample
```

`validate` checks a folder exactly as `load` would, without starting a session:

- It lists every problem, each with its file and key path, such as `characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)`.
- It lists every warning, for things that are allowed but probably not meant, such as a faction no one starts close enough to join.
- It ends with a summary line, such as `content/sample loads: 6 characters, 5 factions, 6 actions, 7 relations and 3 outcomes`.

It exits with 0 if the world would load, warnings or not, and 1 if it wouldn't. A world with any problem never loads (DESIGN.md §12.2). Inside the REPL, `validate <dir>` does the same without replacing the loaded world.

## Help in your editor

There's a JSON Schema for each file in [`schema/`](../schema). An editor that understands it completes keys as you type, shows what each one means, and underlines a misspelt key, a number out of range or an unknown policy.

- **VS Code:** install the Even Better TOML extension (`tamasfe.even-better-toml`).
- **Point each file at its schema** with a first line such as `#:schema ../../schema/factions.schema.json`, relative to the file. The files in `sample/` already have one.
- **Or print a schema** with `cargo run -q -p factional-cli -- schema factions`. Without a file name, `schema` lists the files.

The schema checks one value at a time. Checks across values, such as whether a faction named in `characters.toml` exists, are `validate`'s.

## Trying things out

- `cargo run -q -p factional-cli -- repl`, then `load content/sample` and `help`.
- `--explain` on `act`, `disposition`, `can-join` and `promote` shows the working.
- [`scenarios/`](../scenarios) holds scripted sessions to replay with `factional run`.

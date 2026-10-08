# Riverhold: the complete example content

These files show every content file and setting the design describes (DESIGN.md §12, §13), filled in for Riverhold, the sample world. Every number matches the worked examples in DESIGN.md and PLAN.md.

**The engine reads all of this.** Each setting's comment names the increment that made the engine read it. `content/sample/` holds the same world with fewer comments, and a setting the design adds lands here first, named in a test until the engine reads it.

**It loads as a world, quests included** (Q5): `validate docs/examples/riverhold` checks everything, every lockout included, and `quests docs/examples/riverhold` in the REPL lists the quests.

| File | What's in it | Read from |
| --- | --- | --- |
| [balance.toml](balance.toml) | World-wide rules and defaults: labels, inertia, disposition, relations, spillover, membership rules, knowledge | A1, then each increment adds its section |
| [characters.toml](characters.toml) | Everyone in the world, the player included: alignment, weights, inertia, memberships, starting standing | A1 |
| [factions.toml](factions.toml) | Factions with their alignment, tolerances, drift policy, rank ladders and rule overrides | D1, M1 |
| [relations.toml](relations.toml) | How factions regard each other | M2 |
| [actions.toml](actions.toml) | What characters can do, and its effect on alignment and standing | A3 |
| [outcomes.toml](outcomes.toml) | Named bundles of effects, such as a quest's result, including relation shifts | M3, Q2 |
| [quests.toml](quests.toml) | Quests: the Watch's oath and odd jobs, the captain's dog, Ava's lost ring, the world's long winter and the Ashen Circle's rite, each with its stages and choices, and what each choice locks out | Q1, Q4 |
| [questlines.toml](questlines.toml) | The Watch's career, in four steps: the oath; two of three odd jobs; optional favours; and captain, gated by rank and standing | Q1 |

Two tests keep these files honest. `crates/content/tests/examples.rs` checks that every file is valid TOML. `crates/content/tests/schema.rs` checks that, without the settings the engine doesn't read yet, the files read and check cleanly, quests included, and match the JSON Schema; it lists those settings by name, so each one's arrival is noticed.

# Riverhold: the complete example content

These files show every content file and setting the design describes (DESIGN.md §12, §13), filled in for Riverhold, the sample world. Every number matches the worked examples in DESIGN.md and PLAN.md.

**The engine doesn't read all of this yet.** Each setting's comment names the increment that makes the engine read it. `content/sample/` holds what it reads today, and grows towards these files one increment at a time. Until then, loading this folder fails on the settings that aren't built yet.

**It has quests, so it doesn't load as a world yet.** A world with quests loads once loading can check that they reconcile (Q4, DESIGN.md §17.2). Until then `validate` checks them, and `quests docs/examples/riverhold` in the REPL lists them.

| File | What's in it | Read from |
| --- | --- | --- |
| [balance.toml](balance.toml) | World-wide rules and defaults: labels, inertia, disposition, relations, spillover, membership rules, knowledge | A1, then each increment adds its section |
| [characters.toml](characters.toml) | Everyone in the world, the player included: alignment, weights, inertia, memberships, starting standing | A1 |
| [factions.toml](factions.toml) | Factions with their alignment, tolerances, drift policy, rank ladders and rule overrides | D1, M1 |
| [relations.toml](relations.toml) | How factions regard each other | M2 |
| [actions.toml](actions.toml) | What characters can do, and its effect on alignment and standing | A3 |
| [outcomes.toml](outcomes.toml) | Named bundles of effects, such as a quest's result, including relation shifts | M3, Q2 |
| [quests.toml](quests.toml) | Quests: the Watch's oath and odd jobs, the captain's dog, Ava's lost ring and the world's long winter, each with its stages and choices | Q1 |
| [questlines.toml](questlines.toml) | The Watch's career, in four steps: the oath; two of three odd jobs; optional favours; and captain, gated by rank and standing | Q1 |

Two tests keep these files honest. `crates/content/tests/examples.rs` checks that every file is valid TOML. `crates/content/tests/schema.rs` checks that, without the settings the engine doesn't read yet, the files read and check cleanly, quests included, and match the JSON Schema; it lists those settings by name, so each one's arrival is noticed.

# Riverhold: the complete example content

These files show every content file and setting the design describes (DESIGN.md §12, §13), filled in for Riverhold, the sample world. Every number matches the worked examples in DESIGN.md and PLAN.md.

**The engine doesn't read all of this yet.** Each setting's comment names the increment that makes the engine read it. `content/sample/` holds what it reads today, and grows towards these files one increment at a time. Until then, loading this folder fails on the settings that aren't built yet.

| File | What's in it | Read from |
| --- | --- | --- |
| [balance.toml](balance.toml) | World-wide rules and defaults: labels, inertia, disposition, relations, spillover, membership rules, knowledge | A1, then each increment adds its section |
| [characters.toml](characters.toml) | Everyone in the world, the player included: alignment, weights, inertia, memberships, starting standing | A1 |
| [factions.toml](factions.toml) | Factions with their alignment, tolerances, drift policy, rank ladders and rule overrides | D1, M1 |
| [relations.toml](relations.toml) | How factions regard each other | M2 |
| [actions.toml](actions.toml) | What characters can do, and its effect on alignment and standing | A3 |
| [outcomes.toml](outcomes.toml) | Named bundles of effects, such as a quest's result | M3 |

A test (`crates/content/tests/examples.rs`) checks that every file here is valid TOML.

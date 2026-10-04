# Decisions

This file records every non-obvious choice, with the reason for it. [DESIGN.md](DESIGN.md) and [PLAN.md](PLAN.md) refer to entries by id.

## How to read this

| Status | Meaning |
| --- | --- |
| **Agreed** (D-n) | Settled with the user. Don't reopen it without asking. |
| **Proposed** (P-n) | A default chosen while designing. It stands unless the user objects, and becomes Agreed once they confirm it or an increment ships on it. |
| **Open** (O-n) | Needs the user. Any increment that depends on it is Blocked. |
| **Deferred** (X-n) | To be settled in the design pass named on the entry. |

If an increment forces a decision nobody has made yet, add it here as Proposed and mention it in the PR.

## Agreed — 2026-10-04

- **D-1 · Rust.** It's modern, and memory-safe without a garbage collector. It embeds in any host engine: Bevy natively, Godot/Unity/Unreal through bindings or a C ABI. Its compiler is also a strong guardrail for agent-written code.
- **D-2 · Two-axis sliding alignment.** One axis runs lawful↔chaotic, the other good↔evil. A character's own actions move them along both. The nine named alignments are only labels for regions of the plane.
- **D-3 · Characters and factions each have a view of a character.** It's friendly, neutral or unfriendly, driven by how close their alignments are. P-6 adds history and allegiance to that.
- **D-4 · Joining a faction needs a close enough alignment.**
  - A character can belong to several factions.
  - Factions have relations with each other.
  - Belonging to one faction can bar you from its enemies.
- **D-5 · Per-faction axis weights.** A faction can care about one axis more than the other when measuring distance.
- **D-6 · Configurable inertia.** Designers decide how much harder alignment is to move, depending on where it already is.
- **D-7 · Knowledge starts omniscient.** A `witnesses` field is reserved on actions. A ripple model comes later, where news of an act spreads to connected characters.
- **D-8 · Faction alignment is set by the designer, and other modules can change it at runtime.**
- **D-9 · Drift out of tolerance is configurable per faction.** That is, what happens when a member's alignment drifts too far from their faction's.
- **D-10 · Joining an enemy of a faction you're in depends on three factors:**
  - your rank in that faction;
  - your standing with it, which is separate from rank;
  - whether your alignment has drifted from one faction towards the other.
- **D-11 · Rank and standing are both tracked, per character per faction.**
- **D-12 · Easy to tune.** Designers must be able to tweak parameters easily to balance the game.
- **D-13 · A CLI for manual testing.**
- **D-14 · Small increments, test-driven.** Built with TDD; unit and integration tests keep the behaviour predictable.
- **D-15 · Agnostic of world and graphics.** Graphics, physics, combat, status, characteristics and quests plug in through APIs, in later iterations.
- **D-16 · War between two of your factions raises a conflict for the game to resolve** (was O-1).
  - When a relation change puts two of a character's factions in conflict, the engine emits `MembershipConflict`. It then waits for a `ResolveConflict` from the host or a quest.
  - An optional world-level rule can resolve it automatically instead: keep the higher rank, then the higher standing, then the longer service.
- **D-17 · Promotion happens only when another module asks** (was O-2). Meeting a rank's requirements makes promotion possible, never automatic. There's no per-faction automatic option.
- **D-18 · An action's alignment effect can depend on its target** (was O-3). Designers configure it per action, so killing a cultist can count as less evil than killing a priest. P-28 is the mechanism.
- **D-19 · Secret membership and double agents are wanted, as an option** (was O-4). They're designed in K0 alongside the knowledge model, because secrecy only means something once factions can be unaware of things.

## Proposed — 2026-10-04

- **P-1 · Fixed-point numbers.**
  - Two decimals, computed exactly, rounded once (half away from zero).
  - Floats only cross the content boundary, as decimal strings.
  - *Why:* identical results on every platform, exact expectations in tests, and the CLI shows exactly what the engine holds.
  - *Cost:* multipliers have only two decimals (0.33, not 0.333). That's fine for balance work.
- **P-2 · Each axis runs −100.00…+100.00.** Labels are for display only, with a threshold of 33.00.
  - *Why:* rules that branch on labels create cliffs, where one point changes everything. Continuous rules are easier to balance.
- **P-3 · Distance is weighted Euclidean by default.**
  - `alignment.metric` switches the whole world to Manhattan or Chebyshev.
  - Weights are 0.00–1.00, and at least one must be above 0.
  - *Why:* Euclidean tolerance regions are ellipses, which read naturally as "within N points". The metric is one cheap knob that changes their shape.
- **P-4 · Piecewise-linear curves for every non-trivial knob.**
  - *Why:* it's one shape for designers to learn. It's expressive enough for thresholds, falloffs and plateaus, and it prints as a table.
- **P-5 · How inertia works.**
  - Each axis has a multiplier curve per direction, read at the character's position before the act.
  - Curves are grouped into named profiles that characters opt into.
  - Multipliers are ≥ 0.
  - *Why:* one mechanism covers "hardening", "fall from grace" and "slow redemption". Reading the curve at the starting position keeps each shift a single, explainable multiplication.
- **P-6 · Disposition is a weighted sum of five components, clamped to ±100 and mapped to bands the designer defines** (default: unfriendly ≤ −25 < neutral ≤ 25 < friendly).
  - **Affinity:** how close the alignments are.
  - **Standing.**
  - **Kinship:** how the two sides' memberships relate.
  - **Faction opinion:** an NPC adopting their factions' view.
  - **Modifiers:** from other modules.
  - *Why:* with alignment alone, a completed quest could never improve how someone sees you. Separate weighted parts let designers tune each influence, and let `--explain` show which one mattered.
- **P-7 · Components that aggregate several factions sum them, then clamp the component to ±100.**
  - *Why:* averaging would let joining an unrelated faction dilute how much your enemies hate you.
- **P-8 · Standing and rank.**
  - Standing is −100…+100.
  - It's held for every (character, faction) and (character, character) pair with history, including non-members.
  - Rank exists only for members.
  - *Why:* you can earn a faction's goodwill before you join it.
- **P-9 · Ranks are a per-faction ladder.** Each rung may require a minimum standing, and may set a stricter alignment tolerance.
  - *Why:* rank and standing stay separate (D-11) but connected, and senior members can be held to a higher standard.
- **P-10 · Joining an enemy is settled by two ordered rule tables.**
  - They are the target's `defectors` table and the current faction's `deserters` table.
  - Each uses a small, fixed vocabulary of conditions (rank, standing with either faction, alignment drift), and the first matching rule wins.
  - The built-in defaults refuse every defector, matching D-4.
  - *Why:* it covers D-10's three factors and stays readable and explainable ("refused by rule 1: Officers don't walk away."). It isn't a scripting language.
- **P-11 · Drift policies.**
  - The policies are: ignore, flag, probation (grace ticks, then expel or demote), demote, expel.
  - `member_tolerance` must be ≥ `tolerance`, and a rank's tolerance overrides it.
  - *Why:* D-9's configurability, using a small closed set. The gap between the two tolerances stops membership flip-flopping at the boundary.
- **P-12 · Relations between factions.**
  - Relations are directional, −100…+100, and authored symmetrically by default.
  - Two factions are in conflict if either regards the other at or below −50.
  - *Why:* one-sided grudges are realistic, and "either side" is the cautious reading of "enemies".
- **P-13 · Standing spills over once, through relations, via a curve.** It never cascades.
  - *Why:* one hop is predictable and can't loop.
- **P-14 · Commands in, events out.**
  - `execute` returns events.
  - The core never calls into other modules; they influence it only by sending commands.
  - *Why:* it keeps the core pure, testable and replayable.
- **P-15 · Events carry absolute before and after values.**
  - *Why:* listeners and UIs can read them without context, and replaying them needs no rules.
- **P-16 · The journal of commands drives the what-if tools; the event log plus snapshots drives saves.**
  - *Why:* designers want to re-run history under new numbers, but players' saves must not change when the numbers do.
- **P-17 · The player is an ordinary character.**
  - *Why:* NPC-to-NPC relationships come free, and there's one code path to test.
- **P-18 · Content is TOML.**
  - It's validated strictly: unknown keys are errors, and each error names the file and key path.
  - A generated JSON Schema gives editor autocomplete.
  - *Why:* designers get feedback in their editor as they type, and TOML is friendlier to edit by hand than RON or JSON.
- **P-19 · No `HashMap` or `HashSet`.**
  - Use ordered collections everywhere, and iterate in id order wherever it reaches output.
  - *Why:* Rust's `HashMap` iterates in a different random order in every process, which would make the order of events nondeterministic.
- **P-20 · Time is abstract ticks that the host advances.**
  - *Why:* it keeps the engine world-agnostic (D-15), and tests control time exactly.
- **P-21 · Characters may declare their own axis weights; otherwise they use the global default.** They don't inherit weights from factions.
  - *Why:* with several memberships, inheritance would be ambiguous.
- **P-22 · Leaving and expulsion costs.**
  - Leaving voluntarily costs the faction's `leave_standing_change` (default 0).
  - Expulsion costs `expel_standing_change` (default −20).
  - *Why:* designers can make some factions hard to walk away from.
- **P-23 · Band-change events are only for watched subjects, with an optional hysteresis margin.**
  - *Why:* recomputing every pair after every command doesn't scale, and nobody needs it. Hysteresis stops a guard flickering between neutral and unfriendly.
- **P-24 · Every evaluation returns its working, and every number it shows is the number used.**
  - *Why:* designers balance by understanding, and predictability means being able to see why.
- **P-25 · Four crates: `factional-core`, `factional-reputation`, `factional-content`, `factional-cli`, with dependencies pointing one way.**
  - *Why:* I/O lives only in content and cli. Future modules depend on core, not on each other's internals.
- **P-26 · Outcomes and effects.**
  - Outcomes are named bundles of effects in content, applied with `ApplyOutcome`.
  - `ApplyEffects` takes effects directly.
  - *Why:* quests can be exercised from the CLI long before a quest module exists, and that module will later use the same entry point.
- **P-27 · Scenario scripts (`*.scenario`) are both the manual-testing tool and a regression suite.** `assert` lines state the intent; snapshots catch knock-on changes.
  - *Why:* every manual test becomes an automated test for free.
- **P-28 · Target-aware scaling for actions** (the mechanism for D-18).
  - **By the target's alignment:** for each axis, a curve over the target's position on that same axis gives a multiplier for the action's delta on that axis. Harming the good is more evil than harming the wicked.
  - **By faction hostility:** optionally, one curve over the most hostile relation between the actor's factions and the target's factions gives a multiplier for both axes. An act against a faction you're at war with weighs less.
  - **Never a reversal:** multipliers are ≥ 0, so a target can soften or sharpen an act but never turn it good. An act that should be good against some targets, such as slaying a demon, is a separate action that the host chooses.
  - *Why:* same-axis curves read naturally, and they're the curve shape designers already use. Non-negative multipliers keep a shift's direction fixed by the action, so inertia and invariant 8 stay simple.
- **P-29 · How scenario scripts behave** (made in F0, 2026-10-04).
  - **Script mistakes stop the run** with `line N: …`: an unknown command, an `assert` without ` == `, or a false assertion.
  - **A command that runs and fails** renders as `error: …`. Inside an `assert`, that text is checked like any other result; outside one, it stops the run.
  - **A command's own bad arguments count as the command failing**, not as a script mistake. For example, `calc 1 / 0` gives `error: cannot divide by 0.00`, so scenarios can assert on argument errors. Added in F1.
  - **Transcripts** list each command and its output, but not comments or blank lines.
  - **Snapshots** live in `crates/cli/tests/snapshots/`, not beside the scenarios, because `cargo insta review` only finds snapshots inside a package.
  - *Why:* scripts can test failure paths as easily as successes, while a typo in a script can never pass silently.

- **P-30 · `Fixed`'s range and strictness** (made in F1, 2026-10-04).
  - **Range.** A `Fixed` holds about ±92 trillion. The `+ - *` operators panic beyond that range in every build, debug and release alike. The `checked_*` methods return `None` instead, and that's what the CLI's `calc` uses.
  - **Division** is `checked_div` only, with no `/` operator, because dividing by zero has to be handled where it can happen.
  - **Parsing is strict.** It accepts digits, an optional sign and at most two decimal places. `.5`, `5.`, exponents and spaces are not numbers.
  - *Why:* a number that silently wrapped around would be a wrong answer that looks right. Content validation keeps rule values far inside the range, so the panic marks a bug, never a designer's input.
- **P-31 · Content-loading conventions** (made in A1, 2026-10-04).
  - **Missing files are allowed.** A missing `balance.toml` means every default; a missing `characters.toml` means no characters. `load` says how many characters it read, so an empty or wrong directory is obvious.
  - **Ids** are a lowercase letter, then lowercase letters, digits or `_` (`captain_hale`), because they're typed in the CLI.
  - **Every field is explicit.** A character needs a `name`, and an alignment needs both axes. Defaults live only in `balance.toml`'s knobs.
  - **All problems at once.** Loading reports every problem in every file, in file order and then key order, each as `file: key.path: message`. Unknown keys suggest the closest known key.
  - **A failed `load` changes nothing.** The CLI keeps the world it already had.
  - **CLI paths** are relative to the current directory. The scenario harness runs from the repository root, so scenarios write `load content/sample`.
  - *Why:* designers fix content in one pass, and nothing half-loaded ever runs.
- **P-32 · Content is checked completely before a world exists** (2026-10-04, at the user's request).
  - **Load time is content's compile time.** Every reference (a membership's faction, a rank, an inertia profile, a standing's faction or character) must resolve, and every value must be in range, or the world isn't built. The checks are listed in DESIGN.md §12.2.
  - **The rules live in `factional-reputation`.** `World::new` refuses invalid content, so content built in code gets the same checks as content from TOML. `factional-content` only maps problems to file and key paths. This arrives with M1, the first cross-file reference.
  - **Each increment adds the checks for the content it introduces**, in the same PR, with tests. They aren't saved up for T1, which keeps only the whole-world warnings and the `validate` command.
  - **Errors stop loading; warnings are printed but don't.** A rule table must end with a rule that always decides. Spillover multipliers stay within −1…1.
  - *Why:* a world that starts can't hit a dangling reference mid-game, and designers learn about every mistake when they load, not when a player finds it.

## Open

None right now. A new question gets the next free number, starting at O-5.

## Deferred

- **X-1 · Perceived alignment.** Whether observers judge a character by what they know of them rather than by their true alignment, and how. Settled in K0.
- **X-2 · Host engine and integration route.** A Bevy plugin, or a C ABI for Godot, Unity or Unreal. Settled in E0.
- **X-3 · Save format and versioning.** Settled in T4.

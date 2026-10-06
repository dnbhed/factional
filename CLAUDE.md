# Factional — agent guide

Factional is a modular single-player RPG engine written in Rust. Its first module is reputation & factions (`crates/reputation`). Graphics, physics, combat, quests and the rest come later, as separate crates that talk to it through commands, events and queries.

Before starting any increment, read:

1. **`docs/PLAN.md`:** the queue. Take the first item marked **Next**.
2. **`docs/DESIGN.md`:** the sections the increment touches. The model, the formulas and the sample world (Riverhold) are there.
3. **`docs/DECISIONS.md`:** the decisions, in four kinds:
   - **D (agreed):** don't re-decide these.
   - **P (proposed):** follow these.
   - **O (open):** if the increment depends on one, stop and ask the user.
   - **X (deferred):** settled in a later design pass.

## Increment workflow

1. **Scope.** If anything in the increment is ambiguous, confirm the scope with the user; otherwise proceed.
2. **Branch.** Branch from `main` as `inc/<id>-<slug>` (for example `inc/f1-fixed-point`). Mark the increment **In progress** in PLAN.md.
3. **Red.** Before writing any code, turn every acceptance example into a test. Run the tests and check that each fails for the expected reason: an assertion about the missing behaviour, not an unrelated compile error. Say so in the PR.
4. **Green, then refactor.** Write the least code that passes, then tidy it with the tests still green.
5. **CLI.** Add the increment's CLI commands. Add at least one `scenarios/*.scenario` that exercises them, with `assert` lines for the behaviour under test.
6. **Gates.** All must pass locally before you push:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   git diff main... > target/branch.diff && cargo mutants --in-diff target/branch.diff
   ```

   A surviving mutant means an assertion is missing. Add the test, or explain in the PR why the mutant is equivalent.
7. **Docs, in the same PR:**
   - Compress the increment to one line under **Done** in PLAN.md: id, what shipped, date, PR.
   - Update DESIGN.md if the model, a formula or the tuning table changed.
   - Add to DECISIONS.md any decision you had to make, as Proposed.
   - Extend `content/sample/` and the JSON Schema for any new content. The schema is built in `crates/content/src/schema.rs`; regenerate `schema/<file>.schema.json` with `cargo run -q -p factional-cli -- schema <file>`. Its tests say what's missing.
8. **Commit and PR.** Commit as `Increment <id>: <what it does>`, push, and open a PR against `main`. **Never merge without the user's explicit go-ahead**, even when CI is green.

Keep increments small: one concept, reviewable in one sitting. If an increment grows, split it in PLAN.md rather than shipping it big.

## Architecture rules

- **Fundamental: a world loads only if it is complete (D-20).** Every reference must resolve, every rule must be able to decide, and nothing a game could reach may be left undefined. Never add content, a rule or a feature whose completeness can't be checked at load time. Keep effects and rules declarative, so their reach can be computed without running the game (DESIGN.md §16). If you can't see how a change could be checked, stop and ask. In the future quest module, each faction's quests and questlines must reconcile with every other faction they affect, and with those factions' questlines, at every stage.
- **Dependency direction.** Dependencies point one way: `core ← reputation ← content ← cli`. Nothing depends on `cli`.
- **No I/O in the core.** `factional-core` and `factional-reputation` do no filesystem access, networking, stdout/stderr, environment variables, clock reads or randomness. Content arrives as values; time arrives as `AdvanceTime`.
- **One way in.** `World::execute(Command) -> Result<Vec<Event>, CommandError>` is the only way to change state.
  - A failed command changes nothing and emits nothing.
  - Applying events is the only thing that mutates state.
- **Runtime changes go through commands.** Anything gameplay can change after load goes through a command, so it shows up as an event. That covers faction alignment, relations, standings and ranks.
- **Module boundaries.** Other modules integrate only through commands, events and queries. The core never calls back into host code.
- **The player is an ordinary character.** No `is_player` branches in rules.
- **Explanations come from the engine.** Queries return their working: breakdowns, and the rule that fired. The CLI renders those explanations; it never recomputes them.

## Determinism rules

- **No floats in rule code.** `f32` and `f64` are banned there; `#![deny(clippy::float_arithmetic)]` is set in core and reputation. Use `Fixed`. Its rounding (half away from zero, once per computed value) is the only rounding.
- **No hash collections.** `HashMap` and `HashSet` are banned everywhere, enforced by `clippy.toml`. Use `BTreeMap`, `BTreeSet` or a sorted `Vec`. Anything that reaches events or output iterates in id order.
- **Same inputs, same events.** The same content and the same commands give identical events. A property test guards this (DESIGN.md §14, invariant 3); never weaken it.

## Testing

- **Unit tests** sit beside the code, for value types and individual rules.
- **Integration tests** live in `crates/<crate>/tests/` and drive the public API against `content/sample` (DESIGN.md §13).
- **Property tests** (`proptest`) cover every invariant in DESIGN.md §14. A new invariant means a new property test.
- **Scenario tests:** every `scenarios/*.scenario` runs through the CLI and is snapshotted with `insta`.
  - Use `assert` lines for what the scenario is about; the snapshot catches knock-on changes.
  - Read every snapshot diff before accepting it. An unexplained change is a bug until it's explained.
  - Never bulk-accept snapshots.
- **Expected values** come from DESIGN.md's worked examples, or are worked out by hand from its formulas. Never paste them from the code's own output.
- **Test names state the rule:** `steal_moves_alignment_toward_chaotic_evil`, not `test_steal`.

## Designer-facing rules

- **No balance numbers in code.** A balance number is never a literal in rule code. It's content, with its default defined once and listed in DESIGN.md §12.
- **New knobs land complete.** A new knob arrives with:
  - its schema entry;
  - a value in `content/sample`;
  - a row in DESIGN.md §12;
  - its line in `docs/examples/riverhold`, matching what the engine now reads;
  - its effect visible in the relevant `--explain` output.
- **Every reference is checked at load time.** If an increment adds a field that names something else, such as a faction, rank, profile or character, it adds the check that the thing exists, plus range checks for its numbers, in the same PR, with tests and a row in DESIGN.md §12.2. A world is never built from content that fails a check (P-32).
- **Content errors speak a designer's language.** They name the file and the key path, say what's wrong in plain words, and offer a "did you mean" for misspelt ids. Unknown keys are errors.

## Fix broken windows

If you find a defect while doing something else, fix it in this increment. Say so in the PR, and add a test that would have caught it.

Defer it only with a written reason:

- it needs the user's decision;
- it belongs entirely to another crate's work;
- it's genuinely large.

A deferral gets an id in PLAN.md, not just a note.

## Plan hygiene

- **Detail only where it's needed.** PLAN.md keeps full detail only for the next few increments that aren't done. A done increment becomes one line: id, what shipped, date, PR.
- **Don't write ahead of the queue.** Write detailed acceptance examples for an outline item only when it becomes **Next**.
- **Decisions live in DECISIONS.md.** Decisions and their reasons go there, not in PLAN.md.

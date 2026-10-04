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
| **Outline** | Detail and acceptance examples get written when the increment nears the front of the queue. |

## Order

The order below is the source of truth. Sections further down are grouped by phase for easy scanning, not in delivery order.

`F0 → F1 → F2 → A1 → A2 → A3 → D1 → D2 → M1 → M2 → M3 → M4 → M5 → M7 → A4 → A5 → D3 → M6 → M8 → M9 → M10 → T1 → T2 → T3 → T4 → K0 → K1 → K2 → K3 → K4 → E0 → E1`

## Done

_Nothing yet._

---

## Phase 0 — Foundations

### F0 · Workspace, CI and CLI shell — P0 · Next

**Why:** every later increment needs the crates, the gates, and a place to run scenarios.

**Scope**

- **Workspace.** A Cargo workspace on edition 2024 with resolver 3. `rust-toolchain.toml` pins the current stable release, with rustfmt and clippy.
- **Crates.** Four, per DESIGN.md §15: `factional-core`, `factional-reputation`, `factional-content`, and `factional-cli` with a binary named `factional`. Dependencies point one way.
- **Lints.**
  - `unsafe_code = "forbid"` across the workspace.
  - Clippy warnings are errors.
  - A root `clippy.toml` disallows `HashMap` and `HashSet`, each with a reason (P-19).
  - `#![deny(clippy::float_arithmetic)]` in core and reputation.
- **CI.** GitHub Actions, Linux only. On every push and PR it runs fmt, clippy and test, with dependency caching. A PR-only job runs `cargo mutants --in-diff`. This needs the GitHub remote.
- **CLI.**
  - `factional --version`.
  - `factional repl`: a line editor with `help` and `quit`; on an error it prints it and carries on.
  - `factional run <file>`: runs a scenario script and stops at the first failing line.
- **Scenario script format.**
  - One command per line; blank lines and `#` comments are ignored.
  - `assert <command> == <expected>` passes only if the command's rendered result is exactly `<expected>`. The result counts whether the command succeeded or failed, so `error: …` and `refused: …` can be asserted too.
  - A command that errors outside an `assert` stops the run.
  - `echo <text>` and `fail <message>` exist to test the harness itself.
- **Scenario harness.** A test in the cli crate runs every `scenarios/*.scenario` file and snapshots its transcript with `insta`.
- **Housekeeping.** `.gitignore`, a README quickstart, and install notes for the dev tools (`cargo install cargo-insta cargo-mutants`).

**Acceptance**

1. On a clean checkout, the gate commands in CLAUDE.md pass, and CI runs them on a PR.
2. `factional --version` prints `factional 0.1.0`.
3. `scenarios/smoke.scenario` contains `echo hello` and `assert echo hi == hi`. It passes, and its snapshot shows each command with its output.
4. A script whose line 3 is `frobnicate` stops there with `line 3: unknown command 'frobnicate'`. The exit code is non-zero, and the lines after it don't run.
5. A script whose line 2 is `assert echo hi == bye` fails with `line 2: expected 'bye', got 'hi'`.
6. `assert fail boom == error: boom` passes. A bare `fail boom` on line N stops the run with `line N: error: boom`.
7. Adding a `HashMap` to `factional-core` fails clippy. Check this once by hand and note it in the PR; don't commit it.

### F1 · Fixed-point numbers — P0

**Why:** every rule computes in `Fixed` (DESIGN.md §4.1, P-1).

**Scope**

- **Type.** `Fixed` in core, stored as hundredths in an `i64`, with wider intermediates.
- **Parsing and display.** Parse from a decimal string; display with exactly two decimals.
- **Arithmetic.** Add and subtract exactly. Multiply and divide with rounding half away from zero. Clamp.
- **TOML.** Serde deserialises a TOML integer or float through its decimal string, with no float arithmetic.

**Acceptance**

1. Parsing and display:
   - `"12.5"` → `12.50`
   - `"7"` → `7.00`
   - `"-0.05"` → `-0.05`
   - `"-0.00"` displays as `0.00`
2. Bad input:
   - `"12.345"` → error `12.345 has more than 2 decimal places`
   - `""`, `"1.2.3"` and `"abc"` each give an error that names the input
3. Multiplication:
   - 0.05 × 0.50 = 0.03
   - −0.05 × 0.50 = −0.03
   - 4.00 × 0.41 = 1.64
4. Division:
   - 1.00 ÷ 3.00 = 0.33
   - 2.00 ÷ 3.00 = 0.67
   - −2.00 ÷ 3.00 = −0.67
   - Dividing by 0.00 is an error.
5. (95.00 + 10.00) clamped to −100.00…100.00 is 100.00.
6. TOML:
   - `x = 0.3` deserialises to 0.30.
   - `x = 30` deserialises to 30.00.
   - `x = 0.333` is an error.
7. Properties:
   - `parse(display(x)) = x`
   - a × b = b × a
   - The result of × and ÷ is within 0.005 of the exact rational result.
   - Clamping always lands within bounds.

### F2 · Curves — P0

**Why:** curves are the shared shape for tuning knobs (DESIGN.md §4.2, P-4).

**Scope**

- **Type.** `Curve` in core: either a constant, or two or more `[x, y]` points with strictly increasing x.
- **Evaluation.** Per §4.2: end values hold, linear between points, rounded once.
- **Validation and serde.** From TOML, a curve is either a number or an array of pairs.
- **Bounds helper.** Checks that every y is within given bounds. Later increments use it, for example to require inertia multipliers ≥ 0.

**Acceptance**

1. Curve `[[0, 1.00], [50, 0.70], [100, 0.30]]`:
   - f(25) = 0.85
   - f(75) = 0.50
   - f(33) = 0.80
   - f(50) = 0.70
   - f(−10) = 1.00
   - f(120) = 0.30
2. Curve `[[0, 50], [60, 0], [200, -50]]`:
   - f(70.18) = −3.64
   - f(85.48) = −9.10
   - f(5.59) = 45.34
3. `1.0` is a constant curve: f(anything) = 1.00.
4. Invalid curves:
   - `[[50, 1.0], [40, 0.0]]` → error `curve points must have increasing x: 50.00 then 40.00`
   - `[[0, 1.0]]` → error `a curve is a single number or at least 2 points`
5. Properties:
   - f(x) always lies between the curve's smallest and largest y.
   - If the points' y values never decrease, f never decreases.

## Phase 1 — Characters and alignment

### A1 · Characters, alignment and content loading — P0

**Why:** this is the first domain type and the first designer-facing content (DESIGN.md §5.1, §12, §13).

**Scope**

- **Alignment.** `Alignment { law, good }`.
  - Out-of-range values are content errors, not clamped.
  - It has a nine-box label, using `alignment.label_threshold` (default 33.00).
- **Content loading.** `factional-content` loads `balance.toml` and `characters.toml` from a directory.
  - Every diagnostic carries the file and the key path.
  - Unknown keys are errors, with a "did you mean".
- **World.** `World::new(content)`, with the queries `alignment(id)` and `characters()`.
- **Sample content.** `content/sample/` with Riverhold's characters, alignment only for now.
- **CLI.** `load <dir>`, `characters`, `show character <id>`.

**Acceptance**

1. Labels:
   - `player` (0 / 0): True Neutral
   - `captain_hale` (75 / 30): Lawful Neutral
   - `sister_mira` (35 / 85): Lawful Good
   - `vex` (−55 / −20): Chaotic Neutral
   - `brother_ash` (25 / −70): Neutral Evil
2. The threshold is inclusive:
   - 33.00 / −33.00 → Lawful Evil
   - 32.99 / 0.00 → True Neutral
3. `law = 120` in characters.toml → `characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00`.
4. A misspelt `alignmnet = {…}` → `characters.toml: vex: unknown key 'alignmnet' (did you mean 'alignment'?)`.
5. `show character vex` shows the id, name, law, good and label.

### A2 · World shell: commands, events, time, journal — P0

**Why:** this is the functional core that every later command plugs into (DESIGN.md §2, §11.5; P-14, P-15, P-16, P-20).

**Scope**

- **Types.** `Command`; `Event`, an envelope with a sequence number, the tick and a payload; `CommandError`.
- **World.** `World::execute` and `World::apply`.
- **Time.** `AdvanceTime { ticks }` emits `TimeAdvanced { from, to }`.
- **Journal.** Records every issued command and whether it was accepted.
- **CLI.** `advance <n>`, `time`, `events [--since <seq>]`, `journal`.

**Acceptance**

1. A new world is at tick 0 with no events.
2. `advance 5` emits one event: seq 1, `TimeAdvanced 0 → 5`. `time` then prints 5.
3. `advance 0` gives the error `ticks must be at least 1`. No event is emitted, the state is unchanged, and the journal records the command as rejected.
4. Properties, over random sequences of `AdvanceTime`:
   - Replaying the events from the initial state reproduces the state.
   - Re-executing the journal on a fresh world reproduces the events exactly.
   - A rejected command leaves the state exactly as it was.

### A3 · Actions move alignment — P0

**Why:** this is the brief's core mechanic: what you do determines who you are (DESIGN.md §5.2).

**Scope**

- **Content.** `actions.toml`, with alignment deltas only; standing effects arrive in M3.
- **Command.** `PerformAction { actor, action, target?, scale = 1.00, witnesses = everyone }`.
  - It emits `ActionPerformed`.
  - It then emits `AlignmentChanged { from, to }` if the alignment moved.
- **Inertia.** 1.00 everywhere until A4.
- **CLI.** `actions`, `act <actor> <action> [--target <id>] [--scale <n>]`.

**Acceptance** (Riverhold, with the player starting at 0 / 0)

1. `act player steal --target merchant_ava` moves the player to −5.00 / −3.00. Events: `ActionPerformed`, then `AlignmentChanged 0.00/0.00 → -5.00/-3.00`.
2. With `--scale 2`, the player moves to −10.00 / −6.00.
3. From −98.00 / 0.00, stealing gives −100.00 / −3.00 (clamped).
4. From −100.00 / −100.00, stealing emits `ActionPerformed` only, with no `AlignmentChanged`.
5. `act player stael` → `unknown action 'stael' (did you mean 'steal'?)`, and nothing changes.
6. Other errors:
   - `--scale 0` → `scale must be greater than 0.00`
   - `--target player` → `an action's target must be another character`
   - An unknown actor or target gives an error that names it.

## Phase 2 — Perception

### D1 · Weights and distance — P0 · Outline

**Covers:** DESIGN.md §6.

**Scope**

- Factions arrive here (id, name, alignment, weights), without membership.
- Global `alignment.default_weights` and `alignment.metric`.
- Query `distance(observer, subject)`; CLI `distance <observer> <subject>`.

**Anchors**

- `city_watch` → player at 0 / 0: 70.18 (Manhattan 75.00, Chebyshev 70.00)
- `lantern_guild` → player at −10 / −6: 50.04; at −20 / −12: 40.01
- `city_watch` → vex at 35 / 10: 35.09
- `lantern_guild` → vex at 35 / 10: 95.52
- `temple` → `sister_mira`: 5.59

### D2 · Disposition from affinity — P0 · Outline

**Covers:** DESIGN.md §8.1, the affinity component only.

**Scope:** bands, and `--explain`. Query `disposition(observer, subject)` returns the score, the band and the breakdown.

**Anchors**

- `city_watch` → player: −3.64, neutral
- `temple` → `sister_mira`: 45.34, friendly
- `temple` → `brother_ash`: −32.15, unfriendly
- `city_watch` → vex: −23.36, neutral

### D3 · Watched subjects and band-change events — P1 · Outline

**Covers:** DESIGN.md §8.3.

**Scope:** `Watch` and `Unwatch`, `DispositionBandChanged`, and `disposition.hysteresis`.

**Anchors:** to be written when D3 comes up next. They must cover:

- a band crossing, and the event it emits;
- no event when the score moves within a band;
- hysteresis holding a band near its edge.

## Phase 3 — Factions, standing and rank

### M1 · Factions: tolerance, joining and leaving — P0 · Outline

**Covers:** DESIGN.md §9.1.

**Scope**

- `tolerance`, `member_tolerance`, and starting memberships.
- `JoinFaction` and `LeaveFaction`; `assess_join` with reasons.
- A validation warning for starting members outside their member tolerance.
- CLI: `join`, `leave`, `can-join [--explain]`, `show faction`.

**Anchors** (the player and `lantern_guild`)

- At 0 / 0, refused: 60.21 > 45.00.
- After two thefts, refused: 50.04.
- After four thefts, the player joins.

### M2 · Relations and enemy exclusion — P0 · Outline

**Covers:** DESIGN.md §9.4.

**Scope**

- `relations.toml`, with symmetric `between` entries and directional `from`/`to` entries.
- Relation bands and the conflict threshold.
- `SetRelation` and `ShiftRelation`.
- A join is refused when the joiner belongs to a faction in conflict with the target. That stays the rule until M7 replaces it with rule tables.
- CLI: `relations`, `relate`.

**Anchors**

- `city_watch` and `lantern_guild` are in conflict.
- `city_watch` → `free_company` is −30 (rival) and `free_company` → `city_watch` is −10 (neutral), so they're not in conflict.
- The player, in the guild at −20 / −12, applies to the Watch. The refusal gives both reasons: distance (90.35 > 40.00) and enemy membership.

### M3 · Standing, action effects and outcomes — P0 · Outline

**Covers:** DESIGN.md §7.1, without spillover.

**Scope**

- The standing store, and action `standing` effects.
- `outcomes.toml`; `ApplyOutcome` and `ApplyEffects`; `StandingChanged`.
- The `awareness()` seam, which returns 1.00 under the omniscient model.
- CLI: `standing`, `outcome`.

**Anchors**

- Stealing from `merchant_ava` puts `merchant_ava`'s standing toward the player at −20.00.
- Stealing from vex: vex −20.00 and `lantern_guild` −10.00.
- `outcome fined_by_watch player`: `city_watch` −20.00 and `captain_hale` −10.00.
- Standing clamps at −100.00.

### M4 · Disposition from standing, kinship and faction opinion — P0 · Outline

**Covers:** DESIGN.md §8.1 in full, and §8.2.

**Anchors**

- §8.2's worked example: `captain_hale` → player is −29.10, unfriendly, and the explain table adds up.
- `city_watch` → vex is −63.36, unfriendly: affinity −23.36 plus kinship −80 × 0.50.

### M5 · Rank ladders and promotion — P0 · Outline

**Covers:** DESIGN.md §7.2.

**Scope**

- Rank content; new members join on the lowest rung.
- `Promote` and `Demote` with requirement checks; `RankChanged`.
- Promotion only ever happens on request. Meeting the requirements never promotes anyone automatically (D-17).
- CLI: `ranks`, `promote`, `demote`.

**Anchors**

- vex (fence, standing 30) → promote is refused: needs 60.00, has 30.00. With 30 more standing, vex becomes a shadow.
- `captain_hale` is already at the highest rank.
- `sister_mira` → high_priest is refused on standing (40.00 < 80.00), even though `sister_mira` is within the rank's tolerance (5.59 ≤ 20.00).

### M6 · Standing spillover between factions — P1 · Outline

**Covers:** the spillover part of DESIGN.md §7.1: one hop, using the `standing.spillover` curve.

**Anchors**

- Robbing vex (guild −10.00) → `city_watch` +1.80.
- `donate_to_temple` (temple +10.00) → `city_watch` +1.00 and `ashen_circle` −2.40.
- `lantern_guild` ↔ `free_company` (+20) spills nothing under the default curve.

### M7 · Joining an enemy: defectors and deserters — P0 · Outline

**Covers:** DESIGN.md §9.2.

**Scope**

- The rule tables: world defaults plus per-faction overrides.
- `assess_join` reports which rule fired.
- The defection events.
- `outside_member_tolerance` uses the same member-tolerance rule as §9.3, including rank tolerance.

**Anchors**

- The built-in defaults reproduce M2's refusal.
- With the sample tables, vex at 35 / 10 defects from the guild to the Watch: released on drift, then accepted on `closer_to_target` with −10.00 standing toward the Watch.
- A rank-3 shadow is refused by the `deserters` table.

### M8 · Drift policies and runtime faction alignment — P1 · Outline

**Covers:** DESIGN.md §9.3.

**Scope**

- The drift policies, probation with `grace_ticks`, rank tolerance overrides, and `expel_standing_change`.
- `SetFactionAlignment` and `ShiftFactionAlignment`, which emit `FactionAlignmentChanged` and trigger a membership review.

**Anchors:** to be written when M8 comes up next. They must cover:

- every policy;
- probation that clears, and probation that expires;
- a faction alignment change that pushes a member out.

### M9 · When two of your factions go to war — P1 · Outline

**Covers:** DESIGN.md §9.4, D-16.

**Scope**

- A relation change that puts two of a character's factions in conflict emits `MembershipConflict` and marks both memberships as conflicted.
- `ResolveConflict { character, keep }` ends the other membership with `LeftFaction { reason: ConflictResolved }`.
- `membership.conflict` sets the optional automatic rule, applied straight away or after `auto_after_ticks`. The rule keeps the higher rank, then the higher standing, then the longer service, then the lower faction id.
- Settle the standing consequences of leaving, and record the choice in DECISIONS.md.
- CLI: `resolve <character> <faction-to-keep>`.

**Anchors:** to be written when M9 comes up next. They must cover:

- a war declared between two of a character's factions;
- resolving it by hand;
- each tie-break of the automatic rule;
- `auto_after_ticks` running out, versus a resolution that arrives first.

### M10 · Disposition modifiers from other modules — P1 · Outline

**Scope**

- `AddModifier { id, observer: everyone | faction | character, subject, amount, expires_at? }` and `RemoveModifier`.
- Modifiers expire on `AdvanceTime`.
- The modifiers component of disposition.

### A4 · Inertia profiles — P1 · Outline

**Covers:** DESIGN.md §5.3.

**Scope**

- Inertia profiles, and a per-character `inertia` setting.
- Validation that every multiplier is ≥ 0.
- `act --explain`.

**Anchors**

- A hardening character at good 60.00:
  - `help_stranger` → +2.32, ending at 62.32
  - `extort` → −4.20, ending at 55.80
- `sister_mira` at 85.00 helps a stranger: ×0.41 → +1.64, ending at 86.64.
- A steady character gets the full shift.

### A5 · Target-aware action effects — P1 · Outline

**Covers:** DESIGN.md §5.4; D-18, P-28.

**Scope**

- Optional `by_target.<axis>` and `by_target.relation` curves on actions, with every multiplier validated ≥ 0.
- The target multiplier joins the shift formula (§5.2), which still rounds once.
- `act --explain` shows each multiplier and where it came from.

**Anchors** (`murder`, as in DESIGN.md §13)

- The player murders brother_ash: good × 0.44 → −6.60; law −10.00, unscaled.
- The player murders sister_mira: good × 1.43 → −21.45.
- captain_hale murders vex: good −6.64 (target 0.84, relation 0.62, inertia 0.85); law −6.20 (relation 0.62).
- An action without `by_target` curves, or performed without a target, shifts exactly as before.

## Phase 4 — Designer tooling and persistence

### T1 · Validation sweep and JSON Schema — P1 · Outline

- `factional validate <dir>` reports every error and warning at once:
  - cross-file references;
  - warnings for ranks no one can reach and factions no one can join.
- `factional schema` produces the JSON Schema for editor autocomplete.
- A content README for designers.

### T2 · What-if: compare and reload — P1 · Outline

- `factional compare <scenario> --content A --against B` shows:
  - a diff of the final state: alignments, standings, memberships, ranks, and dispositions toward watched subjects;
  - the first event where the two runs diverge.
- In the REPL, `reload` re-reads the content, replays the journal, and reports what changed.

### T3 · Map, matrix and curves — P2 · Outline

- `map <faction>` draws the alignment plane as ASCII, showing the faction's tolerance region and the characters.
- `matrix [--csv]` shows every observer's disposition toward the chosen subjects.
- `curve <knob>` prints a curve as a table.

### T4 · Saves — P2 · Outline

- Serialises a snapshot plus the event log, with a format version.
- Loading gives identical query answers; a property test checks it.
- Format details are settled here (X-3).

## Phase 5 — Knowledge and rumour

### K0 · Design pass: witnesses, ripple, perceived alignment — P2

A design section and its decisions, agreed before K1 starts. It settles X-1, and designs the double agents that D-19 asks for.

### K1 · Witnessed acts — P2 · Outline

### K2 · Ripple through the social graph — P2 · Outline

### K3 · Perceived alignment — P2 · Outline

Depends on K0's answer.

### K4 · Secret membership and double agents — P2 · Outline

An opt-in option to belong to a faction without its enemies knowing. Designed in K0 (D-19).

## Phase 6 — Embedding

### E0 · Choose a host engine and integration route — P3

Settles X-2.

### E1 · Host adapter — P3

---

## Adding an increment

Use the same shape as above:

1. The id, title, priority and status.
2. **Why.**
3. **Scope.**
4. **Acceptance** examples. Work their numbers out by hand from DESIGN.md's formulas; never take them from the code's output.
5. The CLI commands it ships.
6. Any knobs it adds. These go in DESIGN.md §12 too.

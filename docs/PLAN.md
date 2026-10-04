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

`F0 → F1 → F2 → A1 → A2 → A3 → D1 → D2 → M1 → M2 → M3 → M4 → M5 → M7 → A4 → A5 → D3 → M6 → M8 → M9 → M10 → T1 → T2 → T3 → T4 → K0 → K1 → K2 → K3 → K4 → E0 → E1 → Q0`

## Done

- F0 — workspace of four crates with lints and a pinned toolchain, CI with mutation testing on PRs, the `factional` CLI (`repl`, `run`), and the scenario harness; done 2026-10-04 (#1)
- F1 — `Fixed`, the two-decimal number every rule uses: exact parsing and display, arithmetic rounded half away from zero, checked overflow, TOML input; plus the `calc` CLI command; done 2026-10-04 (#2)
- F2 — `Curve`, the piecewise-linear shape of most tuning knobs: validation, evaluation rounded once, a bounds check, TOML input; plus the `curve <curve> at <x>` CLI command; done 2026-10-04 (#3)
- A1 — characters with a two-axis alignment and its nine-box label; loading `balance.toml` and `characters.toml` with every problem reported at once, by file and key path, with "did you mean" hints; Riverhold's sample characters; `load`, `characters`, `show character`; done 2026-10-04 (#4)
- A2 — the command-and-event core: `World::execute` (refused commands change nothing), numbered and time-stamped events, `World::replay`, the journal, `AdvanceTime`; `advance`, `time`, `events`, `journal`; done 2026-10-04 (#7)

---

## Phase 1 — Characters and alignment

### A3 · Actions move alignment — P0 · Next

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

**Validates** (DESIGN.md §12.2): action ids; each action's alignment names only `law` and `good`.

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

**Validates** (DESIGN.md §12.2): faction ids; weights are 0–1 with at least one above 0, for factions and characters; `metric` is one of the three.

### D2 · Disposition from affinity — P0 · Outline

**Covers:** DESIGN.md §8.1, the affinity component only.

**Scope:** bands, and `--explain`. Query `disposition(observer, subject)` returns the score, the band and the breakdown.

**Anchors**

- `city_watch` → player: −3.64, neutral
- `temple` → `sister_mira`: 45.34, friendly
- `temple` → `brother_ash`: −32.15, unfriendly
- `city_watch` → vex: −23.36, neutral

**Validates** (DESIGN.md §12.2): disposition bands have unique names, increasing `up_to`, and only the last band open-ended.

### D3 · Watched subjects and band-change events — P1 · Outline

**Covers:** DESIGN.md §8.3.

**Scope:** `Watch` and `Unwatch`, `DispositionBandChanged`, and `disposition.hysteresis`.

**Anchors:** to be written when D3 comes up next. They must cover:

- a band crossing, and the event it emits;
- no event when the score moves within a band;
- hysteresis holding a band near its edge.

**Validates** (DESIGN.md §12.2): `hysteresis` is ≥ 0.

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

**Validates** (DESIGN.md §12.2): **a membership names a faction that exists** (`characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)`), and at most once per character; 0 ≤ `tolerance` ≤ `member_tolerance`. Warning: a starting member outside their member tolerance. This is the first cross-file reference, so it also brings in the shared reference check described in DESIGN.md §12.2 (P-32).

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

**Validates** (DESIGN.md §12.2): a relation names two different factions that exist, and sets each direction at most once; relation bands follow the D2 band rules; `conflict_threshold` is within ±100; no character starts in two factions that are in conflict (invariant 6).

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

**Validates** (DESIGN.md §12.2): standing in characters, actions and outcomes names factions and characters that exist, with values within ±100; outcome ids.

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

**Validates** (DESIGN.md §12.2): a membership's rank exists on that faction's ladder; rank ids are unique within a faction; every ladder has at least one rung. Warnings: a starting member below their rank's standing requirement; a rank tolerance looser than the faction's member tolerance.

### M6 · Standing spillover between factions — P1 · Outline

**Covers:** the spillover part of DESIGN.md §7.1: one hop, using the `standing.spillover` curve.

**Anchors**

- Robbing vex (guild −10.00) → `city_watch` +1.80.
- `donate_to_temple` (temple +10.00) → `city_watch` +1.00 and `ashen_circle` −2.40.
- `lantern_guild` ↔ `free_company` (+20) spills nothing under the default curve.

**Validates** (DESIGN.md §12.2): spillover multipliers are within −1…1.

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

**Validates** (DESIGN.md §12.2): rule tables use only known conditions and outcomes; rank ids appear only in a faction's own tables, and only that faction's ranks; `rank_at_least` is ≥ 1; every table ends with a rule that always decides.

### M8 · Drift policies and runtime faction alignment — P1 · Outline

**Covers:** DESIGN.md §9.3.

**Scope**

- The drift policies, probation with `grace_ticks`, rank tolerance overrides, and `expel_standing_change`.
- `SetFactionAlignment` and `ShiftFactionAlignment`, which emit `FactionAlignmentChanged` and trigger a membership review.

**Anchors:** to be written when M8 comes up next. They must cover:

- every policy;
- probation that clears, and probation that expires;
- a faction alignment change that pushes a member out.

**Validates** (DESIGN.md §12.2): drift policies are known; probation has `grace_ticks` > 0 and a `then`.

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

**Validates** (DESIGN.md §12.2): `conflict.resolve` is `ask` or `auto`; `auto_after_ticks` is ≥ 0.

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

**Validates** (DESIGN.md §12.2): a character's `inertia`, and `inertia.default_profile`, name a profile that exists; profiles use only `law`/`good` × `toward_*` curves; multipliers are ≥ 0.

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

**Validates** (DESIGN.md §12.2): `by_target` multipliers are ≥ 0, and `by_target` names only an axis or `relation`.

## Phase 4 — Designer tooling and persistence

### T1 · Validation sweep and JSON Schema — P1 · Outline

- `factional validate <dir>` reports every error and warning at once, without loading a world. Each increment adds its own checks as it lands (DESIGN.md §12.2); T1 adds the warnings that need the whole world: a faction no starting character could join, and a rank no one can reach.
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
- `curve <knob>` prints a named knob's curve as a table, extending F2's `curve <curve> at <x>`; `curve <knob> at <x>` evaluates it.

### T4 · Saves — P2 · Outline

- Serialises a snapshot plus the event log, with a format version.
- Loading gives identical query answers; a property test checks it.
- Format details are settled here (X-3).

## Phase 5 — Knowledge and rumour

### K0 · Design pass: witnesses, ripple, perceived alignment — P2

A design section and its decisions, agreed before K1 starts. It settles X-1, and designs the double agents that D-19 asks for.

### K1 · Witnessed acts — P2 · Outline

**Validates** (DESIGN.md §12.2): `knowledge.model` is a known model.

### K2 · Ripple through the social graph — P2 · Outline

### K3 · Perceived alignment — P2 · Outline

Depends on K0's answer.

### K4 · Secret membership and double agents — P2 · Outline

An opt-in option to belong to a faction without its enemies knowing. Designed in K0 (D-19).

## Phase 6 — Embedding

### E0 · Choose a host engine and integration route — P3

Settles X-2.

### E1 · Host adapter — P3

## Beyond this module

### Q0 · Quest module design pass: reconciling questlines across factions — P3

The quest module comes after this one. Its first step is a design pass that settles X-4: what it means for a faction's quests and questlines to reconcile with every other faction they affect, and those factions' questlines, at every stage, and how the loader checks it (D-20, DESIGN.md §16). No quest content loads until that check exists.

---

## Adding an increment

Use the same shape as above:

1. The id, title, priority and status.
2. **Why.**
3. **Scope.**
4. **Acceptance** examples. Work their numbers out by hand from DESIGN.md's formulas; never take them from the code's output.
5. The CLI commands it ships.
6. Any knobs it adds. These go in DESIGN.md §12 too.

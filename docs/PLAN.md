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

`F0 → F1 → F2 → A1 → A2 → A3 → D1 → D2 → M1 → M2 → M3 → M4 → M5 → M7 → A4 → A5 → D3 → M6 → M8 → M11 → M9 → M10 → T1 → T2 → T3 → T4 → K0 → K1 → K2 → K3 → K4 → E0 → E1 → Q0 → U0`

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

---

## Phase 1 — Characters and alignment

## Phase 2 — Perception

## Phase 3 — Factions, standing and rank

## Phase 4 — Designer tooling and persistence

### T3 · Map, matrix and curves — P2 · Next

**Why:** DESIGN.md §12.3. Some tuning questions are about shape, not single numbers: who could join a faction, how everyone regards someone, and what a curve does across its range.

**Scope**

- **`map <faction>`** draws the alignment plane as text:
  - law across (chaotic left, lawful right), good up (good at the top), −100 to 100 on each axis;
  - the cells within the faction's tolerance (measured as `distance` measures it, with the faction's weights and the world's metric), the faction itself, and each character, with a key below;
  - **settle here:** the grid's size, what marks a cell, and how two characters sharing a cell are shown.
- **`matrix [<subject>...] [--csv]`** shows every observer's disposition score toward each subject (default: every character), observers as rows, factions first, each in id order. `--csv` gives the same as CSV, for a spreadsheet.
- **`curve <knob>`** prints a named knob's curve as a table: each point, and the value at every 10 between its first and last x. The knobs:
  - `disposition.affinity`, `standing.spillover`;
  - `inertia.<profile>.<axis>.<toward>`, such as `inertia.hardening.good.toward_good`;
  - `<action>.by_target.<curve>`, such as `murder.by_target.good`.
- **`curve <knob> at <x>`** evaluates it, as F2's `curve <curve> at <x>` does a written curve. Unknown knobs get a "did you mean".

**Acceptance**

1. `map city_watch` on content/sample marks Captain Hale (75, 30) inside the Watch's region and the player (0, 0) outside it, and its key gives the Watch's tolerance, 40.00.
2. In `map`, a cell's centre exactly at the tolerance counts as inside, as `can-join` would.
3. `matrix player` on content/sample gives Captain Hale's score as exactly what `disposition captain_hale player` gives; `matrix player --csv` starts with the header `observer,player`.
4. `curve disposition.affinity` lists 50.00 at 0, 0.00 at 60 and −50.00 at 200, and `curve disposition.affinity at 130` gives −25.00.
5. `curve inertia.hardening.good.toward_good at 50` gives 0.65, halfway between 1.0 at 0 and 0.3 at 100.
6. `curve disposition.affinty` suggests `disposition.affinity`.

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

### U0 · Editor design pass: web or egui — P3

Settles X-5. Comes after T1, whose JSON Schema can drive the forms, and after Q0, which defines the quest model a questline editor would edit. Likely prerequisites: a format-preserving TOML writer in `factional-content` (`toml_edit`), `Serialize` on query results and their breakdowns, and an editor facade crate beside `cli` that nothing depends on.

---

## Adding an increment

Use the same shape as above:

1. The id, title, priority and status.
2. **Why.**
3. **Scope.**
4. **Acceptance** examples. Work their numbers out by hand from DESIGN.md's formulas; never take them from the code's output.
5. The CLI commands it ships.
6. Any knobs it adds. These go in DESIGN.md §12 too.

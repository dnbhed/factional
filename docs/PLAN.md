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

`F0 → F1 → F2 → A1 → A2 → A3 → D1 → D2 → M1 → M2 → M3 → M4 → M5 → M7 → A4 → A5 → D3 → M6 → M8 → M9 → M10 → T1 → T2 → T3 → T4 → K0 → K1 → K2 → K3 → K4 → E0 → E1 → Q0 → U0`

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

---

## Phase 1 — Characters and alignment

## Phase 2 — Perception

## Phase 3 — Factions, standing and rank

### M8 · Drift policies and runtime faction alignment — P1 · Next

**Why:** D-9 and P-11. A member who drifts away from their faction's ideals is noticed, put on probation, demoted or thrown out, as the faction chooses (DESIGN.md §9.3).

**Scope**

- **Content.**
  - A faction's `drift = { policy = … }`: `ignore`, `flag`, `probation` (with `grace_ticks` > 0 and `then = "expel"` or `"demote"`), `demote` or `expel`.
  - `membership.default_drift` for factions that set none. Built in, it's `flag`.
  - A faction's `expel_standing_change`, −20.00 by default.
- **When drift is checked.** For each member whose alignment, or whose faction's alignment, a command changed. A member is drifting when their distance to the faction is past their member tolerance: the stricter of their rank's `tolerance` and the faction's `member_tolerance`, as in M7.
- **Probation expiry** is checked when time advances. It expires once `grace_ticks` have passed since it started.
- **Commands.** `SetFactionAlignment` and `ShiftFactionAlignment` emit `FactionAlignmentChanged { faction, from, to }`, then the faction's members are reviewed.
- **Events:** `MemberOutOfTolerance`, `MemberBackInTolerance`, `ProbationStarted`, `ProbationCleared`, `RankChanged` and `LeftFaction { reason: Expelled }`. An expulsion applies `expel_standing_change`, which spills like any standing change (M6).
- **CLI.** `faction-align <faction> <law> <good>`, `faction-shift <faction> --law <n> --good <n>`, and drift in `show faction`.

**Acceptance** (Riverhold, with the example world's drift policies)

1. **`flag` (the Guild).** Vex is a fence at −55 / −20, 7.07 from the Guild. Shifting the Guild +70 law, to 10 / −10, puts him 65.19 away (gaps 65 and 5 × 0.5): past the 60.00 member tolerance, so `MemberOutOfTolerance` and nothing else. Shifting it back −20, to −10 / −10, puts him 45.28 away, so `MemberBackInTolerance`.
2. **`expel` (the Ashen Circle).** Brother Ash is at 25 / −70, 10.08 from the Circle. Shifting the Circle +60 good, to 20 / −20, puts him 50.02 away (gaps 5 × 0.25 and 50): past 40.00. He's expelled: `LeftFaction(expelled)`, then standing with the Circle −20.00, which spills +4.80 to the Temple (−0.24 at −90).
3. **`demote` (the Temple).** A high priest at 30 / 50 is 30.00 from the Temple: past the high priest's own tolerance of 20.00, but within the Temple's 45.00. They're demoted to ordained, then checked again: ordained sets no tolerance, so the 45.00 applies, and they stay. A member already on the lowest rung would be expelled instead.
4. **`probation` (the Watch: 100 ticks, then expel).** Captain Hale is at 75 / 30. Shifting the Watch −30 law, to 40 / 20, puts him 35.09 away (gaps 35 and 10 × 0.25): past the captain's 25.00. So `ProbationStarted`, ending at tick 100.
   - **Cleared:** shifting the Watch +20 law, to 60 / 20, puts him 15.21 away, so `ProbationCleared`.
   - **Expired:** if nothing changes, `advance 100` expels him. That's `LeftFaction(expelled)`, then the Watch −20.00 (75.00 → 55.00), which spills +3.60 to the Guild and −2.00 to the Temple.
5. **`ignore` (the Free Company):** no drift events at all.
6. **Member alignment changes trigger it too:** an act that takes a member past their tolerance is checked like a faction change.
7. **Load errors at their keys:**
   - an unknown policy, with a "did you mean";
   - `probation` without `grace_ticks` or `then`;
   - `grace_ticks = 0`;
   - `then` other than `expel` or `demote`;
   - `expel_standing_change` outside ±100.

**Validates** (DESIGN.md §12.2): drift policies are known; probation has `grace_ticks` > 0 and a `then`.

### M9 · When two of your factions go to war — P1 · Outline

**Covers:** DESIGN.md §9.4, D-16.

**Scope**

- A relation change that puts two of a character's factions in conflict emits `MembershipConflict` and marks both memberships as conflicted. This replaces M2's interim refusal (P-38).
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

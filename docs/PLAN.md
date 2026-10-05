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
- A3 — actions move alignment: `actions.toml` (alignment deltas), `PerformAction` with scale and witnesses, `ActionPerformed` and `AlignmentChanged`, clamping at the ends of each axis, refusals with "did you mean"; `actions`, `act`; done 2026-10-04 (#8)
- D1 — weights and distance: `factions.toml` (name, alignment, weights), character weights, `alignment.metric` and `alignment.default_weights`; an exact `distance(observer, subject)` query with its working; one id namespace for factions and characters, checked by `World::new` (P-32's mechanism, arriving early); `factions`, `show faction`, `distance [--explain]`; done 2026-10-05 (#9)
- D2 — disposition from affinity: `disposition.affinity` (a curve, checked within ±100 by `World::new`) and `disposition.bands` (validated: valid unique names, increasing `up_to`, only the last open-ended); a `disposition(observer, subject)` query with score, band and working; `disposition [--explain]`; plain "expected a number" errors; done 2026-10-05 (#10)
- M1 — factions' `tolerance` and `member_tolerance`, starting `memberships`; `JoinFaction` (refused with every failing check) and `LeaveFaction`; `assess_join` and membership queries; load warnings, starting with members outside member tolerance; `can-join [--explain]`, `join`, `leave`, memberships in `show`; done 2026-10-05 (#11)
- M2 — relations between factions: `relations.toml` (`between` and `from`/`to`), relation bands and `conflict_threshold`; `SetRelation` and `ShiftRelation` with `RelationChanged`; enemy exclusion in `assess_join`; invariant 6 kept by refusing a war between a character's own factions until M9; `relations`, `relate`; done 2026-10-05 (#12)
- M3 — standing: a store of how factions and characters regard each character; starting standing, action `standing` effects (target, target factions, named), `outcomes.toml`, `ApplyOutcome`, `ApplyEffects` and `StandingChanged`; the `awareness` seam (1.00); `leave_standing_change`; `standing`, `outcomes`, `outcome`; done 2026-10-05 (#13)
- M4 — the full disposition: affinity, standing, kinship (with `same_faction`), faction opinion and modifiers (0 until M10), each clamped to ±100, weighted by `disposition.weights` and summed; `disposition --explain` shows DESIGN.md §8.2's table, with where kinship and faction opinion came from; done 2026-10-05 (#14)
- M5 — rank ladders: `[[<faction>.ranks]]` with standing requirements and stricter tolerances, starting ranks, new members on the lowest rung; `Promote` (only on request, refused with every unmet requirement) and `Demote`, with `RankChanged`; `assess_promotion`; rank load checks and warnings; `ranks`, `promote [--explain]`, `demote`; done 2026-10-05 (#16)

---

## Phase 1 — Characters and alignment

## Phase 2 — Perception

### D3 · Watched subjects and band-change events — P1 · Outline

**Covers:** DESIGN.md §8.3.

**Scope:** `Watch` and `Unwatch`, `DispositionBandChanged`, and `disposition.hysteresis`.

**Anchors:** to be written when D3 comes up next. They must cover:

- a band crossing, and the event it emits;
- no event when the score moves within a band;
- hysteresis holding a band near its edge.

**Validates** (DESIGN.md §12.2): `hysteresis` is ≥ 0.

## Phase 3 — Factions, standing and rank

### M6 · Standing spillover between factions — P1 · Outline

**Covers:** the spillover part of DESIGN.md §7.1: one hop, using the `standing.spillover` curve.

**Anchors**

- Robbing vex (guild −10.00) → `city_watch` +1.80.
- `donate_to_temple` (temple +10.00) → `city_watch` +1.00 and `ashen_circle` −2.40.
- `lantern_guild` ↔ `free_company` (+20) spills nothing under the default curve.

**Validates** (DESIGN.md §12.2): spillover multipliers are within −1…1.

### M7 · Joining an enemy: defectors and deserters — P0 · Next

**Why:** D-10, where joining an enemy of your faction depends on rank, standing and drift. M2's plain refusal becomes designer-tunable (DESIGN.md §9.2, P-10).

**Scope**

- **Content.**
  - `balance.toml` gains `[membership.defectors]` and `[membership.deserters]`, each with `rules = [...]`.
  - A faction can override either with its own `[<faction>.defectors]` or `[<faction>.deserters]`.
  - Conditions: `rank_at_least` and `rank_below` (a rung number, or one of the faction's own rank ids in its own tables); `standing_with_current_at_least` and `_below`; `standing_with_target_at_least` and `_below`; `closer_to_target`; `outside_member_tolerance`.
  - Outcomes: `accept` or `refuse` for defectors; `release` or `refuse` for deserters. Each has an optional `standing_change`, and a refusal has a `reason`.
- **Built-in defaults.** `defectors` refuses everyone and `deserters` releases everyone. That's exactly M2's behaviour (D-4).
- **Assessing.** For each current faction in conflict with the target:
  - that faction's `deserters` table must release;
  - then the target's `defectors` table must accept.
  - `assess_join` reports, per table, every rule tried and the one that fired.
- **Defecting.** If every table agrees, the join goes ahead:
  - `LeftFaction { reason: Defected }` for each conflicting faction, each followed by its deserter `standing_change`;
  - then `JoinedFaction`, followed by the defector `standing_change`.
- **`outside_member_tolerance`** uses the member's rank tolerance if it's stricter than the faction's `member_tolerance`, the same rule M8's drift will use.
- **CLI.** `can-join --explain` lists each table's rules and marks the one that fired.

**Acceptance** (Riverhold, with the sample tables in DESIGN.md §9.2)

1. **Built-in tables, no content:** a Guild member applying to the Watch is refused, `refused by the built-in defectors rule`. That reproduces M2.
2. **Vex reformed to 35 / 10, a fence (rank 2):**
   - to the Watch he's 35.09 ≤ 40.00, so eligible;
   - to the Guild he's 95.52 > 60.00, so drifted.
   - The Guild's `deserters` table releases him on `outside_member_tolerance` (rule 2). The Watch's `defectors` table accepts on `closer_to_target` (rule 2), with −10.00 standing toward the Watch.
   - Events: `LeftFaction(defected)` from the Guild, `JoinedFaction` to the Watch as a recruit, then `StandingChanged` with the Watch, 0 → −10.00.
3. **The same Vex as a shadow (rank 3):** refused by the Guild's `deserters` rule 1, `Officers don't walk away.`
4. **The Ashen Circle's own `deserters` table** refuses everyone: `No one leaves the Circle.`
5. **Standing first.** A Guild member with standing 50.00 with the Watch is accepted by `defectors` rule 1, with no cost.
6. **Load errors at their keys:**
   - an unknown condition, with a "did you mean";
   - an unknown outcome for the table;
   - `rank_at_least = 0`;
   - a rank id in the world table, or another faction's rank in a faction's table;
   - a table whose last rule has conditions, so it might not decide.

**Validates** (DESIGN.md §12.2): rule tables use only known conditions and outcomes; rank ids appear only in a faction's own tables, and only that faction's ranks; rank numbers are ≥ 1; every table ends with a rule that always decides.

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

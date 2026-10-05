# Factional — design: reputation & factions

> Draft, 2026-10-04. Decisions are referenced by id: **D-n** agreed, **P-n** proposed default, **O-n** open, **X-n** deferred. All of them, with reasons, are in [DECISIONS.md](DECISIONS.md). Delivery order is in [PLAN.md](PLAN.md).

## 1. What this is

Factional is a modular, single-player RPG engine written in Rust (D-1). It knows nothing about maps, physics, rendering or combat. Those are separate modules, built later, that talk to it through commands, events and queries (§11, D-15).

The first module, and the subject of this document, is **reputation & factions**. It covers:

- where each character stands morally;
- how characters and factions regard each other;
- how that changes as characters act, complete quests, and join or leave factions.

The module does not decide what anyone does (AI), run quests (they report their outcomes in, §11), resolve combat, or know where anything is.

## 2. Principles

1. **A world loads only if it is complete.** This is fundamental, and every module, now and future, must keep to it (D-20). A world is built only from content that is complete in principle: every reference resolves, every rule can decide, and nothing a game could reach is left undefined. For this module, that means the load-time checks in §12.2. For the quest module, it means each quest and questline for a faction must reconcile with every other faction it affects, and with those factions' questlines, at every stage (§16).
2. **World-agnostic.** No coordinates, no rendering, no wall clock. Time is an abstract tick count that the host advances (P-20).
3. **Functional core.** State changes only by executing a command. A command either fails and changes nothing, or succeeds and emits events. Applying those events is the only thing that mutates state (P-14).
4. **Deterministic.** The same content and the same commands give byte-identical events on every machine. That means integer fixed-point maths, ordered collections and no randomness (P-1, P-19).
5. **Designer-first.** Every number that affects balance is content, not code (§12). Content errors name the file and key, and say what's wrong.
6. **Explainable.** Every evaluation returns its working: a disposition, a refused join, an alignment shift. The numbers in the working are exactly the numbers used (P-24). The CLI only renders them.
7. **The player is just a character** (P-17). Every rule applies to NPCs the same way, so NPC-to-NPC relationships come free.

## 3. Glossary

| Term | Meaning |
| --- | --- |
| **Alignment** | Where a character or faction sits morally, on two axes: **law** (−100 chaotic … +100 lawful) and **good** (−100 evil … +100 good). A character's alignment moves when they act. |
| **Weights** | How much an observer cares about each axis, 0.00–1.00 each. The City Watch cares about law far more than kindness. |
| **Distance** | How far apart two alignments are, as seen through the observer's weights. |
| **Inertia** | How hard a character's alignment is to move, given where it already is and which way it's being pushed. |
| **Standing** | What a faction or character thinks of someone because of their history: quests done, crimes committed. −100…+100. Separate from alignment and from rank, and held by non-members too. |
| **Rank** | A member's rung on a faction's ladder (Recruit → Sergeant → Captain). Only members have one. |
| **Tolerance** | How close to a faction's alignment you must be to join it. |
| **Member tolerance** | How far a member may drift before the faction's drift policy applies. |
| **Relation** | How one faction regards another, −100…+100, in bands: enemy, rival, neutral, friendly, allied. |
| **In conflict** | Two factions where either regards the other at or below the conflict threshold. |
| **Disposition** | How an observer (a character or a faction) regards a character right now. It's a score built from affinity, standing, kinship, faction opinion and modifiers, then put in a band (unfriendly, neutral, friendly). Always calculated, never stored. |
| **Affinity** | The part of disposition that comes from alignment distance. |
| **Action** | Something a character does, such as stealing or helping a stranger. Defined in content, with its effects. |
| **Outcome** | A named bundle of effects applied directly. This is how a quest's result arrives, both before and after a quest module exists. |
| **Effect** | One atomic change: shift alignment, change standing, change a relation, promote, and so on. |
| **Tick** | One unit of abstract time, advanced by the host. |

## 4. Numbers and curves

### 4.1 Fixed-point (P-1)

All rule arithmetic uses `Fixed`, a signed number with exactly two decimal places, stored as an integer count of hundredths.

- **Content.** Designers write ordinary decimals (`12.5`, `-3`, `0.25`). More than two decimal places is a content error, never a silent rounding.
- **Rounding.** Every rule computes exactly in integers and rounds **once**, half away from zero: 0.025 → 0.03, −0.025 → −0.03.
- **Explanations add up.** Where an explanation shows intermediate terms, each term shown is rounded, and the total is the sum of the terms shown.
- **No floats in rules.** At the TOML boundary, a float is converted through its shortest decimal string (`0.3` → `"0.3"` → 0.30), so nothing does float arithmetic at all.

Why: integer maths is identical on every platform, test expectations are exact, and what the CLI shows is exactly what the engine holds.

### 4.2 Curves (P-4)

Most tuning knobs are **curves**: piecewise-linear maps from one number to another. A curve is either a constant, or a list of `[x, y]` points with strictly increasing x.

```toml
deepening = 1.0                                          # constant
affinity  = [[0.0, 50.0], [60.0, 0.0], [200.0, -50.0]]   # points
```

How a curve is evaluated:

- Below the first point it holds the first y; above the last point it holds the last y.
- Between two points it interpolates linearly and rounds once:

```
y = ( y0·(x1 − x0) + (y1 − y0)·(x − x0) ) / (x1 − x0)
```

Curves are the one shape designers have to learn. In the CLI, `curve <curve> at <x>` gives any curve's value at a point, such as `curve [[0, 50], [60, 0], [200, -50]] at 70.18` → `-3.64`. T3 adds printing a knob's curve as a table.

## 5. Alignment

### 5.1 The two axes (D-2, P-2)

Alignment has two axes, `law` and `good`, each running −100.00…+100.00.

- Shifts clamp at either end.
- A starting alignment outside that range is a content error; it's never clamped.

Labels (Lawful Good … Chaotic Evil, with True Neutral in the middle) are **display only**:

- an axis value ≥ `alignment.label_threshold` (default 33.00) reads as Lawful or Good;
- a value ≤ −threshold reads as Chaotic or Evil;
- anything between reads as Neutral.

No rule ever branches on a label. That keeps behaviour continuous, with no cliff at 33.

### 5.2 Actions move alignment

Each action in the catalogue has an alignment delta. When a character performs it:

```
shift(axis) = round( base(axis) × scale × target(axis) × inertia(axis, direction, position) )
new(axis)   = clamp( position(axis) + shift(axis), −100, +100 )
```

- `scale` (default 1.00, must be > 0) lets the host say how big this instance was, such as stealing a loaf versus a crown.
- `target` is the target-aware multiplier (§5.4). It's 1.00 when the action has no target scaling, or no target.
- `inertia` is read from the character's inertia profile at their position **before** the act (P-5).
- Every multiplier is ≥ 0, so each can damp or amplify a shift but never reverse it. A shift's direction is always the action's own.

`PerformAction` emits `ActionPerformed`, then `AlignmentChanged { from, to }` if the actor's alignment moved. An actor already at the end of an axis the act pushes toward gets no `AlignmentChanged`. A scale too large to compute with still lands at the end of the axis; it never overflows. In `actions.toml`, an axis an action leaves out isn't moved (P-34).

### 5.3 Inertia profiles (D-6, P-5)

A profile gives up to four curves: one per axis per direction. Each maps the character's current position to a multiplier. Any curve a profile leaves out is 1.0.

```toml
[inertia]
default_profile = "steady"

[inertia.profiles.steady]       # every act counts in full

[inertia.profiles.hardening]    # convictions harden toward the extremes
good.toward_good = [[-100.0, 0.5], [0.0, 1.0], [100.0, 0.3]]
good.toward_evil = [[-100.0, 0.3], [0.0, 1.0], [100.0, 0.5]]
```

A character opts in with `inertia = "hardening"`.

Worked example: take a *hardening* character at good 60.00.

- They help a stranger (good +4.00). `toward_good` at 60 is 0.58, so the shift is +2.32 and they end at 62.32.
- If instead they extort someone (good −6.00), `toward_evil` at 60 is 0.70, so the shift is −4.20 and they end at 55.80.

The same mechanism expresses other shapes:

- **Fall from grace:** `toward_evil` above 1.0 at high good.
- **Slow redemption:** `toward_good` below 1.0 at low good.
- **Rigid lawful types:** both law curves low at high law.

### 5.4 Target-aware effects (D-18, P-28)

An action can scale its alignment effect by who it's done to. It has two optional curves, and both give multipliers ≥ 0:

- **`by_target.<axis>`:** a curve over the target's position on that axis. It scales the action's delta on that axis.
- **`by_target.relation`:** a curve over the most hostile relation from any of the actor's factions toward any of the target's factions. That's 0 if either side belongs to no faction. It scales both axes.

```toml
[murder]
alignment = { law = -10.0, good = -15.0 }
by_target.good     = [[-100.0, 0.2], [0.0, 1.0], [100.0, 1.5]]   # killing the wicked is less evil
by_target.relation = [[-100.0, 0.5], [-50.0, 0.8], [0.0, 1.0]]   # so is killing a sworn enemy
```

Worked examples:

| Act | Multipliers | Shift |
| --- | --- | --- |
| The player murders brother_ash (good −70) | good × 0.44 | good −6.60, law −10.00 |
| The player murders sister_mira (good 85) | good × 1.43 | good −21.45, law −10.00 |
| captain_hale (City Watch) murders vex (Lantern Guild, good −20) | good × 0.84 (target); both × 0.62 (the Watch regards the guild at −80); good × 0.85 (hale's hardening inertia) | good −6.64, law −6.20 |

A target can soften or sharpen an act, but never make it good. An act that should be good against some targets, such as slaying a demon, is a separate action that the host chooses.

## 6. Weights and distance (D-5, P-3, P-21)

Each faction has **weights** for the two axes, 0.00–1.00 each, with at least one above 0. A character may declare their own; if not, they use `alignment.default_weights` (1.00 / 1.00).

Distance from observer O to subject S is measured with O's weights:

```
d = sqrt( (w_law · Δlaw)² + (w_good · Δgood)² )      # euclidean, the default
```

It's computed exactly and rounded once: the weighted gaps are exact in ten-thousandths, and the Euclidean root is an exact integer square root, rounded half up to hundredths (P-35).

Observers are factions or characters; subjects are characters. Factions and characters share one set of ids, so an id alone names an observer (P-35).

`alignment.metric` can switch the whole world to a different measure:

- `manhattan`: w_law·|Δlaw| + w_good·|Δgood|
- `chebyshev`: the larger of those two terms

That changes the shape of every tolerance region from an ellipse to a diamond or a rectangle.

Worked example: the City Watch (alignment 70 / 20, weights 1.00 / 0.25) looks at a true-neutral player at 0 / 0.

- The gaps are Δlaw 70 and Δgood 20.
- Weighted, they become 70 and 5.
- So d = 70.18. Under Manhattan it would be 75.00; under Chebyshev, 70.00.

## 7. Standing and rank (D-11)

### 7.1 Standing (P-8)

`standing(subject, party)` is how `party` regards `subject` because of what has passed between them. The party is a faction or a character.

- Range −100…+100, default 0 (or a value set in content), clamped.
- It exists whether or not the subject is a member of anything.

Standing changes through effects:

- an action's `standing` block: toward the target, toward the target's factions, or toward named factions or characters;
- outcome bundles;
- `ApplyEffects` sent by other modules.

```toml
[steal]
alignment = { law = -5.0, good = -3.0 }
standing  = { target = -20.0, target_factions = -10.0 }

[donate_to_temple]
alignment = { good = 3.0 }
standing  = { factions = { temple = 10.0 } }
```

**Spillover (P-13).** When standing with faction F changes, standing with every other faction G changes too, by:

```
change × spillover( relation(G → F) )
```

`standing.spillover` is a curve. By default:

- allies share up to half of your gain or loss;
- bitter enemies feel a little of the opposite;
- spillover happens once and never cascades.

Example: robbing a member of the Lantern Guild costs −10.00 with the guild. The City Watch regards the guild at −80, so you gain +1.80 with the Watch.

### 7.2 Rank (P-9)

Each faction defines a ladder of ranks. Members join on the bottom rung.

```toml
[[city_watch.ranks]]
id = "recruit"
[[city_watch.ranks]]
id = "sergeant"
requires = { standing = 30.0 }
[[city_watch.ranks]]
id = "captain"
requires = { standing = 70.0 }
tolerance = 25.0          # captains are held to a stricter standard
```

- `Promote` moves a member up one rung if the next rank's requirements hold: standing at or above its minimum, and alignment within that rank's `tolerance` if it sets one.
- `Demote` moves a member down one rung.
- A refusal says which requirement failed.
- Promotion only happens when something asks for it, such as a quest, dialogue or the CLI sending `Promote` (D-17). Meeting the requirements makes a promotion possible, never automatic.
- Rank feeds the join rules (§9.2) and the drift policy (§9.3).

## 8. Disposition (D-3, P-6, P-7)

`disposition(observer, subject)` is how an observer (a character or a faction) regards a subject (a character). It's calculated on demand and never stored; the only thing kept is band tracking for watched subjects (§8.3).

### 8.1 Components

| Component | When the observer is a character | When the observer is a faction |
| --- | --- | --- |
| **affinity** | `disposition.affinity` curve applied to the distance, using the observer's weights | the same, using the faction's alignment and weights |
| **standing** | the observer's personal standing toward the subject | the subject's standing with the faction |
| **kinship** | the sum of relation(F → G) over every pairing of an observer faction F with a subject faction G; a faction they share counts as `disposition.same_faction` (default +50) | the same, with the faction itself as the only F |
| **faction opinion** | the sum of the subject's standing with each of the observer's factions: the NPC partly adopts their factions' view | not used |
| **modifiers** | the sum of active modifiers from other modules (§11) | the same |

Each component is clamped to ±100. The score is the weighted sum of the components, and its band is the first one whose `up_to` is at or above the score:

```
score = clamp( Σ round(weight_c × component_c), −100, +100 )
band  = the first band whose up_to ≥ score
```

Defaults:

| Setting | Default |
| --- | --- |
| weights | affinity 1.00, standing 1.00, kinship 0.50, faction opinion 0.50, modifiers 1.00 |
| affinity curve | `[[0, 50], [60, 0], [200, -50]]` |
| bands | unfriendly ≤ −25 < neutral ≤ 25 < friendly |

Designers may add more bands, such as hostile or allied.

### 8.2 Worked example

**The situation:**

- The player has stolen twice from a merchant, so their alignment is now −10.00 / −6.00.
- They've been fined by the Watch. That's outcome `fined_by_watch`: standing −20 with the City Watch and −10 with Captain Hale personally.

**The observer:** Captain Hale, alignment 75 / 30, weights 1.00 / 0.25, member of the City Watch.

```
disposition captain_hale → player = −29.10 (unfriendly)
  affinity          −9.10 × 1.00 =  −9.10   distance 85.48 (law Δ85.00 × 1.00, good Δ36.00 × 0.25)
  standing         −10.00 × 1.00 = −10.00
  kinship            0.00 × 0.50 =   0.00   player belongs to no factions
  faction opinion  −20.00 × 0.50 = −10.00   city_watch −20.00
  modifiers          0.00 × 1.00 =   0.00
  bands: unfriendly ≤ −25.00 < neutral ≤ 25.00 < friendly
```

### 8.3 Band changes as events (P-23)

Other modules care when a guard turns unfriendly, not about every 0.01 of movement. So disposition changes are reported only at band boundaries, and only for subjects the host asks about:

- The host marks some subjects as **watched**, typically the player and their companions.
- After every command, the engine recomputes every observer's disposition toward each watched subject.
- When a band changes, it emits `DispositionBandChanged`.
- `disposition.hysteresis` (default 0) is a margin: leaving a band means crossing its edge by at least that much. It stops a score hovering on a boundary from flickering between bands.

## 9. Factions

### 9.1 Joining and leaving (D-4, D-8, P-11, P-22)

A faction has:

- an alignment, set by the designer and changeable at runtime by commands from other modules;
- weights;
- a `tolerance`;
- a `member_tolerance`, which is at least the tolerance and defaults to it. This is deliberate hysteresis: it's easier to stay than to get in.

`JoinFaction` succeeds when the character:

- isn't already a member;
- is within tolerance (distance ≤ `tolerance`);
- isn't blocked by a membership conflict (§9.2).

A refusal lists every failing check with its numbers:

> refused: 50.04 from the Lantern Guild's ideals, tolerance is 45.00

`LeaveFaction` always succeeds. It applies the faction's `leave_standing_change`, which defaults to 0.

### 9.2 Joining an enemy of a faction you're in (D-10, P-10)

By default you can't. Designers loosen that with two ordered rule tables, evaluated for each conflicting membership:

- the **target's `defectors`** table: will it accept someone from its enemy?
- the **current faction's `deserters`** table: will it let you go, and at what cost?

How the tables work:

- Rules are checked top to bottom, and the first whose conditions all hold decides.
- **Conditions** come from a fixed vocabulary:
  - `rank_at_least`, `rank_below`: rank position, 1 = lowest. A faction's own tables may use rank ids instead.
  - `standing_with_current_at_least`, `standing_with_current_below`
  - `standing_with_target_at_least`, `standing_with_target_below`
  - `closer_to_target`: distance to the target is less than distance to the current faction, each measured with that faction's own weights.
  - `outside_member_tolerance`: of the current faction, using the same rule as §9.3.
- **Outcomes:**
  - `defectors` → `accept`, with an optional `standing_change` toward the target, or `refuse`, with a reason.
  - `deserters` → `release`, with an optional `standing_change` toward the current faction, or `refuse`.
- **Built-in defaults:** `defectors` refuses everyone and `deserters` releases everyone. That's exactly D-4: enemies exclude each other. The sample world ships richer tables:

```toml
[membership.defectors]      # world default; any faction can override with its own
rules = [
  { when = { standing_with_target_at_least = 50.0 }, then = "accept" },
  { when = { closer_to_target = true }, then = "accept", standing_change = -10.0 },
  { then = "refuse", reason = "You serve our enemies." },
]

[membership.deserters]
rules = [
  { when = { rank_at_least = 3 }, then = "refuse", reason = "Officers don't walk away." },
  { when = { outside_member_tolerance = true }, then = "release" },
  { then = "release", standing_change = -40.0 },
]
```

The result:

- **Defection.** If every `deserters` table releases and the target accepts, the character joins the target and leaves each conflicting faction, with the standing changes applied. Each defection emits a `LeftFaction { reason: Defected }` and a `JoinedFaction` event.
- **Refusal.** Otherwise nothing changes, and the refusal names the rule that fired.

Worked example: Vex, a fence (rank 2) of the Lantern Guild, has reformed to 35 / 10.

1. To the City Watch, Vex is 35.09 away. The tolerance is 40, so Vex is eligible.
2. To the Guild, Vex is 95.52 away. The member tolerance is 60, so Vex has drifted.
3. The Guild's `deserters` table releases Vex on `outside_member_tolerance`.
4. The Watch's `defectors` table accepts on `closer_to_target`, with −10 standing.

At rank 3 (Shadow), the first `deserters` rule would have refused.

The tables are deliberately not a scripting language: conditions within a rule are ANDed, OR means writing another rule, and there's nothing else. If designers outgrow that, it's a decision to revisit, not a reason to add operators piecemeal.

### 9.3 Drift (D-9, P-11)

The drift policy applies when a member's distance to their faction exceeds their **member tolerance**: the rank's `tolerance` if their rank sets one, otherwise the faction's `member_tolerance`.

| Policy | Effect |
| --- | --- |
| `ignore` | Nothing. |
| `flag` | `MemberOutOfTolerance` and `MemberBackInTolerance` events, nothing else. This is the world default. |
| `probation` | `ProbationStarted`. If they're back within tolerance before `grace_ticks` pass: `ProbationCleared`. Otherwise `then` applies: `expel` or `demote`. |
| `demote` | Down one rank immediately, then checked again. A member already on the lowest rung is expelled. |
| `expel` | `LeftFaction { reason: Expelled }`, applying `expel_standing_change` (default −20). |

Drift is checked whenever the member's alignment, the faction's alignment, or a tolerance changes. Probation expiry is checked when time advances.

### 9.4 Relations between factions (P-12)

`relation(F → G)` runs −100…+100.

- **Direction.** It may be asymmetric (the Watch can distrust mercenaries more than they distrust it), but it's authored symmetrically by default.
- **Bands** default to enemy ≤ −50 < rival ≤ −15 < neutral ≤ 15 < friendly ≤ 50 < allied.
- **Conflict.** Two factions are in conflict when either regards the other at or below `relations.conflict_threshold` (−50).
- **Runtime changes.** Relations change through `SetRelation` and `ShiftRelation`, for wars and treaties sent by a quest module or the CLI.

**War between your own factions (D-16).** If a relation change puts two of a character's factions in conflict:

- The engine emits `MembershipConflict` and marks both memberships as conflicted. Invariant 6 allows that state until it's resolved.
- The host or a quest resolves it with `ResolveConflict { character, keep }`. The other membership ends with `LeftFaction { reason: ConflictResolved }`.
- Optionally, `membership.conflict` sets an automatic rule instead, applied straight away or after `auto_after_ticks` if nobody has resolved it. The rule keeps the higher rank, then the higher standing, then the longer service, then the lower faction id.
- The exact standing consequences of leaving are settled when M9 comes up.

## 10. Knowledge (D-7)

For now every character and faction knows about every action immediately (`knowledge.model = "omniscient"`). Two things are already in place so this can change later without touching the API:

- `PerformAction` carries `witnesses`: everyone, these characters, or nobody.
- Every standing effect of an action is multiplied by `awareness(party, event)`, which is 1.00 under the omniscient model. That function is the seam later models replace.

Planned for Phase 5, after its own design pass (K0):

- **`witnessed`:** only the witnesses and the target learn of the act.
- **`ripple`:** news spreads from the witnesses through a social graph made of designer-declared connections between characters plus shared faction membership.
  - It loses strength at each hop, takes ticks to travel, and stops below a threshold.
  - Effects scale with awareness, so hearsay counts for less than seeing it.
  - It's deterministic: breadth-first, with ties broken by id.
- **Secret membership and double agents (D-19):** an opt-in option to belong to a faction without its enemies knowing. It's designed in K0 and built in K4, because secrecy only means something once factions can be unaware of things.
- **Still to settle in K0 (X-1):** whether observers judge a character's alignment by what they know about them, and what that costs in memory in a large world.

## 11. API for other modules (D-15, P-14, P-15)

The module exposes commands, events and queries, and nothing else. Other modules never reach into its state, and it never calls into theirs.

### 11.1 Commands (in)

| Command | Introduced | Typical sender |
| --- | --- | --- |
| `AdvanceTime { ticks }` | A2 | the host's game loop |
| `PerformAction { actor, action, target?, scale, witnesses }` | A3 | combat, dialogue, world interaction, AI |
| `JoinFaction`, `LeaveFaction` | M1 | dialogue, quests |
| `SetRelation`, `ShiftRelation` | M2 | quests, world events |
| `ApplyOutcome { outcome, actor }`, `ApplyEffects { source, effects }` | M3 | quests and missions |
| `Promote`, `Demote` | M5 | quests, dialogue |
| `SetFactionAlignment`, `ShiftFactionAlignment` | M8 | world events, quests |
| `ResolveConflict { character, keep }` | M9 | quests, dialogue |
| `AddModifier`, `RemoveModifier` | M10 | status, characteristics |
| `Watch`, `Unwatch` | D3 | the host |

### 11.2 Events (out)

`execute` returns the events it produced, and the host forwards them to whoever cares. Each event carries a sequence number, the tick, and absolute before and after values.

| Area | Events |
| --- | --- |
| Time and actions | `TimeAdvanced`, `ActionPerformed`, `AlignmentChanged` |
| Standing and membership | `StandingChanged`, `JoinedFaction`, `LeftFaction { Voluntary \| Defected \| Expelled \| ConflictResolved }`, `RankChanged` |
| Drift | `ProbationStarted`, `ProbationCleared`, `MemberOutOfTolerance`, `MemberBackInTolerance` |
| Faction changes | `FactionAlignmentChanged`, `RelationChanged`, `MembershipConflict` |
| Disposition | `DispositionBandChanged`, `ModifierAdded`, `ModifierExpired` |

### 11.3 Queries (read)

- `alignment`, `distance`
- `disposition`, with its breakdown
- `standing`
- `membership`: rank, status and when they joined
- `assess_join`: the outcome plus the rules that fired
- `relation`
- `now`: the current tick
- `events_since`: the events after a sequence number (P-33)
- `journal`: every command issued, and whether it was accepted

`World::replay(content, events)` rebuilds a world from its event log without running any rules: what saves are built on.

### 11.4 How the future modules plug in

| Module | Sends | Reads or listens for |
| --- | --- | --- |
| Quests and missions | `ApplyOutcome`, `ApplyEffects`, `Promote`, `ResolveConflict`, relation changes | `assess_join`, rank, standing; `MembershipConflict` as a story hook |
| Combat | `PerformAction` (attack, kill, spare, flee) | disposition bands, to decide who is hostile; `DispositionBandChanged` |
| Status (disguise, wanted) | `AddModifier`; `witnesses = nobody` | — |
| Characteristics (charisma…) | `AddModifier`, or a `scale` on actions | — |
| Physics and world | who witnessed what; `AdvanceTime` | — |
| Graphics and UI | — | everything, read-only |

The Rust shape, as an illustration:

```rust
let content = factional_content::load_dir("content/sample")?;   // the I/O happens here
let mut world = World::new(content)?;                             // pure from here on
let events = world.execute(Command::PerformAction {
    actor: player, action: steal, target: Some(ava),
    scale: Fixed::ONE, witnesses: Witnesses::Everyone,
})?;
let d = world.disposition(Observer::Character(hale), player);    // d.score, d.band, d.breakdown
```

### 11.5 The journal and the event log (P-16)

- The **journal** is the commands as they were issued. Replaying it against changed content answers "what would have happened with these numbers?" It's the designer's what-if tool (T2).
- The **event log** is what happened, with the resolved values. Replaying it reproduces state exactly, whatever the content says now. Saves are built on it (T4).

## 12. Tuning surface (D-12)

This table lists every knob: where it lives, its default, and the increment that adds it. An increment that adds a knob also adds a row here, an entry in the JSON Schema, and a value in the sample content.

| Knob | Where | Default | Added in |
| --- | --- | --- | --- |
| `alignment.label_threshold` | balance.toml | 33.00 (0.01–100.00) | A1 |
| starting alignment | characters.toml | — | A1 |
| action alignment deltas | actions.toml | — | A3 |
| `inertia.default_profile`, `inertia.profiles.*` | balance.toml | `steady` (all 1.0) | A4 |
| character `inertia` | characters.toml | the default profile | A4 |
| action `by_target.<axis>`, `by_target.relation` | actions.toml | none (1.00) | A5 |
| `alignment.default_weights` | balance.toml | 1.00 / 1.00 | D1 |
| `alignment.metric` | balance.toml | `euclidean` | D1 |
| faction or character `weights` | factions.toml, characters.toml | the default weights | D1 |
| `disposition.affinity` | balance.toml | `[[0, 50], [60, 0], [200, -50]]` | D2 |
| `disposition.bands` | balance.toml | unfriendly ≤ −25 < neutral ≤ 25 < friendly | D2 |
| `disposition.hysteresis` | balance.toml | 0 | D3 |
| faction `tolerance`, `member_tolerance` | factions.toml | — / same as tolerance | M1 |
| faction `leave_standing_change` | factions.toml | 0 | M1 |
| relations | relations.toml | 0 | M2 |
| `relations.bands`, `relations.conflict_threshold` | balance.toml | see §9.4; −50 | M2 |
| action `standing` effects | actions.toml | none | M3 |
| outcomes | outcomes.toml | — | M3 |
| initial standings | characters.toml | 0 | M3 |
| `disposition.weights`, `disposition.same_faction` | balance.toml | 1 / 1 / 0.5 / 0.5 / 1; 50 | M4 |
| faction `ranks` (requirements, tolerance) | factions.toml | — | M5 |
| `standing.spillover` | balance.toml | `[[-100, -0.3], [-50, 0], [50, 0], [100, 0.5]]` | M6 |
| `membership.defectors`, `membership.deserters`, per-faction overrides | balance.toml, factions.toml | refuse all / release all | M7 |
| faction `drift`, `expel_standing_change`; `membership.default_drift` | factions.toml, balance.toml | `flag`; −20 | M8 |
| `membership.conflict` (`resolve`, `auto_after_ticks`) | balance.toml | resolved by the host or a quest | M9 |
| `knowledge.model` | balance.toml | `omniscient` | K1 |

### 12.1 Content layout (P-18)

```
content/<world>/
  balance.toml      global rules, curves, bands, defaults, rule tables
  factions.toml     factions, ranks, per-faction overrides
  relations.toml    faction ↔ faction
  characters.toml   characters, starting alignment, memberships, standings
  actions.toml      the action catalogue
  outcomes.toml     named effect bundles (quest results and the like)
```

- **Missing files are fine.** A missing file means the defaults, or none of that kind (P-31).
- **Errors stop loading.** Unknown keys are errors, with a "did you mean". References are checked across files, and values are range-checked. Each error names the file and the key path, for example: `characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)`.
- **Warnings don't stop loading.** For example: a starting member outside member tolerance, a rank whose requirements can never be met, or a faction no starting character could ever join.
- **Editor support.** `factional schema` writes a JSON Schema, so an editor (VS Code with Even Better TOML) autocompletes and underlines mistakes as the designer types (T1).

### 12.2 Validation (P-32)

Content is data, so its equivalent of a compile step is loading. **A world is only ever built from content that passed every check.** A running world never holds a reference to a faction, rank, profile or character that doesn't exist.

- **Errors stop loading.** Nothing is half-loaded, and the CLI keeps the world it already had.
- **Warnings don't stop loading.** `load` prints them after its summary, and `factional validate` (T1) lists them too.
- **The rules live in `factional-reputation`.** `World::new` runs the checks and refuses invalid content, so a host that builds content in code, not from TOML, gets the same protection. `factional-content` turns each problem's location into `file: key.path`.
- **CI loads `content/sample` on every PR.** Broken sample content fails the build like a compile error. Once every setting is implemented, `docs/examples/riverhold` is loaded too.
- **Each check arrives with the increment that adds the content it checks**, never later.

| Check | Kind | Added in |
| --- | --- | --- |
| Ids are lowercase letters, digits and `_`, starting with a letter | error | A1 (done) |
| Alignment axes within −100…100; `label_threshold` 0.01–100 | error | A1 (done) |
| Unknown keys, missing fields, wrong types, bad numbers | error | A1 (done), then every increment for its own fields |
| Action alignment names only `law` and `good` | error | A3 (done) |
| `inertia` and `inertia.default_profile` name a profile that exists; multipliers ≥ 0 | error | A4 |
| `by_target` multipliers ≥ 0 | error | A5 |
| Faction ids are unique across factions and characters | error | D1 (done) |
| Weights 0–1 with at least one above 0; `metric` is a known metric | error | D1 (done) |
| Bands: unique names, increasing `up_to`, only the last open-ended | error | D2 (disposition), M2 (relations) |
| `hysteresis` ≥ 0 | error | D3 |
| A membership names a faction that exists, at most once per character | error | M1 |
| 0 ≤ `tolerance` ≤ `member_tolerance` | error | M1 |
| A starting member outside their member tolerance | warning | M1 |
| A relation names two different factions that exist; each direction is set at most once | error | M2 |
| No character starts in two factions that are in conflict (invariant 6) | error | M2 |
| Standing names factions and characters that exist; values within ±100 | error | M3 |
| A membership's rank is on that faction's ladder; rank ids unique; every ladder has a rung | error | M5 |
| A starting member below their rank's standing requirement; a rank tolerance looser than the faction's | warning | M5 |
| Spillover multipliers within −1…1 | error | M6 |
| Rule tables use known conditions; rank ids only in a faction's own tables and only its ranks; every table ends with a rule that always decides | error | M7 |
| Drift policies are known; probation has `grace_ticks` > 0 and a `then` | error | M8 |
| `conflict.resolve` is `ask` or `auto`; `auto_after_ticks` ≥ 0 | error | M9 |
| `knowledge.model` is a known model | error | K1 |
| A faction no starting character could join; a rank no one can reach | warning | T1 |

### 12.3 Designer workflow

- `factional repl content/sample` to poke at a world.
- `calc 4.00 * 0.41` in the REPL to check exactly how the engine rounds a calculation.
- `curve [[0, 1.0], [100, 0.5]] at 25` in the REPL to try a curve's shape before using it.
- `factional run scenarios/<name>.scenario` to replay a scripted playthrough.
- `--explain` on `act`, `disposition`, `can-join` and `promote` to see the working.
- `factional compare <scenario> --content A --against B` to see what new numbers change (T2).
- `reload` in the REPL to re-read the content and replay the session.
- `map <faction>` to draw the alignment plane and who could join (T3), and `matrix --csv` to get dispositions into a spreadsheet.

## 13. The sample world: Riverhold

Every example in this document and in PLAN.md uses this world. It lives in `content/sample/` and is built up increment by increment. [docs/examples/riverhold](examples/riverhold/README.md) shows all of it as content files, every setting included. Changing a number here means updating the examples that use it, and those files.

**Factions**

| id | Name | Alignment (law / good) | Weights | Tolerance / member | Drift |
| --- | --- | --- | --- | --- | --- |
| `city_watch` | The City Watch | 70 / 20 | 1.00 / 0.25 | 40 / 50 | probation, 100 ticks, then expel |
| `temple` | Temple of the Dawn | 30 / 80 | 0.50 / 1.00 | 35 / 45 | demote |
| `lantern_guild` | The Lantern Guild | −60 / −10 | 1.00 / 0.50 | 45 / 60 | flag |
| `free_company` | The Free Company | −10 / 0 | 0.25 / 0.25 | 60 / 80 | ignore |
| `ashen_circle` | The Ashen Circle | 20 / −80 | 0.25 / 1.00 | 30 / 40 | expel |

**Ranks** (minimum standing in brackets)

| Faction | Ladder, lowest first |
| --- | --- |
| `city_watch` | recruit → sergeant (30) → captain (70, tolerance 25) |
| `lantern_guild` | cutpurse → fence (25) → shadow (60) |
| `temple` | acolyte → ordained (30) → high_priest (80, tolerance 20) |
| `ashen_circle` | initiate → adept (40) |
| `free_company` | sellsword → sergeant (30) |

**Relations** (symmetric unless an arrow shows a direction)

| Factions | Relation |
| --- | --- |
| `city_watch` ↔ `lantern_guild` | −80 |
| `temple` ↔ `ashen_circle` | −90 |
| `city_watch` ↔ `temple` | +60 |
| `city_watch` ↔ `ashen_circle` | −40 |
| `lantern_guild` ↔ `free_company` | +20 |
| `city_watch` → `free_company` | −30 |
| `free_company` → `city_watch` | −10 |

**Characters**

| id | Alignment | Weights | Inertia | Memberships (rank, standing) |
| --- | --- | --- | --- | --- |
| `player` | 0 / 0 | default | steady | — |
| `captain_hale` | 75 / 30 | 1.00 / 0.25 | hardening | city_watch (captain, 75) |
| `sister_mira` | 35 / 85 | default | hardening | temple (ordained, 40) |
| `vex` | −55 / −20 | default | steady | lantern_guild (fence, 30) |
| `merchant_ava` | 20 / 10 | default | steady | — |
| `brother_ash` | 25 / −70 | default | steady | ashen_circle (initiate, 10) |

**Actions**

| id | Alignment | Standing |
| --- | --- | --- |
| `help_stranger` | good +4 | target +10 |
| `steal` | law −5, good −3 | target −20, target's factions −10 |
| `report_crime` | law +4, good +1 | city_watch +5 |
| `donate_to_temple` | good +3 | temple +10 |
| `extort` | law −2, good −6 | target −30, target's factions −15 |
| `murder` | law −10, good −15 | target −100, target's factions −40 |

`murder` also scales by its target (§5.4): `by_target.good = [[-100, 0.2], [0, 1.0], [100, 1.5]]` and `by_target.relation = [[-100, 0.5], [-50, 0.8], [0, 1.0]]`.

**Outcomes**

| id | Effects |
| --- | --- |
| `fined_by_watch` | city_watch −20; captain_hale personally −10 |
| `rescued_merchant` | good +6; merchant_ava +30; city_watch +10 |

## 14. Invariants

Each invariant has a property test (`proptest`) over random content and random command sequences. Adding an invariant means adding its test.

1. Alignment axes, standings and relations always stay within −100…+100.
2. A failed command changes nothing and emits nothing.
3. The same content and the same commands give identical events.
4. Replaying the event log from the initial state reproduces the state exactly.
5. Queries never mutate: asking for a disposition twice gives the same answer and leaves the same state.
6. No character is a member of two factions in conflict, except while a `MembershipConflict` for that pair is unresolved.
7. Every member holds a rank that exists on their faction's ladder.
8. Inertia never reverses the direction of a shift.
9. Band lookup is total and monotone: a higher score never lands in a lower band.
10. A curve's value always stays within the range of its own y values.

## 15. Crates (P-25)

```
crates/
  core/          factional-core         Fixed, Curve, ids, Tick, the event envelope. No I/O.
  reputation/    factional-reputation   the model, rules, World, commands, events, queries. No I/O.
  content/       factional-content      reads TOML from disk, validates, diagnostics, JSON Schema.
  cli/           factional-cli          the `factional` binary: repl, run, validate, schema, compare.
content/sample/  Riverhold
scenarios/       *.scenario scripts; their snapshots are in crates/cli/tests/snapshots/
```

- **Dependencies point one way:** core ← reputation ← content ← cli.
- **Future modules** (quests, combat and the rest) become sibling crates. They depend on core and talk to reputation only through §11.
- **A host-engine adapter** waits until a host is chosen (E0, X-2). That would be a Bevy plugin, or a C ABI for Godot, Unity or Unreal.

## 16. Completeness across modules (D-20)

A world loads only if it is complete in principle. This module's part is the load-time checks in §12.2. The principle reaches further than this module, and it shapes this module now.

### 16.1 What it asks of the quest module

Each quest and questline for a faction must reconcile with every other faction it affects, and with those factions' questlines, at every stage. A world whose quests contradict each other at some reachable stage doesn't load.

What "reconcile" means exactly, and how to check it without exploring every combination of stages, is for the quest module's design pass (Q0, X-4). The questions to settle there:

- **Which factions a quest affects.** Directly, through the effects of its outcomes. Indirectly, through spillover to factions related to those, and through war and membership changes.
- **What it means for a quest to conflict with another faction's questline at a stage.** For example, an outcome that harms faction B while B's questline at that stage needs the player's standing with B to have risen; or two questlines whose stages require memberships that the faction rules make impossible to hold together.
- **How to check it.** Every reachable combination of stages across all questlines explodes quickly. Stages probably need declared preconditions and effects that can be checked faction by faction, or pair by pair.

### 16.2 What it asks of this module now

A quest checker can only reconcile what it can see without running the game. So everything quests may depend on here must stay declarative:

- **Effects are data.** Actions and outcomes are bundles of declared effects (P-26), never code or scripts. Which factions an effect can touch is computable from content alone, spillover included, because spillover follows the declared relations and curve.
- **Rules are data.** Drift policies, defector and deserter tables, and conflict resolution are closed vocabularies in content, so their possible results can be enumerated.
- **Every rule decides.** A rule table always ends with a rule that decides (P-32), so no state is left without an answer.
- **Runtime changes come only through commands** (§11), so another module can know every way this module's state can change.

A feature that would make an effect's reach impossible to compute from content, such as computed effects or script hooks, conflicts with D-20. It needs the user's agreement before it's built.

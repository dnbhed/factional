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
- The product is computed exactly and rounded once (P-1, P-43): a curve's value isn't rounded on its own first. `act --explain` shows each multiplier exactly, to up to four decimals.
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

A character opts in with `inertia = "hardening"`; without it, `default_profile` applies. The curves are `law.toward_lawful`, `law.toward_chaotic`, `good.toward_good` and `good.toward_evil`, and the direction is the way the act pushes that axis. `steady` is always there, even if content leaves it out, so `default_profile` defaults to it. Outcomes and effects from other modules move alignment with inertia too, at scale 1.00.

Worked example: take a *hardening* character at good 60.00.

- They help a stranger (good +4.00). `toward_good` at 60 is 0.58, so the shift is +2.32 and they end at 62.32.
- If instead they extort someone (good −6.00), `toward_evil` at 60 is 0.70, so the shift is −4.20 and they end at 55.80.
- Sister Mira, hardening, at good 85.00 helps a stranger. `toward_good` at 85 is 0.405, so the shift is 4.00 × 0.405 = 1.62, rounded once, and she ends at 86.62.

The same mechanism expresses other shapes:

- **Fall from grace:** `toward_evil` above 1.0 at high good.
- **Slow redemption:** `toward_good` below 1.0 at low good.
- **Rigid lawful types:** both law curves low at high law.

### 5.4 Target-aware effects (D-18, P-28)

An action can scale its alignment effect by who it's done to. It has two optional curves, and both give multipliers ≥ 0:

- **`by_target.<axis>`:** a curve over the target's position on that axis. It scales the action's delta on that axis.
- **`by_target.relation`:** a curve over the most hostile relation from any of the actor's factions toward any of the target's factions, the first pair in id order on a tie. A faction and itself don't count, so with no pair of different factions (either side in none, or only a shared one) it's read at 0. It scales both axes.

Each target multiplier joins base, scale and inertia in one product, computed exactly and rounded once (P-43). Without a target, or without the curves, an act moves exactly as before. `act --explain` shows each multiplier and where it came from: the target's position, or the relation and the two factions.

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
| The player murders sister_mira (good 85) | good × 1.425 | good −21.38 (−21.375, rounded once), law −10.00 |
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

Standing starts at what `characters.toml` gives (`standing = { factions = { … }, characters = { … } }`), and changes through effects:

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

How effects apply (P-39):

- `target` and `target_factions` need the act to have a target; named parties don't.
- Effects on the same party add up first, then apply once, giving one `StandingChanged` per party that moved: factions first, then characters, each in id order.
- Each change is multiplied by `awareness(party)`, which is 1.00 under the omniscient model (§10), then stops at ±100.
- An action's `scale` changes its alignment shift only, not its standing effects.
- Leaving a faction applies its `leave_standing_change` (P-22).

**Spillover (P-13).** When standing with faction F changes, standing with every other faction G changes too, by:

```
change × spillover( relation(G → F) )
```

`standing.spillover` is a curve. By default:

- allies share up to half of your gain or loss;
- bitter enemies feel a little of the opposite;
- spillover happens once and never cascades.

Example: robbing a member of the Lantern Guild costs −10.00 with the guild. The City Watch regards the guild at −80, so you gain +1.80 with the Watch.

How it applies (P-46):

- **Every standing change with a faction spills,** whatever caused it: acts, outcomes, effects, leaving or defecting.
- **The change as applied.** A change cut short at ±100 spills only what landed. Each spill is `change × multiplier`, computed exactly and rounded once, and one that rounds to 0 is left out.
- **It adds up per faction** with any direct change, so each party still gets one `StandingChanged`. Its `spilled` list says how much came from which faction, at what relation and multiplier.
- **Values within −1…1,** checked at load.

Riverhold: a donation to the Temple (+10.00) gives +1.00 with the Watch (it regards the Temple at +60: 0.10) and −2.40 with the Ashen Circle (−90: −0.24).

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
- A refusal says which requirement failed, with its numbers, and `promote --explain` shows every requirement of the next rank.
- `Demote` refuses on the bottom rung, and `Promote` on the top one.
- A starting membership may name its rank (`{ faction = "city_watch", rank = "captain" }`); otherwise it's the lowest rung. `JoinedFaction` carries the rung a new member starts on.
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
| **modifiers** | the sum of active modifiers from other modules (§11): those for everyone, for the observer, and for the observer's factions | those for everyone and for the faction |

Modifiers come from `AddModifier { id, observer, subject, amount, expires_at }`. The observer is everyone, a faction (which counts for its members too) or a character, and `amount` is within ±100. They last until `RemoveModifier`, or until time reaches `expires_at` (`ModifierExpired`). An id is unique per subject, and adding one that's there is refused (P-50).

Each component is clamped to ±100 (P-7). Kinship sums one part per pair of the observer's and subject's factions: a faction they share counts as `same_faction`, and any other pair as how the observer's faction regards the subject's. Faction opinion sums the subject's standing with each of a character observer's factions; a faction observer has none. Each weighted component is rounded once (P-40). The score is the weighted sum of the components, and its band is the first one whose `up_to` is at or above the score:

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
- When a band changes, it emits `DispositionBandChanged { observer, subject, from, to, score }`, after the command's own events.
- The observers are every faction, then every other character, each in id order.
- `disposition.hysteresis` (default 0) is a margin. Leaving a band means going above its upper edge by more than the margin, or to its lower edge less the margin or below. It stops a score hovering on a boundary from flickering between bands.
- `Watch` records every observer's band in its `Watched` event, and each band change updates it, so replaying events restores the bands without running rules (P-15).

Worked example: watch the player after the two thefts of §8.2. The fine then emits two band changes, after its own three events: the Watch turns unfriendly at −27.24 (affinity −7.24 at 80.26, standing −20.00), then Captain Hale at −29.10. With `hysteresis = 5.0` it emits neither. A third theft then turns Merchant Ava unfriendly at −43.18 and Hale at −30.90.

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
- is let go by each enemy faction they're in, and taken by this one (§9.2).

A refusal lists every failing check with its numbers:

> refused: 50.04 from the Lantern Guild's ideals, tolerance is 45.00

`LeaveFaction` always succeeds for a member; leaving a faction you're not in is refused. From M3 it applies the faction's `leave_standing_change`, which defaults to 0.

A character's starting factions are listed in `characters.toml` as `memberships = [{ faction = "lantern_guild" }]`. They're members from tick 0. Loading warns about a starting member who is already outside their faction's `member_tolerance`, but loads anyway (P-37).

### 9.2 Joining an enemy of a faction you're in (D-10, P-10)

By default you can't. Designers loosen that with two ordered rule tables, evaluated for each conflicting membership:

- the **target's `defectors`** table: will it accept someone from its enemy?
- the **current faction's `deserters`** table: will it let you go, and at what cost?

How the tables work:

- Rules are checked top to bottom, and the first whose conditions all hold decides. A rule with no `when` always holds, and every table's last rule must have none, so a table always decides (P-42).
- **Conditions** come from a fixed vocabulary. "Current" is the enemy faction being left and "target" the faction being joined, in either table:
  - `rank_at_least`, `rank_below`: the member's rung in the current faction, 1 = lowest. A faction's own tables may use one of its rank ids instead, standing for that rank's rung.
  - `rank_at_least` is inclusive and `rank_below` strict; likewise the standing pairs: `_at_least` holds at the value itself, `_below` doesn't.
  - `standing_with_current_at_least`, `standing_with_current_below`
  - `standing_with_target_at_least`, `standing_with_target_below`
  - `closer_to_target`: distance to the target is less than distance to the current faction, each measured with that faction's own weights. `false` means not closer.
  - `outside_member_tolerance`: distance to the current faction is more than the member's tolerance there: their rank's `tolerance` if it's stricter than the faction's `member_tolerance`, otherwise that, the same rule as §9.3. `false` means within it.
- **Outcomes:**
  - `defectors` → `accept`, with an optional `standing_change` toward the target, or `refuse`, with a `reason`.
  - `deserters` → `release`, with an optional `standing_change` toward the current faction, or `refuse`, with a `reason`.
  - A refusal must give a reason and can't have a `standing_change`; letting someone through can't have a reason.
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

- **Both tables, for each enemy faction.** For every faction the character is in that's in conflict with the target, that faction's `deserters` table and the target's `defectors` table each decide. A faction's own table replaces the world's; with neither, the built-in one decides. `assess_join` reports every rule tried in each table, so `can-join --explain` can show them.
- **Defection.** If every table lets them through (and the other checks of §9.1 pass), the character leaves each enemy faction, in id order, then joins the target as a member of its lowest rank. The events are:
  - for each enemy faction, `LeftFaction { reason: Defected }`, then its deserters `standing_change` toward that faction as a `StandingChanged`;
  - then `JoinedFaction`, then the defectors `standing_change` toward the target. Leaving two enemy factions at once adds the two defectors changes together into one `StandingChanged` (P-42).
  - A `standing_change` of 0, or none, emits nothing. A faction's `leave_standing_change` is for leaving of one's own accord, and doesn't apply.
- **Refusal.** Otherwise nothing changes, and each table that refused is named with its rule and reason:

> vex belongs to The Lantern Guild, in conflict with The City Watch (-80.00): refused by The Lantern Guild's deserters rule 1, "Officers don't walk away."

  With no tables in content, the refusal names the built-in rule: `refused by the built-in defectors rule`.

Worked example: Vex, a fence (rank 2) of the Lantern Guild, has reformed to 35 / 10.

1. To the City Watch, Vex is 35.09 away. The tolerance is 40, so Vex is eligible.
2. To the Guild, Vex is 95.52 away. The member tolerance is 60, so Vex has drifted.
3. The Guild's `deserters` table releases Vex on `outside_member_tolerance`.
4. The Watch's `defectors` table accepts on `closer_to_target`, with −10 standing.
5. The events: `LeftFaction { reason: Defected }` from the Guild, `JoinedFaction` to the Watch as a recruit, then `StandingChanged` with the Watch, 0 → −10.00.

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

How it applies (P-47):

- **When.** After a command that moved a member's alignment, or their faction's (`SetFactionAlignment`, `ShiftFactionAlignment`, which emit `FactionAlignmentChanged`), each affected membership is reviewed in id order. That happens after the command's own events, then probation expiries, then watched band changes (§8.3). Tolerances don't change at runtime.
- **A faction without its own policy uses `membership.default_drift`,** which is built in as `flag`. `flag` remembers who is out, so it reports crossing out and back once each.
- **`demote` steps down** to the highest rung at or below theirs whose tolerance allows them, one `RankChanged` per rung. If none does, they're expelled.
- **`expel` costs `expel_standing_change`** (−20 by default), which spills like any standing change (§7.1).
- **Exactly at the tolerance is within it,** as for joining.
- **Probation** starts on drifting out (`ProbationStarted { until }`), clears if a later review finds them back (`ProbationCleared`), and runs out when time reaches `until`. Then `ProbationExpired`, followed by `then`'s events, as `demote` or `expel` would give them. Leaving ends it.
- **A faction's alignment is state.** It starts as content gives it, and distance, disposition and joining always use the current value.

Worked example: Ash, an initiate 40.00 from the Ashen Circle, does a good deed and ends 44.00 away. The Circle expels: `LeftFaction(expelled)`, the Circle −20.00, and +4.80 with the Temple, which regards the Circle at −90.

### 9.4 Relations between factions (P-12)

`relation(F → G)` runs −100…+100.

- **Direction.** It may be asymmetric (the Watch can distrust mercenaries more than they distrust it), but it's authored symmetrically by default.
- **Bands** default to enemy ≤ −50 < rival ≤ −15 < neutral ≤ 15 < friendly ≤ 50 < allied.
- **Conflict.** Two factions are in conflict when either regards the other at or below `relations.conflict_threshold` (−50).
- **Runtime changes.** Relations change through `SetRelation` and `ShiftRelation`, for wars and treaties sent by a quest module or the CLI. Each emits a `RelationChanged` for every direction that moved. Shifts stop at ±100, and a set outside ±100 is refused.
- **Content.** `relations.toml` lists `[[relation]]` entries, each `between = [a, b]` (both directions) or `from`/`to` (one), with a `value`. Every direction may be set once, and any left out is 0.
- **Enemy exclusion (D-4).** A member of a faction in conflict with another can't join it. The refusal names the faction and the more hostile of the two directions. M7 replaces this plain refusal with rule tables.
- **A war between someone's own factions** is accepted, and opens a conflict for them (below). Before M9 it was refused (P-38).

**War between your own factions (D-16).** If a relation change puts two of a character's factions in conflict:

- The engine emits `MembershipConflict` and marks both memberships as conflicted. Invariant 6 allows that state until it's resolved.
- The host or a quest resolves it with `ResolveConflict { character, keep }`. The other membership ends with `LeftFaction { reason: ConflictResolved }`.
- Optionally, `membership.conflict` sets an automatic rule instead, applied straight away or after `auto_after_ticks` if nobody has resolved it. The rule keeps the higher rank, then the higher standing, then the longer service, then the lower faction id.

How it applies (P-49):

- **When.** After a command's own events, every pair of each character's factions is checked. A pair now in conflict without an open conflict opens one (`MembershipConflict`); an open conflict whose pair is no longer in conflict ends (`MembershipConflictEnded`), and the character keeps both. Then any conflict the rule settles is settled.
- **The rule.** `conflict = { resolve = "ask" }` waits for `ResolveConflict` (the default); add `auto_after_ticks = N` to settle automatically N ticks after it opened. `resolve = "auto"` settles straight away.
- **Leaving costs the faction's `leave_standing_change`,** in one standing step that spills, whether a person or the rule settled it. `ResolveConflict` settles every open conflict between `keep` and another of their factions at once.
- **Leaving either side by any route ends the conflict.**

Worked example: Vex, a fence of the Guild, joins the Free Company, and the two fall to −60. A conflict opens. `resolve vex lantern_guild` leaves the Company: −10.00 there, which spills +0.60 to the Guild (−0.06 at −60).
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
| Drift | `ProbationStarted`, `ProbationCleared`, `ProbationExpired`, `MemberOutOfTolerance`, `MemberBackInTolerance` |
| Faction changes | `FactionAlignmentChanged`, `RelationChanged`, `MembershipConflict`, `MembershipConflictEnded` |
| Disposition | `Watched`, `Unwatched`, `DispositionBandChanged`, `ModifierAdded`, `ModifierRemoved`, `ModifierExpired` |

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

This table lists every knob: where it lives, its default, and the increment that adds it. An increment that adds a knob also adds a row here, an entry in the JSON Schema (`crates/content/src/schema.rs`, written out to `schema/`), and a value in the sample content.

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
| faction `leave_standing_change` | factions.toml | 0 | M3 |
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
- **Warnings don't stop loading.** For example: a starting member outside member tolerance or below their rank's standing, or a faction no one starts within tolerance of.
- **Editor support.** `factional schema <file>` prints a JSON Schema for each file, also kept in `schema/`, so an editor (VS Code with Even Better TOML) autocompletes and underlines mistakes as the designer types (T1, P-51). It's built from the engine's own keys, enumerations, ranges and defaults, and a test keeps it in step with the readers. `content/README.md` explains how to use it.

### 12.2 Validation (P-32)

Content is data, so its equivalent of a compile step is loading. **A world is only ever built from content that passed every check.** A running world never holds a reference to a faction, rank, profile or character that doesn't exist.

- **Errors stop loading.** Nothing is half-loaded, and the CLI keeps the world it already had.
- **Warnings don't stop loading.** `load` prints them after its summary. `factional validate <dir>` (T1) gives the same problems and warnings without starting a session, then a summary, and exits 1 if the world wouldn't load.
- **The rules live in `factional-reputation`.** `World::new` runs the checks and refuses invalid content, so a host that builds content in code, not from TOML, gets the same protection. `factional-content` turns each problem's location into `file: key.path`.
- **CI loads `content/sample` on every PR.** Broken sample content fails the build like a compile error. `docs/examples/riverhold` is loaded too, without the settings the engine doesn't read yet, which a test names (T1).
- **Each check arrives with the increment that adds the content it checks**, never later.

| Check | Kind | Added in |
| --- | --- | --- |
| Ids are lowercase letters, digits and `_`, starting with a letter | error | A1 (done) |
| Alignment axes within −100…100; `label_threshold` 0.01–100 | error | A1 (done) |
| Unknown keys, missing fields, wrong types, bad numbers | error | A1 (done), then every increment for its own fields |
| Action alignment names only `law` and `good` | error | A3 (done) |
| `inertia` and `inertia.default_profile` name a profile that exists; profiles use only `law.toward_lawful`, `law.toward_chaotic`, `good.toward_good` and `good.toward_evil`; multipliers ≥ 0 | error | A4 (done) |
| `by_target` names only `law`, `good` or `relation`; multipliers ≥ 0 | error | A5 (done) |
| Faction ids are unique across factions and characters | error | D1 (done) |
| Weights 0–1 with at least one above 0; `metric` is a known metric | error | D1 (done) |
| Bands: at least one; unique names that are ids; increasing `up_to`; only the last open-ended | error | D2 (done, disposition), M2 (done, relations) |
| `disposition.affinity` stays within ±100 | error | D2 (done) |
| `disposition.weights` names only the five components, each ≥ 0; `same_faction` within ±100 | error | M4 (done) |
| `hysteresis` ≥ 0 | error | D3 (done) |
| A membership names a faction that exists, at most once per character | error | M1 (done) |
| 0 ≤ `tolerance` ≤ `member_tolerance` | error | M1 (done) |
| A starting member outside their member tolerance | warning | M1 (done) |
| A relation names two different factions that exist; each direction is set at most once; values within ±100; `conflict_threshold` within ±100 | error | M2 (done) |
| No character starts in two factions that are in conflict (invariant 6) | error | M2 (done) |
| Standing names factions and characters that exist; values within ±100; `leave_standing_change` within ±100 | error | M3 (done) |
| A membership's rank is on that faction's ladder; rank ids unique; every ladder has a rung; rank standing requirements within ±100, tolerances ≥ 0 | error | M5 (done) |
| A starting member below their rank's standing requirement; a rank tolerance looser than the faction's | warning | M5 (done) |
| Spillover multipliers within −1…1 | error | M6 (done) |
| Rule tables use known conditions, with the right kind of value, and only their table's outcomes; a refusal has a reason and no `standing_change`; rung numbers ≥ 1; rank ids only in a faction's own tables and only its ranks; standing thresholds and `standing_change` within ±100; every table ends with a rule that always decides | error | M7 (done) |
| Drift policies are known (`ignore`, `flag`, `demote`, `expel`); `expel_standing_change` within ±100 | error | M8 (done) |
| Probation has `grace_ticks` > 0 and a `then` of `expel` or `demote` | error | M11 (done) |
| `conflict.resolve` is `ask` or `auto`; `auto_after_ticks` is a whole number ≥ 0, and only with `ask` | error | M9 (done) |
| `knowledge.model` is a known model | error | K1 |
| A faction no starting character is within joining tolerance of, naming the nearest and their distance (P-51) | warning | T1 (done) |

### 12.3 Designer workflow

- `factional validate <dir>` to check a world without starting a session (T1).
- `factional schema <file>` for a content file's JSON Schema, to get completion and checking in an editor (T1).
- `factional repl`, then `load content/sample`, to poke at a world.
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

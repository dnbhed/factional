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
| **Awareness** | How well a party knows of a piece of news, 0–1. Seeing it is 1; hearsay is less (§10). |
| **Contacts** | The characters someone passes news to, declared in content. With shared membership they make the social graph news ripples through. |
| **Perceived alignment** | What an observer believes a character's alignment to be: where they started, moved by every shift the observer has learned of (§10.3). |
| **Secret membership** | Belonging to a faction, which allows it, without anyone outside it knowing until it's exposed (§10.4). |

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

Curves are the one shape designers have to learn. In the CLI, `curve <curve> at <x>` gives any curve's value at a point, such as `curve [[0, 50], [60, 0], [200, -50]] at 70.18` → `-3.64`. `curve <knob>` prints a named knob's curve as a table, such as `curve disposition.affinity`, and `curve <knob> at <x>` evaluates it (T3).

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
| **affinity** | `disposition.affinity` curve applied to the distance, using the observer's weights and, from K3, the observer's perceived alignment of the subject (§10.3) | the same, using the faction's alignment and weights |
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
- is within tolerance (distance ≤ `tolerance`), as the faction perceives them from K3 (§10.3);
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

Drift is checked whenever the member's alignment, the faction's alignment, or a tolerance changes; from K3, whenever the faction's perceived alignment of the member changes (§10.3). Probation expiry is checked when time advances.

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

## 10. Knowledge (D-7, D-21–D-24)

By default every character and faction knows about every act immediately (`knowledge.model = "omniscient"`). `PerformAction` carries `witnesses` (everyone, these characters, or nobody), and from K1 a `witnessed` world uses them.

Phase 5 replaces that with knowledge that has to travel. It was designed in K0 and is built in five steps: who learns firsthand (K1), how news spreads (K2), judging by what you know (K3), secret membership (K4), and exposure (K5).

**The whole idea in one paragraph.** An act is news. The people who saw it learn it fully; their factions learn it through them; news then passes from person to person along declared contacts and through factions to their members, weaker at each hop and taking time at each one, until it has gone as far as the world lets it. Everyone who learns of an act updates their picture of the actor, their *perceived alignment*, by what they learned, at the strength they learned it, and anyone the act's standing effects name changes their standing by the same fraction. From K3 everyone judges by their own picture (D-21): disposition, joining, promotion and drift. A faction that allows it can have secret members, whom other factions don't know about until they're exposed (D-23, D-24).

### 10.1 Models and awareness (K1, P-55)

`knowledge.model` is one of:

| Model | Who learns of an act |
| --- | --- |
| `omniscient` | Everyone, fully and at once. The default, and exactly today's behaviour. |
| `witnessed` | Only those who learn firsthand (below). |
| `ripple` | Those who learn firsthand, then everyone the news reaches through the social graph (§10.2). |

**Awareness** is how well a party knows of a piece of news, from 0 (not at all) to 1 (as if they'd seen it). Each hop's awareness is a two-decimal value from content (§10.2), and every value it scales is rounded once (P-1).

**Who learns firsthand,** at awareness 1:

- **The witnesses.** `Everyone` means every character and faction. `These(…)` means exactly those characters, and the target only if listed: someone pickpocketed without noticing doesn't know. `Nobody` means no one.
- **The parties an act names directly,** such as the Temple in `donate_to_temple`'s `standing = { factions = { temple = 10.0 } }`. The act is addressed to them.
- **Every faction of a character who learned firsthand,** through that member. A faction knows what its best-informed member knows, at the same moment and the same strength.

**The actor** always knows the truth about themself, but isn't a source: their own deed doesn't reach their factions through them. A captain who takes a bribe unseen doesn't tell the Watch.

**Standing effects scale with awareness.** Each party an act's standing effects name (its target, the target's factions as they were when it happened, and named parties) changes by `change × awareness` when it learns of the act, rounded once and spilling as usual (§7.1). Spillover follows the faction's change, whether or not the other faction has heard of the act: it's reacting to the faction, not to the deed. A party that never learns never changes.

**Outcomes and effects** (`ApplyOutcome`, `ApplyEffects`) come from a quest or another module, which has already decided whose mind changes, so the parties they name always know. From K3 they also carry `witnesses` (default everyone), for who learns of their alignment shift.

**An act seen by everyone emits no news events,** under any model. Content that never uses witnesses behaves exactly as it does today.

### 10.2 Ripple (K2, D-22, P-56)

The **social graph** is built from content, so its reach is known before the game runs (§16):

- **Contacts.** A character lists the characters they talk to: `contacts = ["captain_hale", "sister_mira"]`. A contact works both ways, so it's declared on one side only.
- **Membership.** A faction hears from each of its members at no cost (§10.1), and passes news on to all its members as one hop.

**How news travels:**

- From each party that has just learned, the news goes one hop to each neighbour that hasn't heard it yet, arriving `knowledge.ripple.hop_ticks` later.
- **`knowledge.ripple.strength`** lists the awareness it arrives at on each hop, first hop first, and it goes no further than the last. The default, `[0.5, 0.25, 0.1]`, is three hops beyond those who saw it. Each is within 0.01–1.00 and none is stronger than the one before (P-56).
- Each party learns a piece of news once, the first time it reaches them. Since strength never rises, the first arrival is always the strongest.
- **News in flight is state.** An act some didn't see emits `NewsSent` after its own events: who has heard (those who learned firsthand, and the actor), the standing changes still due to the rest, and the first hop. It's delivered as time advances (`AdvanceTime`), oldest first, so a single long advance carries news several hops, each going on from when it arrived. Each arrival is a `NewsArrived` event, with who learned, at what awareness, and what is now on its way, so replay rebuilds the news in flight without running rules, and saves carry it (P-54).
- **Order.** Arrivals are taken by tick, then by news (the act's sequence number), then factions before characters, each in id order. Each arrival's standing changes follow its `NewsArrived` event, right after `TimeAdvanced` and before any other review.
- Once nothing more is on its way, the news is forgotten. What people learned stays in their pictures and standings.

**Worked example.** In Riverhold, Ava's contacts are Hale and Mira, and Mira's include Brother Ash. Hop ticks are 10. The player picks Captain Hale's pocket at tick 0, seen only by Merchant Ava (`steal` costs the target 20 and the target's factions 10).

1. **Tick 0.** The player's alignment moves to −5.00 / −3.00. Ava learns at 1.00, but `steal` names no standing for her. Hale doesn't know yet, so nothing else changes. On its way: Hale and Mira, at 0.50.
2. **Tick 10.** Hale and Mira learn at 0.50, and through them the Watch and the Temple. Hale's standing toward the player drops by 20 × 0.50 = 10.00 and the Watch's by 10 × 0.50 = 5.00, which spills +0.90 to the Lantern Guild (it regards the Watch at −80: −0.18) and −0.50 to the Temple (+60: 0.10). All four now picture the player at −2.50 / −1.50. On its way: Ash, at 0.25.
3. **Tick 20.** Ash learns at 0.25, and through him the Ashen Circle; they picture the player at −1.25 / −0.75. Ash's only other neighbours have heard, so the news stops. Vex, the Guild and the Free Company never hear of it.

### 10.3 Perceived alignment (K3, D-21, P-57)

Settles X-1. Every observer, character or faction, has a **perceived alignment** of each character:

```
perceived(observer, subject) = clamp( starting alignment + Σ shift × awareness, −100, +100 )
```

summed over the subject's shifts the observer has learned of, each axis rounded once per shift. A shift is the change as applied, after inertia and clamping, so a party that saw everything perceives the truth. A character's starting alignment is public: it's who they are when the world begins. A faction's alignment is always public. Everyone knows themself.

How it's kept (P-61): an act, outcome or `ApplyEffects` whose shift not everyone saw emits `ShiftWitnessed`, naming those who learned it firsthand, and under ripple the news carries the shift on, each `NewsArrived` with the shift at its hop's awareness. So a picture is computed as the truth, less the subject's shifts not everyone has heard of, plus what the observer has heard of them, each axis stopping at its ends. Replay and saves rebuild every picture from the events.

**Everyone judges by what they know** (D-21). Wherever a rule measures a character's distance to someone, it uses that someone's perceived alignment of the character:

| Rule | Whose picture |
| --- | --- |
| Affinity (§8.1) | the observer's |
| Joining's tolerance check (§9.1) | the faction being joined |
| `defectors` conditions (§9.2) | the faction being joined |
| `deserters` conditions | the faction being left |
| A rank's tolerance (§7.2) | the faction |
| Drift (§9.3) | the faction: a member is reviewed when the faction's picture of them moves, not their true alignment |

The `distance` query takes an observer, so it uses theirs too, and `distance`, `disposition` and `can-join --explain` say where the subject truly is when the observer pictures them elsewhere. `perceived <observer> <subject>` shows the working, and `map` draws each character where the faction pictures them. Labels, inertia and an action's `by_target` curves use the true alignment, since they're about the character, not anyone's view of them.

So a secretly corrupt captain keeps his rank until the Watch hears of it. With §9.3's numbers: if Brother Ash's good deed is seen only by its target, and the news never reaches a member of the Ashen Circle, the Circle still pictures him 40.00 away and keeps him. If it reaches the Circle at full strength, the Circle's review expels him then, as in §9.3.

**Under `omniscient`, everyone perceives the truth,** so this changes nothing for worlds that don't opt in.

### 10.4 Secret membership and double agents (K4, K5, D-19, D-23, D-24, P-58)

**Opting in.** A faction allows secret members with `secret_members = true` (D-23). Joining it can then be secret (`JoinFaction { …, secretly: true }`), and so can a starting membership (`{ faction = "lantern_guild", secret = true }`). Secrecy means nothing if everyone knows everything, so `secret_members` needs a `witnessed` or `ripple` world.

**Who knows.** An open membership is known to everyone. A secret one is known to the faction, its members and the character, and to anyone it has been exposed to (below). Everyone else judges by the memberships they know of:

- **Kinship** (§8.1) counts only memberships the observer knows of. A character always knows their own, so their own secret factions shape how they regard others.
- **Joining** (§9.2) checks only the enemy memberships the joining faction knows of. A secret Guild fence can join the Watch: the Watch sees no enemy membership, so no `defectors` table applies.
- **A double agent doesn't leave.** Joining an enemy of a faction you're secretly in keeps that membership, and its `deserters` table doesn't apply: you're its agent. Joining secretly never leaves anyone either. The joining faction still applies its `defectors` table to the enemy memberships it knows of.
- **War** (§9.4) opens a `MembershipConflict` only between memberships each side knows of. Invariant 6 allows the rest.

**Exposure** (K5, P-63). `Expose { character, faction, witnesses }` reveals a secret membership to the witnesses. It's news like an act: their factions learn through them, and under `ripple` it spreads (learning a membership is all or nothing, so any arrival counts). `MembershipExposed` records who learned; exposed to everyone, the membership is open from then on. Only a secret membership can be exposed. `assess_exposure` gives how each faction that learns at once would decide, and `expose … --explain` shows it.

**When a faction learns that one of its members is secretly in a faction it's in conflict with** (either direction at or below `conflict_threshold`), its `exposed` rule table decides (D-24). It works like `defectors` and `deserters` (§9.2): the same conditions, with "current" the faction that found out and "target" the secret faction, judged by the current faction's picture. Outcomes, each with an optional `standing_change` toward the current faction:

- `keep`: they stay, and the two memberships are now known to each other, so a `MembershipConflict` opens (§9.4) for them, a quest or the conflict rule to settle;
- `demote`: down one rung, as drift's `demote` does, and the conflict opens as for `keep`; on the lowest rung, expelled instead;
- `expel`: `LeftFaction { reason: Exposed }`.

A faction's own table replaces the world's `membership.exposed`. Built in: `expel`, costing the faction's `expel_standing_change`. Leaving a secret membership stays secret.

**Worked example** (the `exposure` scenario). Riverhold's `membership.exposed` keeps a member with standing 60 or more at −30, and otherwise expels at −40. The player, at −20 / −12, is openly in the Free Company with Merchant Ava, and secretly in the Lantern Guild, which goes to war with the Company (−60). `expose player lantern_guild --seen-by merchant_ava`: Ava learns, and through her the Company. The player's standing with it is 0, so the second rule decides: `LeftFaction { reason: Exposed }` from the Company, then −40.00 with it, which spills +2.40 to the Guild (−60: −0.06). The news goes on from Ava to Hale and Mira at 0.50, who only learn: the player isn't in a faction of theirs.

### 10.5 Memory in a large world (P-57)

X-1 also asked what this costs. Nothing is stored per observer for what everyone knows, and nothing per act once its news has stopped:

- **Pictures** are kept as two stores beside the true alignment: for each character, the sum of their shifts not everyone has heard of; and for each observer–subject pair where the observer has heard some of those, what it heard. Perceived = truth − hidden + heard. That's the same as the starting alignment plus a public offset plus a private one, and costs the same (P-61).
- So under `omniscient`, or with every act seen by everyone, both stores stay empty. Otherwise the per-pair store grows with the reach of unwitnessed acts, which the strength list bounds: three hops by default. Entries that come back to zero are dropped.
- **News in flight** holds the act's facts (actor, target, shift, the standing changes it carries) and who has heard, and is dropped when it stops.
- **Secret memberships** keep the set of outsiders each has been exposed to.

If private offsets grow too large for a host, letting them fade back to the public picture over time is the natural next step. It's not planned until a host shows the need (E1).

### 10.6 What it keeps

- **Completeness (D-20).** The graph and the strength list are content, so who can ever hear of what is computable at load time (§16.2). Every `exposed` table ends with a rule that decides.
- **Determinism.** Breadth-first in a fixed order, with exact fractions and one rounding per value.
- **Replay and saves.** News, pictures and exposures change only through events, so replaying the event log rebuilds them, and a save restores them (P-15, P-54).

## 11. API for other modules (D-15, P-14, P-15)

The module exposes commands, events and queries, and nothing else. Other modules never reach into its state, and it never calls into theirs.

### 11.1 Commands (in)

| Command | Introduced | Typical sender |
| --- | --- | --- |
| `AdvanceTime { ticks }` | A2 | the host's game loop |
| `PerformAction { actor, action, target?, scale, witnesses }` | A3 | combat, dialogue, world interaction, AI |
| `JoinFaction`, `LeaveFaction` | M1 | dialogue, the player's own choice (never a quest's effect, D-30) |
| `SetRelation`, `ShiftRelation` | M2 | quests, world events |
| `ApplyOutcome { outcome, actor }`, `ApplyEffects { source, effects }` | M3 | quests and missions |
| `Promote`, `Demote` | M5 | dialogue, the host |
| `SetFactionAlignment`, `ShiftFactionAlignment` | M8 | world events, quests |
| `ResolveConflict { character, keep }` | M9 | dialogue, the player's own choice |
| `AddModifier`, `RemoveModifier` | M10 | status, characteristics |
| `Watch`, `Unwatch` | D3 | the host |
| `JoinFaction { …, secretly }` | K4 | dialogue, the player's own choice |
| `Expose { character, faction, witnesses }` | K5 | dialogue, quests, stealth |
| `witnesses` on `ApplyOutcome` and `ApplyEffects` | K3 | quests |
| `relations` in an outcome's or `ApplyEffects`' effects: shifts between factions, after the standing changes (D-30) | Q2 | quests |

### 11.2 Events (out)

`execute` returns the events it produced, and the host forwards them to whoever cares. Each event carries a sequence number, the tick, and absolute before and after values.

| Area | Events |
| --- | --- |
| Time and actions | `TimeAdvanced`, `ActionPerformed`, `AlignmentChanged` |
| Standing and membership | `StandingChanged`, `JoinedFaction`, `LeftFaction { Voluntary \| Defected \| Expelled \| ConflictResolved }`, `RankChanged` |
| Drift | `ProbationStarted`, `ProbationCleared`, `ProbationExpired`, `MemberOutOfTolerance`, `MemberBackInTolerance` |
| Faction changes | `FactionAlignmentChanged`, `RelationChanged`, `MembershipConflict`, `MembershipConflictEnded` |
| Disposition | `Watched`, `Unwatched`, `DispositionBandChanged`, `ModifierAdded`, `ModifierRemoved`, `ModifierExpired` |
| Knowledge | `NewsSent`, `NewsArrived` (K2), `ShiftWitnessed` (K3), `JoinedFaction { secret }` (K4), `MembershipExposed` and `LeftFaction { Exposed }` (K5) |

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
- `news`: each piece of news on its way, with who has heard, its next hop and the standing changes still due (K2)
- `perceived`: an observer's perceived alignment of a character, with the shifts it's made of (K3)
- `knows_membership`: whether an observer knows of a membership (K4)
- `assess_join_secretly`: whether a secret join would be accepted, asking only the joining faction's `defectors` table (K4)
- `assess_exposure`: how each faction that would learn of a secret membership at once would decide (K5)

`World::replay(content, events)` rebuilds a world from its event log without running any rules: what saves are built on. `saved_journal()` gives each journal entry's command and how many events it produced, and `World::restore(content, journal, events)` rebuilds a world and its journal from them (T4, P-54).

### 11.4 How the future modules plug in

| Module | Sends | Reads or listens for |
| --- | --- | --- |
| Quests and missions | `ApplyOutcome`, `ApplyEffects`, with relation shifts among their effects; never membership or rank changes (D-30) | memberships, rank, standing and perception, for requirements; `MembershipConflict` as a story hook |
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
- The **event log** is what happened, with the resolved values. Replaying it reproduces state exactly, whatever the content says now. Saves are built on it (T4): a save holds the journal and the events, names its content directory with a fingerprint of each file, and is restored only onto that content unchanged (P-54). Since version 2 it holds the quest log's journal and events too (Q7, P-73).

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
| `knowledge.ripple.strength`, `.hop_ticks` | balance.toml | `[0.5, 0.25, 0.1]`, 1 | K2 |
| character `contacts` | characters.toml | none | K2 |
| faction `secret_members`; a starting membership's `secret` | factions.toml, characters.toml | false | K4 |
| `membership.exposed`, per-faction override | balance.toml, factions.toml | expel, at the faction's `expel_standing_change` | K5 (done) |

### 12.1 Content layout (P-18)

```
content/<world>/
  balance.toml      global rules, curves, bands, defaults, rule tables
  factions.toml     factions, ranks, per-faction overrides
  relations.toml    faction ↔ faction
  characters.toml   characters, starting alignment, memberships, standings
  actions.toml      the action catalogue
  outcomes.toml     named effect bundles (quest results and the like)
  quests.toml       quests: giver, gate, stages and choices (§17.1)
  questlines.toml   questlines: giver and steps of quests (§17.1)
```

- **Missing files are fine.** A missing file means the defaults, or none of that kind (P-31).
- **Errors stop loading.** Unknown keys are errors, with a "did you mean". References are checked across files, and values are range-checked. Each error names the file and the key path, for example: `characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)`.
- **Warnings don't stop loading.** For example: a starting member outside member tolerance or below their rank's standing, or a faction no one starts within tolerance of.
- **Editor support.** `factional schema <file>` prints a JSON Schema for each file, also kept in `schema/`, so an editor (VS Code with Even Better TOML) autocompletes and underlines mistakes as the designer types (T1, P-51). It's built from the engine's own keys, enumerations, ranges and defaults, and a test keeps it in step with the readers. `content/README.md` explains how to use it.

### 12.2 Validation (P-32)

Content is data, so its equivalent of a compile step is loading. **A world is only ever built from content that passed every check.** A running world never holds a reference to a faction, rank, profile or character that doesn't exist.

- **Errors stop loading.** Nothing is half-loaded, and the CLI keeps the world it already had.
- **Warnings don't stop loading.** `load` prints them after its summary. `factional validate <dir>` (T1) gives the same problems and warnings without starting a session, then a summary, and exits 1 if the world wouldn't load.
- **The rules live in `factional-reputation`,** and quests' in `factional-quests` (`Quests::problems`, against the reputation content). `World::new` runs the checks and refuses invalid content, so a host that builds content in code, not from TOML, gets the same protection. `factional-content` turns each problem's location into `file: key.path`.
- **CI loads `content/sample` on every PR,** quests included (Q5). Broken sample content fails the build like a compile error. `docs/examples/riverhold` is read and checked too, without any settings the engine doesn't read yet, which a test names (T1).
- **Quests are checked with everything else** (Q1 to Q4), lockouts included, and a world loads with them only if they pass (D-20). Their warnings come with the world's (Q5, P-71).
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
| No character starts in two factions that are in conflict (invariant 6), unless one membership is secret | error | M2 (done), K4 (done) |
| Standing names factions and characters that exist; values within ±100; `leave_standing_change` within ±100 | error | M3 (done) |
| A membership's rank is on that faction's ladder; rank ids unique; every ladder has a rung; rank standing requirements within ±100, tolerances ≥ 0 | error | M5 (done) |
| A starting member below their rank's standing requirement; a rank tolerance looser than the faction's | warning | M5 (done) |
| Spillover multipliers within −1…1 | error | M6 (done) |
| Rule tables use known conditions, with the right kind of value, and only their table's outcomes; a refusal has a reason and no `standing_change`; rung numbers ≥ 1; rank ids only in a faction's own tables and only its ranks; standing thresholds and `standing_change` within ±100; every table ends with a rule that always decides | error | M7 (done) |
| Drift policies are known (`ignore`, `flag`, `demote`, `expel`); `expel_standing_change` within ±100 | error | M8 (done) |
| Probation has `grace_ticks` > 0 and a `then` of `expel` or `demote` | error | M11 (done) |
| `conflict.resolve` is `ask` or `auto`; `auto_after_ticks` is a whole number ≥ 0, and only with `ask` | error | M9 (done) |
| `knowledge.model` is a known model | error | K1 (done) |
| `strength` lists at least one hop, each within 0.01–1.00 and none stronger than the one before; `hop_ticks` a whole number ≥ 1 | error | K2 (done) |
| Contacts name characters that exist, not themselves, each pair once whichever side declares it | error | K2 (done) |
| Contacts in a world whose model isn't `ripple` | warning | K2 (done) |
| `secret_members` only in a `witnessed` or `ripple` world; a secret starting membership only in a faction that allows them | error | K4 (done) |
| `exposed` tables: as for the M7 tables, with the outcomes `keep`, `demote` and `expel` | error | K5 (done) |
| A faction no starting character is within joining tolerance of, naming the nearest and their distance (P-51) | warning | T1 (done) |
| Quests and questlines: a giver is a faction or a character; outcomes, factions, ranks, parties, quests, stages and choices they name exist; standing within ±100; a quest has a stage, a stage a choice, a questline a step, a step a quest; stage ids unique in a quest, none called `end`; choice ids unique in a stage; `next` names a later stage; an `outcome` or `effects`, not both; no list names something twice; `need` no more than the step's quests; a quest in at most one questline, at one step; `leftovers` is `open` or `close` | error | Q1 (done) |
| `leftovers = "close"` on a step that needs all its quests | warning | Q1 (done) |
| Relation shifts in outcomes and quests' effects: two different factions that exist, `by` within −200…200, each direction shifted at most once; the same refuses `ApplyEffects` | error | Q2 (done) |
| Quests can be reached: every stage by some choice; no gate needing `member` or `rank_at_least` and `not_member` of one faction; `done` on a quest's own progress only where it can have happened; no quest that can never start, stage whose `done` can never happen, quest closed by its own step before it can start, or stage needing its quest over first (§17.2) | error | Q3 (done) |
| A choice's `locks` name another quest, or a stage of one, that exists, once each, written `quest` or `quest.stage` | error | Q4 (done) |
| Every lockout a choice can cause is declared in its `locks` (§17.2, §17.3) | error | Q4 (done) |
| A declared lock that can't happen | warning | Q4 (done) |

### 12.3 Designer workflow

- `factional validate <dir>` to check a world without starting a session (T1).
- `outline <dir>` in the REPL for each content file, its entries, and every problem and warning at the entry it's about; `factional-editor <dir>` shows the same in the editor (U1).
- `quests <dir> [<quest>]` in the REPL to list a world's quests and questlines, or one quest's stages and choices with what each locks, read and checked without loading it (Q1, Q4); `quests` alone lists the loaded world's (Q5).
- `can-start <character> <quest>`, `start <character> <quest>`, `choose <character> <quest> <choice> [--seen-by <id>,… | --unseen]` and `progress <character>` to play quests (Q6).
- `factional schema <file>` for a content file's JSON Schema, to get completion and checking in an editor (T1).
- `factional repl`, then `load content/sample`, to poke at a world.
- `calc 4.00 * 0.41` in the REPL to check exactly how the engine rounds a calculation.
- `curve [[0, 1.0], [100, 0.5]] at 25` in the REPL to try a curve's shape before using it; without `at`, the whole curve as a table. A knob's name, such as `disposition.affinity`, `standing.spillover`, `inertia.hardening.good.toward_good` or `murder.by_target.good`, shows the curve the world uses (T3).
- `factional run scenarios/<name>.scenario` to replay a scripted playthrough.
- `--explain` on `act`, `disposition`, `can-join` and `promote` to see the working. On `act` it includes who learns of the act and why (K1).
- `act … --seen-by <id>,…` or `--unseen` to say who saw an act; without either, everyone did (K1).
- `news` in the REPL for what's on its way: who has heard of each act, who hears next, when and how strongly, and the standing changes still due (K2).
- `join <character> <faction> --secretly` and `can-join … --secretly` for secret membership (K4); `expose <character> <faction> [--seen-by <id>,…] [--explain]` to reveal one (K5).
- `perceived <observer> <subject>` for where the observer pictures someone, and why; `outcome … --seen-by <id>,…` or `--unseen` to say who saw a quest's result (K3).
- `factional compare <scenario> --content A --against B` to see what new numbers change: each character's alignment, standings and memberships, and dispositions toward watched subjects, as `A → B`, then the first event where the runs diverge (T2, P-52).
- `reload` in the REPL to re-read the content, replay the session on it and see what changed; if the content no longer loads, or any command comes out differently, nothing changes (T2, P-52).
- `save <file>` and `restore <file>` in the REPL to keep a session and come back to it, quest progress included (T4, P-54, Q7).
- `map <faction>` to draw the alignment plane, law across and good up: the cells within the faction's tolerance, the faction, and every character, with each one's distance in a key (T3, P-53).
- `matrix [<subject>...] [--csv]` for every observer's disposition toward each subject, or with `--csv`, to get them into a spreadsheet (T3).

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
| `fenced_the_crown_jewels` | lantern_guild +30 |
| `sowed_discord` | city_watch ↔ temple −40; city_watch → ashen_circle −20 (Q2) |
| `turned_in_vex` | law +4; city_watch +15, lantern_guild −25; vex personally −40 (the Watch's oath) |
| `took_a_bribe` | law −4, good −2; lantern_guild +10; vex personally +15 (the Watch's oath) |

**Quests** (from Q5; DESIGN.md §17) are those of [the complete example](examples/riverhold/quests.toml): the Watch's career in four steps (the oath; two of three odd jobs, the third closing; optional favours; captain, needing sergeant and standing 40), Ava's lost ring, the world's long winter, and the Ashen Circle's rite. Its choices declare two lockouts (§17.3).

**Knowledge** (from K1; designed in K0)

- The model is `ripple`: news arrives at 0.50, 0.25, then 0.10, ten ticks a hop (§10.2).
- Contacts: `merchant_ava` ↔ `captain_hale`, `merchant_ava` ↔ `sister_mira`, `sister_mira` ↔ `brother_ash`.
- The Lantern Guild and the Ashen Circle allow secret members (K4). `membership.exposed` keeps a member with standing 60 or more at −30, and otherwise expels at −40 (K5). Riverhold has no starting double agent: its warring factions are so far apart that anyone in both would start outside one's member tolerance; the `secrets` and `exposure` scenarios make one in play (P-63).

## 14. Invariants

Each invariant has a property test (`proptest`) over random content and random command sequences. Adding an invariant means adding its test.

1. Alignment axes, standings and relations always stay within −100…+100.
2. A failed command changes nothing and emits nothing.
3. The same content and the same commands give identical events.
4. Replaying the event log from the initial state reproduces the state exactly.
5. Queries never mutate: asking for a disposition twice gives the same answer and leaves the same state.
6. No character is a member of two factions in conflict, except while a `MembershipConflict` for that pair is unresolved, or while one faction doesn't know of the other membership (K4).
7. Every member holds a rank that exists on their faction's ladder.
8. Inertia never reverses the direction of a shift.
9. Band lookup is total and monotone: a higher score never lands in a lower band.
10. A curve's value always stays within the range of its own y values.
11. Restoring a save gives the same world: the same state, events and journal (T4), and the same quest log (Q7).
12. Knowledge only ever hides: with every act and outcome seen by everyone and no secret memberships, `witnessed` and `ripple` give exactly the events `omniscient` does, and every awareness is within 0…1 (K1–K3).
13. News always stops: each party learns a piece of news at most once, none arrives before it's due, and none is still on its way more than `hop_ticks` × the length of `strength` after it began (K2).
14. A refused quest command changes nothing but the journals: not the quest log's progress or events, nor the world's state or events (Q6).
15. Quest progress only grows: a quest started stays started until it's finished, one finished or closed never changes, and stages reached and choices made stay so (Q6).

## 15. Crates (P-25)

```
crates/
  core/          factional-core         Fixed, Curve, ids, Tick, the event envelope. No I/O.
  reputation/    factional-reputation   the model, rules, World, commands, events, queries. No I/O.
  quests/        factional-quests       quests, questlines and their checks (§17). No I/O.
  content/       factional-content      reads TOML from disk, validates, diagnostics, JSON Schema.
  cli/           factional-cli          the `factional` binary: repl, run, validate, schema, compare.
  editor/        factional-editor       the visual editor, in egui (§18; from U1).
  bevy/          factional-bevy         the Bevy plugin (§19; from E1).
content/sample/  Riverhold
scenarios/       *.scenario scripts; their snapshots are in crates/cli/tests/snapshots/
```

- **Dependencies point one way:** core ← reputation ← quests ← content ← cli. `factional-content` reads every module's files, so it sits above them all (P-67). The editor and the Bevy plugin sit beside `cli`, at the top: nothing depends on them (D-32, D-33).
- **Other modules** (quests, then combat and the rest) are sibling crates. They talk to reputation only through §11. `factional-quests` also reads reputation's content, to check itself at load (§17, P-64).
- **The host engine is Bevy** (D-33, settling X-2): the module embeds as a Bevy plugin (§19). The core stays engine-agnostic, so nothing below the plugin knows Bevy exists.

## 16. Completeness across modules (D-20)

A world loads only if it is complete in principle. This module's part is the load-time checks in §12.2. The principle reaches further than this module, and it shapes this module now.

### 16.1 What it asks of the quest module

Each quest and questline for a faction must reconcile with every other faction it affects, and with those factions' questlines, at every stage. A world whose quests contradict each other at some reachable stage doesn't load. Quests and questlines outside the factions, given by a character or by no one, are held to the same check (D-28).

What "reconcile" means, and how loading checks it without exploring every combination of stages, was settled in Q0 (D-25 to D-29, settling X-4): §17 has the design. In short, a choice in one quest that could permanently close a stage or the gate of another must say so, and loading finds every such choice from conservative bounds, pair by pair.

### 16.2 What it asks of this module now

A quest checker can only reconcile what it can see without running the game. So everything quests may depend on here must stay declarative:

- **Effects are data.** Actions and outcomes are bundles of declared effects (P-26), never code or scripts. Which factions an effect can touch is computable from content alone, spillover included, because spillover follows the declared relations and curve.
- **Rules are data.** Drift policies, defector, deserter and exposure tables, and conflict resolution are closed vocabularies in content, so their possible results can be enumerated.
- **Who can hear of what is data.** Contacts, membership and the ripple settings are content, so the parties an act can ever reach are computable without running the game (§10.6).
- **Every rule decides.** A rule table always ends with a rule that decides (P-32), so no state is left without an answer.
- **Runtime changes come only through commands** (§11), so another module can know every way this module's state can change.

A feature that would make an effect's reach impossible to compute from content, such as computed effects or script hooks, conflicts with D-20. It needs the user's agreement before it's built.

## 17. Quests (designed in Q0; D-25 to D-31, P-64 to P-72)

The quest module is a sibling crate, `factional-quests`. It reads this module's content and sends it commands, like any other module (§11, D-15). This section is its design; Q1 onwards builds it.

### 17.1 Quests, questlines, stages and choices (D-25, D-28, D-29)

A **quest** is a list of stages. Each stage has requirements and one or more choices; each choice has effects and says which stage comes next, or that the quest ends. Choices only lead forward, so a quest is a tree of paths with no loops, and what's reachable is easy to work out.

A **questline** is an ordered list of steps: a story arc (D-29). Each step is a group of one or more quests that are open at the same time and can be done in any order. A step opens once the step before it is complete and its own requirements hold; it's complete once `need` of its quests are done. `need` defaults to all of them and may be 0, which makes the step's quests optional: the next step's requirements, such as a rank or a standing, decide when the character can move on. A one-quest step is a plain link in a chain. A quest can also stand alone, in no questline, and belongs to at most one questline.

**Gates.** A quest's own `requires` must hold to start it, and a stage's to reach it; a step's `requires` are added to the gate of every quest in it. They use the same vocabulary, so a faction's questline gates its later work by rank or standing in the faction.

**Leftovers.** When a step needs fewer than all its quests, the designer chooses what happens to the rest once the character moves on: `leftovers = "open"` (the default) keeps them available; `"close"` shuts the ones not yet started when the character starts a quest of any later step (P-69). Closing is written on the step, so it's a declared lockout of those quests (§17.2).

**Who they belong to** (D-28). A quest or questline has an optional `giver`: a faction (the Watch's career), a character (an NPC's personal errand), or no one, left out (the world's own quests). A quest in a questline belongs to the questline's giver unless it names its own. Whoever owns them, every quest is reconciled with every other (§17.2): ownership says whose story it is, not which rules apply.

```toml
# quests.toml
[watch_oath]
name = "The Watch's Oath"
giver = "city_watch"
requires = { not_member = ["lantern_guild"] }

[[watch_oath.stages]]
id = "patrol"
requires = { standing = { city_watch = 0.0 } }
choices = [
  { id = "report", outcome = "turned_in_vex", next = "oath" },
  { id = "look_away", outcome = "took_a_bribe", next = "end" },
]

[[watch_oath.stages]]
id = "oath"
requires = { standing = { city_watch = 10.0 } }
choices = [{ id = "swear", effects = { alignment = { law = 5.0 } }, next = "end" }]

[lost_ring]
name = "Ava's Lost Ring"
giver = "merchant_ava"
# ...

[the_long_winter]
name = "The Long Winter"
# no giver: the world's own

# questlines.toml
[watch_career]
name = "A Life in the Watch"
giver = "city_watch"

[[watch_career.steps]]
quests = ["watch_oath"]

# odd jobs, in any order: two of the three, and the third goes
[[watch_career.steps]]
quests = ["night_patrol", "dock_inspection", "smugglers_cove"]
need = 2
leftovers = "close"

# favours, done or not: they earn standing, and rank decides when to move on
[[watch_career.steps]]
quests = ["harbour_errands", "lost_dog"]
need = 0

[[watch_career.steps]]
requires = { rank_at_least = { city_watch = "sergeant" }, standing = { city_watch = 40.0 } }
quests = ["watch_captain"]
```

- **Requirements** come from a closed vocabulary, all about the character doing the quest: `standing` (at least, with a faction or character), `member` and `not_member`, `rank_at_least` in a faction, `within_tolerance` of a faction (as it perceives them, §10.3), and `done` (another quest finished, or one of its stages or choices: `quest`, `quest.stage` or `quest.stage.choice`). A questline's order is added for the designer: each quest's gate gets its step's `requires`, and a requirement that the step before is complete (`need` of its quests done).
- **Effects** are an outcome from `outcomes.toml`, or the same kinds inline: the character's alignment and standing, and `relations`, shifts in how factions regard each other (Q2). All are sent to this module as commands. Nothing is computed or scripted (§16.2). **Quests never change memberships or ranks** (D-30): joining, leaving and promotion are always the character's own actions, which a quest's requirements can only wait for.
- **Locks** are a choice's declaration of what it can shut off for good in other quests: `locks = ["watch_captain", "watch_captain.command"]`, a quest for its gate or `quest.stage` for a stage (§17.2, Q4).
- **The quest module keeps who has done what:** each character's progress, as its own events. This module never sees quests, only the commands they send.

### 17.2 Reconciling: every lockout is declared (D-26)

A choice **locks out** a stage of another quest, or its gate, if, in the worst case, its effects can make one of those requirements false for good: false, and nothing else in the content can make it true again. This holds between any two quests, in a questline or not, whoever gives them. Choices with consequences are allowed, such as joining the Watch closing the Guild's story. Accidental ones aren't: every lockout a choice can cause must be declared on it, as `locks = ["guild_heist.vault"]` for a stage or `locks = ["guild_heist"]` for the quest's gate (its step's requirements included), or the world doesn't load. A declared lock that can't actually happen is a warning, so stale declarations get noticed. A step's `leftovers = "close"` is the other way to lock a quest out, and it's declared where it's written.

What counts as for good, requirement by requirement (P-64):

| Requirement | Made false by a choice that… | For good unless… |
| --- | --- | --- |
| `standing` with a party | lowers it (directly, or by spillover) below the threshold | some action, which can be repeated, raises it directly (P-70: a spill can't be counted on, since nothing spills once standing with its source is at 100) |
| `member` of a faction | shifts alignment along an axis the faction weighs, under a drift policy that demotes (expelling from the lowest rank) or expels; or starts a war between it and another faction | never undone: rejoining depends on too much to prove. Under `probation`, drift is undone if some action moves each axis back, as for `within_tolerance` (P-70) |
| `rank_at_least` | anything that can end the membership: drift demotes as well as expels | never undone, but probation as for `member` |
| `within_tolerance` | shifts alignment along an axis the faction weighs | some action moves that axis back, with every inertia profile's curve for that way above 0 everywhere |
| `not_member` | nothing: joining is never a quest's effect (D-30) | — |
| `done` of another quest, or the step before complete | (only that quest's own other choices, or a step's leftovers closing) | not a lockout: the requirement already names the quest or the step |

Within one quest, choices exclude each other by design and need no declaration. Every stage must still be reachable along some path of its own quest, and every step of a questline reachable from the steps before it, with `need` no more than its quests, or it's dead content and an error. A rank or standing gate counts as reachable if it's in range and names a real rung, as P-51 reasons: standing can always be raised, and promotion can always be asked for.

**How loading finds dead content** (Q3, P-69). Once every reference resolves, it checks two things.

- **Structure, within each quest:**
  - every stage is reached by a choice of an earlier one;
  - no gate needs both `member` (or a `rank_at_least`) and `not_member` of a faction. A quest's start gate is its own `requires` and its step's;
  - `done` on the quest's own progress is only where it can have happened: never to start it, and at a stage, only for an earlier stage or a choice that leads there.
- **Dependencies, once the structure is sound,** through a generous run. The run works out what the `done` requirements and the questlines' order allow, taking every other requirement as one the character can meet and ignoring timing. So anything it can't reach can never be reached, and each error is certain. It reports:
  - **A quest that can never start,** naming the `done` it waits on or the step before it that can never be complete.
  - **A stage whose `done` can never happen.**
  - **A quest its own step's leftovers close before it can start.** That's the same run with the questline's later steps shut.
  - **A stage that needs something only possible once its quest is over.** That's the same run with the quest held at the stages that lead there.

### 17.3 Checking it: conservative bounds (D-27, P-65, P-70)

Loading never plays the game out. Once per world it works out what the character can always undo, and how far each relation can go:

- **Raisable standing:** the parties some action raises directly: a named faction or character, or any character an action with a positive `target` effect is done to.
- **Movable axes:** each way an axis can go (toward lawful, chaotic, good or evil) that some action moves it, with every inertia profile's curve for that way above 0 everywhere, so repeating it always gets there.
- **Relation reach:** each direction between two factions, from its starting value, moved by every quest choice's shift of it once, and all the way to ±100 if any outcome shifts it, since outcomes can be applied any number of times.

Then, for each choice, its bounds from content alone:

- **Standing:** the parties whose standing it can lower: directly, or by one hop of spillover anywhere over the reach of how the receiving faction regards the source. The curve is straight between its points, so its ends and its points within that reach are enough.
- **Alignment:** the ways it moves each axis. Where the character stands is unknown, so any move can take them out of a faction's tolerance along an axis the faction weighs.
- **Wars:** each direction its relation shifts can take to the conflict threshold or below, from above it with the other direction above it too, counting every other choice's shifts but not its own twice.

Then each choice is checked against the gate (its step's requirements included) and each stage of every other quest, requirement by requirement, as §17.2's table says: O(choices × stages × requirements), with no combinations of stages. The first lockout found for a gate or stage is reported at the choice. A quest certainly finished before the choice can be made isn't checked: one its gate, its step or its stage needs done, every quest of an earlier step that needs all of them, and, in turn, those finished before each of these started. The bounds over-estimate, so the check may report a lockout that couldn't really happen; the designer declares it, and that's the price of a check that always finishes.

**Worked example** (Riverhold's complete example). The Ashen Circle's `circle_rite` has a choice `set_them_at_war` that shifts the Temple and the Watch by −120. They start at 60, and `sowed_discord`, an outcome, can take them to −100; −100 − 120 stops at −100, at or below the conflict threshold of −50. A character in both could lose either membership in the war that opens, so it locks out `watch_captain`, whose step needs the rank of sergeant in the Watch, and `watch_captain.command`, which needs membership of the Watch. Undeclared, loading reports:

> quests.toml: circle_rite.stages[0].choices[1]: may lock out watch_captain: it can start a war between city_watch and temple, ending the sergeant rank in city_watch that its gate needs; declare it in locks

Sharing in `the_long_winter` gives the Temple 10. The Circle regards the Temple at −90, where the spillover curve gives −0.30 + 10 / 50 × 0.30 = −0.24, so 10 × −0.24 = −2.40 spills to the Circle; no action raises standing with the Circle, so sharing locks out the rite, whose gate needs standing 10 with it. The example declares both.

By contrast, a choice taking 40 from the Watch doesn't lock out `watch_oath.oath` (Watch standing 10): `report_crime`, an action, raises standing with the Watch again, so the loss isn't for good. Nor does any choice lock out membership of the Watch by drift: it puts members on probation, and for every way each axis moves some action moves it back. The Temple demotes at once, so a stage needing membership of the Temple would be locked out by every choice that moves alignment; that's the cost of a drift policy with no grace.

### 17.4 Playing quests (Q6, D-31, P-72)

The quest log keeps each character's progress, and changes only by its two commands, by applying the events they produce. Any character can play a quest; the player is an ordinary one.

| Command | Accepted when | Events |
| --- | --- | --- |
| `StartQuest { character, quest }` | the quest isn't started, finished or closed for them; every step before its own is complete; its gate holds, its own requirements and its step's | `QuestStarted`, `StageReached` at its first stage, then `QuestClosed` for each earlier step's leftover not yet started, where they close |
| `MakeChoice { character, quest, choice, witnesses }` | the quest is under way; the choice is at the stage they're at, and that stage's requirements hold | `ChoiceMade`, the world's events for its effects, then `StageReached` at the next stage or `QuestFinished` |

- **A step is complete once `need` of its quests are finished,** and open once every step before it is complete. Started isn't done.
- **Reaching a stage needs nothing** (D-31): a choice leading to a stage whose requirements don't hold yet still reaches it, and the character waits there until they do. So `done = ["quest.stage"]` means reached.
- **Effects go to the world in the same command,** as `ApplyOutcome`, or `ApplyEffects` with the source `quest:<quest>.<stage>.<choice>`, with the choice's witnesses. The world's command is checked last; if it refuses, the choice isn't made.
- **Requirements are judged on the world as it is:** standing with the faction or character; membership, secret or not; rank on the faction's ladder; `within_tolerance`, by the faction's picture (§10.3), within its member tolerance; `done` from the log.
- **Explanations come from the engine.** `assess_start` lists every reason a quest can't start: its progress, the earliest step before it that isn't complete with how many of how many are done, then each requirement that doesn't hold, with what it needs and what the character has. A refused choice says the same of its stage.
- **Saves and `reload` carry quest progress** (Q7, P-73). The quest log keeps a journal: each command, accepted or not, with where it came in the world's journal and whether it sent the world a command. A save holds that journal and the quest events; restoring replays the events without running rules, once they're checked to fit. `reload` re-runs each quest command where it came among the world's, skipping the world commands it sent, and stops, changing nothing, if any command comes out differently; it reports changed progress, such as `player's watch_oath: at oath → finished`.

### 17.5 What's built when

| Increment | Builds |
| --- | --- |
| Q1 (done) | `factional-quests`, `quests.toml` and `questlines.toml`: quests with their givers, gates, stages, choices and requirements, and questlines of steps (`quests`, `need`, `requires`, `leftovers`), with every reference and range checked; the CLI lists them |
| Q2 (done) | Relation effects, as outcomes and inline effects (D-30) |
| Q3 (done) | Reachability: no dead stages in a quest, no unreachable step in a questline, no gate that can never hold, no quest that can never start |
| Q4 (done) | The bounds and the lockout check against stages and gates, with `locks` declarations and stale-lock warnings |
| Q5 (done) | Worlds with quests load: the gate comes off, the session keeps the quests, saves fingerprint the quest files, and Riverhold's quests join `content/sample` |
| Q6 (done) | Playing quests: starting them, making choices and progress through quests and the steps of questlines, leftovers closing, as commands and events, in the CLI (§17.4) |
| Q7 (done) | Saves hold the quest log, and `reload` replays it |

## 18. The editor (designed in U0; D-32, P-74)

A visual editor for content, in egui (eframe), all Rust (D-32, settling X-5). It's a tool beside the code, for the designer who writes the content; it reads and writes the same TOML files, so hand edits, the CLI and the editor stay interchangeable.

- **A crate beside `cli`, `factional-editor`,** depending on `content`, `quests` and `reputation`. Nothing depends on it. Desktop first, through eframe; a browser build through wasm can follow, once file access there is worked out.
- **It never writes TOML itself.** `factional-content` gains a format-preserving writer (`toml_edit`): set a key, add or remove a table or a list entry, keeping comments, order and layout. The editor asks it for each change, so every write goes through one place that tests can pin down.
- **It never checks anything itself.** After every change it validates through the loader (`parse_content`, P-32) and shows each `Diagnostic` at its field, by its file and key, with the "did you mean" the loader gives. Quests' reach and lockout problems show on the graph's nodes. Warnings show beside errors, as in `validate`.
- **It never re-implements a rule** (D-20, P-24). Previews, such as a disposition matrix, the alignment map, a curve, or whether a character can start a quest, come from engine queries on a world built from the content as it stands, and show the engine's own working.
- **Panels:** a browser of every file's entries; a form for each entry, with the schema's ranges and enumerations; a graph of each questline's steps and each quest's stages and choices (`egui-snarl`), with `locks` drawn as edges; the diagnostics; the previews.
- **A model under the UI.** Opening a directory, editing a field, undoing and saving are plain Rust on the editor's state, tested with `cargo test` and mutation testing like everything else. The egui layer stays thin: it draws the state and turns clicks into model calls, and is tested headless through AccessKit (`egui_kittest`). Floats in layout are egui's own; content numbers are edited as text and read as `Fixed`.
- **Editing goes through the writer** (U2, P-77): `set_value` changes one value in a file's text, read as the kind already there (text, a number, or `true` or `false`), and nothing else; `entry_fields` lists an entry's values at any depth. The editor keeps each file's text in memory, makes the outline again after every change, and writes only on Save.
- **The outline is the content crate's** (U1, P-76): `outline(dir)` lists each content file, whether it's there, its entries (a top-level table, or one table of a top-level list, such as `relation[2]`) in id order, and the loader's problems and warnings, each at the entry its key starts with or else at its file. The editor shows it, and `outline <dir>` in the REPL prints it, so the CLI's scenarios cover what the editor shows.

| Increment | Builds |
| --- | --- |
| U1 (done) | The editor shell: open a content directory, browse every entry, and see every problem and warning at its entry, read-only |
| U2 (done) | Editing values: the format-preserving writer, each entry's values in fields, validation on every change, undo, save |
| U3 | Adding and removing keys, entries and list items, with the schema's keys to choose from |
| U4 | Quests in the editor: graphs of questlines and of quests' stages and choices, `locks` as edges, reach and lockout problems on nodes |
| U5 | Previews from engine queries: the disposition matrix, the alignment map, curves, and whether a character can start a quest |

## 19. The host engine: Bevy (designed in U0; D-33, P-75)

The module embeds in games through a Bevy plugin (D-33, settling X-2). The core stays as it is: no Bevy types below the plugin, no floats in rule code, time only through `AdvanceTime`.

- **A crate beside `cli`, `factional-bevy`,** depending on `content`, `quests` and `reputation`, and on Bevy, pinned to one version. Nothing depends on it.
- **The plugin holds the world and the quest log as a resource,** loaded from a content directory at startup; a world that doesn't load stops the app with its diagnostics, as `load` would (D-20).
- **Commands in, events out.** Game systems send reputation and quest commands as Bevy messages; the plugin executes them in the order sent, once per frame, and sends each event out as a message, with refusals as messages too, so a game reacts to `StandingChanged` or `QuestFinished` like any other. Queries are read straight from the resource.
- **Time** advances only when the game says: the plugin sends `AdvanceTime` from a tick rate the game sets, or the game sends it itself.
- **Saves** are the same saves (P-54, P-73), written and read through `factional-content`; T5 adds the binary encoding a finished game wants.
- **Left to E1:** the Bevy version, how the plugin is configured, and whether the crate builds in the main workspace or its own, to keep CI's Rust gates fast.

| Increment | Builds |
| --- | --- |
| E1 | The plugin: the resource, commands and events as messages, time, and an example app |
| T5 | Binary saves, beside JSON (P-54) |


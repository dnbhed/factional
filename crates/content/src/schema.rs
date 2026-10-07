//! A JSON Schema for each content file, for editor autocomplete and checking (DESIGN.md
//! §12.3). It's built from the same keys, enumerations, ranges and defaults the readers and
//! the world's checks use, so it can't drift from what loads. Checks that need more than
//! one value at a time, such as whether a faction named elsewhere exists, are left to
//! `factional validate`.

use factional_core::Fixed;
use factional_reputation::{
    AXIS_LIMIT, Axis, Balance, ComponentKind, Condition, Consequence, DriftPolicy, Faction,
    KnowledgeModel, Metric, TableKind, TargetCurve, Toward,
};
use serde_json::{Map, Value, json};

/// The content files with a schema, by name without `.toml`, in the order `load` reads them.
pub const SCHEMA_FILES: [&str; 6] = [
    "balance",
    "factions",
    "characters",
    "actions",
    "relations",
    "outcomes",
];

/// The JSON Schema for `file`, such as `factions`; `None` if there's no such content file.
pub fn schema(file: &str) -> Option<Value> {
    let (description, body, defs) = match file {
        "balance" => (
            "World-wide rules and defaults (DESIGN.md §12). Anything left out uses its default.",
            balance(),
            vec![
                ("weights", weights()),
                ("bands", bands()),
                ("drift", drift()),
                ("rule_tables", rule_tables()),
                ("conditions", conditions()),
            ],
        ),
        "factions" => (
            "The factions, each a table keyed by its id (DESIGN.md §9). Factions and characters share one set of ids.",
            by_id("#/$defs/faction"),
            vec![
                ("faction", faction()),
                ("alignment", alignment()),
                ("weights", weights()),
                ("rank", rank()),
                ("drift", drift()),
                ("rule_tables", rule_tables()),
                ("conditions", conditions()),
            ],
        ),
        "characters" => (
            "Everyone in the world, the player included, each a table keyed by their id (DESIGN.md §13).",
            by_id("#/$defs/character"),
            vec![
                ("character", character()),
                ("alignment", alignment()),
                ("weights", weights()),
                ("standing", named_standing()),
            ],
        ),
        "actions" => (
            "What characters can do, each a table keyed by its id: the act's effect on alignment and standing (DESIGN.md §5, §7).",
            by_id("#/$defs/action"),
            vec![("action", action()), ("delta", delta())],
        ),
        "relations" => (
            "How factions regard each other, as [[relation]] tables; any direction left out is 0 (DESIGN.md §9.4).",
            object(
                [(
                    "relation",
                    list(json!({ "$ref": "#/$defs/relation" }), "The relations."),
                )],
                &[],
                None,
            ),
            vec![("relation", relation())],
        ),
        "outcomes" => (
            "Named bundles of effects, such as a quest's result, each a table keyed by its id (P-26).",
            by_id("#/$defs/outcome"),
            vec![
                ("outcome", outcome()),
                ("delta", delta()),
                ("standing", named_standing()),
            ],
        ),
        _ => return None,
    };
    let mut schema = Map::new();
    schema.insert(
        "$schema".into(),
        "https://json-schema.org/draft/2020-12/schema".into(),
    );
    schema.insert("title".into(), format!("{file}.toml").into());
    schema.insert("description".into(), description.into());
    if let Value::Object(body) = body {
        schema.extend(body);
    }
    let defs: Map<String, Value> = defs
        .into_iter()
        .map(|(name, def)| (name.to_owned(), def))
        .collect();
    schema.insert("$defs".into(), Value::Object(defs));
    Some(Value::Object(schema))
}

/// The schema for `file` as pretty-printed JSON, ending with a newline.
pub fn schema_text(file: &str) -> Option<String> {
    schema(file).map(|schema| format!("{schema:#}\n"))
}

/// A fixed-point number as JSON, exactly as written in content.
fn number(value: Fixed) -> Value {
    value
        .to_string()
        .parse::<serde_json::Number>()
        .map(Value::Number)
        .expect("a fixed-point number is valid JSON")
}

/// A number within `low..=high`, either end left open with `None`.
fn ranged(low: Option<Fixed>, high: Option<Fixed>, description: &str) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), "number".into());
    if let Some(low) = low {
        schema.insert("minimum".into(), number(low));
    }
    if let Some(high) = high {
        schema.insert("maximum".into(), number(high));
    }
    schema.insert("description".into(), description.into());
    Value::Object(schema)
}

/// A number within ±100, like an axis, a standing or a relation.
fn within_axis(description: &str) -> Value {
    ranged(Some(-AXIS_LIMIT), Some(AXIS_LIMIT), description)
}

fn at_least_zero(description: &str) -> Value {
    ranged(Some(Fixed::ZERO), None, description)
}

/// `schema` with a default, as an editor would offer it.
fn with_default(mut schema: Value, default: Value) -> Value {
    schema["default"] = default;
    schema
}

/// The pattern every id follows: a lowercase letter, then lowercase letters, digits or `_`.
const ID_PATTERN: &str = "^[a-z][a-z0-9_]*$";

fn id(description: &str) -> Value {
    json!({ "type": "string", "pattern": ID_PATTERN, "description": description })
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn one_of(keys: impl IntoIterator<Item = &'static str>, description: &str) -> Value {
    let keys: Vec<&str> = keys.into_iter().collect();
    json!({ "type": "string", "enum": keys, "description": description })
}

fn list(items: Value, description: &str) -> Value {
    json!({ "type": "array", "items": items, "description": description })
}

/// A table with exactly these keys, of which `required` must be there.
fn object<const N: usize>(
    properties: [(&str, Value); N],
    required: &[&str],
    description: Option<&str>,
) -> Value {
    let properties: Map<String, Value> = properties
        .into_iter()
        .map(|(key, schema)| (key.to_owned(), schema))
        .collect();
    let mut schema = Map::new();
    schema.insert("type".into(), "object".into());
    if let Some(description) = description {
        schema.insert("description".into(), description.into());
    }
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    schema.insert("additionalProperties".into(), false.into());
    Value::Object(schema)
}

/// A table whose keys are ids, each holding `values`.
fn id_table(values: Value, description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "propertyNames": { "pattern": ID_PATTERN },
        "additionalProperties": values,
    })
}

/// A whole file of tables keyed by id, each as `reference` describes.
fn by_id(reference: &str) -> Value {
    json!({
        "type": "object",
        "propertyNames": { "pattern": ID_PATTERN },
        "additionalProperties": { "$ref": reference },
    })
}

/// A curve: a number, or `[x, y]` points (DESIGN.md §4.2), its values within `low..=high`.
fn curve(low: Option<Fixed>, high: Option<Fixed>, description: &str) -> Value {
    let value = ranged(low, high, "y");
    json!({
        "description": format!("{description} A number, or a list of [x, y] points, straight between them and flat beyond the ends."),
        "oneOf": [
            value,
            {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "array",
                    "prefixItems": [{ "type": "number", "description": "x" }, value],
                    "minItems": 2,
                    "maxItems": 2,
                },
            },
        ],
    })
}

/// `{ law = …, good = … }`, both axes required, each within ±100.
fn alignment() -> Value {
    let axes = [Axis::Law, Axis::Good].map(|axis| {
        let description = match axis {
            Axis::Law => "Chaotic (-100) to lawful (100).",
            Axis::Good => "Evil (-100) to good (100).",
        };
        (axis.key(), within_axis(description))
    });
    object(
        axes,
        &["law", "good"],
        Some("Where they stand on the two axes."),
    )
}

/// An alignment change: either axis may be left out, which doesn't move it.
fn delta() -> Value {
    let axes = [Axis::Law, Axis::Good].map(|axis| {
        (
            axis.key(),
            ranged(
                None,
                None,
                "How far it moves this axis; left out, it doesn't.",
            ),
        )
    });
    object(axes, &[], Some("The change to alignment."))
}

fn weights() -> Value {
    let axes = [Axis::Law, Axis::Good].map(|axis| {
        (
            axis.key(),
            ranged(Some(Fixed::ZERO), Some(Fixed::ONE), "0 to 1."),
        )
    });
    object(
        axes,
        &["law", "good"],
        Some("How much each axis counts when judging distance, 0 to 1, at least one above 0."),
    )
}

/// `standing = { factions = { … }, characters = { … } }`, each value within ±100.
fn named_standing() -> Value {
    object(
        [
            (
                "factions",
                id_table(within_axis("-100 to 100."), "By faction id."),
            ),
            (
                "characters",
                id_table(within_axis("-100 to 100."), "By character id."),
            ),
        ],
        &[],
        Some("Standing, by faction or character id, -100 to 100."),
    )
}

/// A list of bands, lowest first; the last has no `up_to`.
fn bands() -> Value {
    let band = object(
        [
            ("name", id("The band's name, an id such as neutral.")),
            (
                "up_to",
                ranged(
                    None,
                    None,
                    "The highest value in the band; the last band has none.",
                ),
            ),
        ],
        &["name"],
        None,
    );
    json!({
        "type": "array",
        "minItems": 1,
        "items": band,
        "description": "Names for ranges of values, lowest first: a value lands in the first band whose up_to it doesn't exceed.",
    })
}

/// A drift setting: `{ policy = "flag" }`, or probation with its grace and consequence.
fn drift() -> Value {
    let probation = DriftPolicy::KEYS[4];
    let mut drift = object(
        [
            (
                "policy",
                one_of(
                    DriftPolicy::KEYS,
                    "What happens when a member drifts past their tolerance.",
                ),
            ),
            (
                "grace_ticks",
                json!({ "type": "integer", "minimum": 1, "description": "For probation: how long they have to come back." }),
            ),
            (
                "then",
                one_of(
                    Consequence::ALL.map(Consequence::key),
                    "For probation: what happens if they're still out once the grace has passed.",
                ),
            ),
        ],
        &["policy"],
        Some(
            "What a faction does when a member's alignment moves them past their member tolerance (DESIGN.md §9.3).",
        ),
    );
    drift["if"] = json!({ "properties": { "policy": { "const": probation } } });
    drift["then"] = json!({ "required": ["grace_ticks", "then"] });
    drift["else"] =
        json!({ "not": { "anyOf": [{ "required": ["grace_ticks"] }, { "required": ["then"] }] } });
    drift
}

/// `membership.conflict`: `{ resolve = "ask" }`, with an optional `auto_after_ticks`, or
/// `{ resolve = "auto" }`.
fn conflict() -> Value {
    let mut conflict = object(
        [
            (
                "resolve",
                one_of(
                    ["ask", "auto"],
                    "ask waits for the game to choose; auto settles straight away.",
                ),
            ),
            (
                "auto_after_ticks",
                json!({ "type": "integer", "minimum": 0, "description": "With ask: settle automatically if nobody has within this many ticks." }),
            ),
        ],
        &["resolve"],
        Some("How a war between two of someone's own factions is settled (DESIGN.md §9.4)."),
    );
    conflict["if"] = json!({ "properties": { "resolve": { "const": "auto" } } });
    conflict["then"] = json!({ "not": { "required": ["auto_after_ticks"] } });
    conflict
}

/// One condition of a rule, as `defectors` and `deserters` read it: "current" is the enemy
/// faction being left, "target" the faction being joined.
fn condition(key: &str) -> Value {
    let rank = |comparison: &str| {
        json!({
            "description": format!("Your rung in the faction you're leaving is {comparison}: 1 the lowest, or in a faction's own tables, one of its rank ids."),
            "oneOf": [{ "type": "integer", "minimum": 1 }, { "type": "string", "pattern": ID_PATTERN }],
        })
    };
    let standing = |with: &str, comparison: &str| {
        within_axis(&format!(
            "Your standing with the faction you're {with} is {comparison}, -100 to 100."
        ))
    };
    let flag = |description: &str| json!({ "type": "boolean", "description": description });
    match key {
        "rank_at_least" => rank("at least this"),
        "rank_below" => rank("below this"),
        "standing_with_current_at_least" => standing("leaving", "at least this"),
        "standing_with_current_below" => standing("leaving", "below this"),
        "standing_with_target_at_least" => standing("joining", "at least this"),
        "standing_with_target_below" => standing("joining", "below this"),
        "closer_to_target" => {
            flag("Whether you're nearer the faction you're joining than the one you're leaving.")
        }
        _ => flag("Whether you've drifted past your tolerance in the faction you're leaving."),
    }
}

/// A rule's `when`: its conditions, the same in either table.
fn conditions() -> Value {
    let conditions: Map<String, Value> = Condition::KEYS
        .into_iter()
        .map(|key| (key.to_owned(), condition(key)))
        .collect();
    json!({
        "type": "object",
        "description": "The conditions, all of which must hold; left out, the rule always applies.",
        "properties": conditions,
        "additionalProperties": false,
    })
}

/// A `defectors` or `deserters` table: `rules`, tried top to bottom.
fn rule_table(kind: TableKind) -> Value {
    const REFUSE: &str = "refuse";
    let mut rule = object(
        [
            ("when", json!({ "$ref": "#/$defs/conditions" })),
            (
                "then",
                one_of([kind.allow_key(), REFUSE], "What the rule decides."),
            ),
            (
                "standing_change",
                within_axis("When letting someone through: the standing it costs, -100 to 100."),
            ),
            ("reason", text("When refusing: why.")),
        ],
        &["then"],
        None,
    );
    rule["if"] = json!({ "properties": { "then": { "const": REFUSE } } });
    rule["then"] = json!({ "required": ["reason"], "not": { "required": ["standing_change"] } });
    rule["else"] = json!({ "not": { "required": ["reason"] } });
    let description = match kind {
        TableKind::Defectors => {
            "Does the faction joined take a member of its enemies? The last rule must have no conditions."
        }
        TableKind::Deserters => {
            "Does each enemy faction someone is in let them go? The last rule must have no conditions."
        }
    };
    object(
        [(
            "rules",
            json!({ "type": "array", "minItems": 1, "items": rule, "description": "Tried top to bottom until one whose conditions all hold." }),
        )],
        &["rules"],
        Some(description),
    )
}

fn rule_tables() -> Value {
    let tables: Map<String, Value> = TableKind::ALL
        .into_iter()
        .map(|kind| (kind.key().to_owned(), rule_table(kind)))
        .collect();
    json!({ "type": "object", "properties": tables })
}

/// The `defectors` and `deserters` keys, referring to their definitions.
fn rule_table_keys() -> [(&'static str, Value); 2] {
    TableKind::ALL.map(|kind| {
        (
            kind.key(),
            json!({ "$ref": format!("#/$defs/rule_tables/properties/{}", kind.key()) }),
        )
    })
}

fn balance() -> Value {
    let defaults = Balance::default();
    let alignment = object(
        [
            (
                "label_threshold",
                with_default(
                    ranged(
                        Some(Fixed::from_hundredths(1)),
                        Some(AXIS_LIMIT),
                        "An axis at or beyond ± this reads as lawful or chaotic, good or evil. For display only.",
                    ),
                    number(defaults.label_threshold),
                ),
            ),
            (
                "metric",
                with_default(
                    one_of(
                        Metric::ALL.map(Metric::key),
                        "How the weighted gaps on the two axes combine into one distance (DESIGN.md §6).",
                    ),
                    defaults.metric.key().into(),
                ),
            ),
            ("default_weights", json!({ "$ref": "#/$defs/weights" })),
        ],
        &[],
        Some("Labels and distance."),
    );
    let weights = ComponentKind::ALL.map(|kind| {
        (
            kind.key(),
            with_default(
                at_least_zero("How much this part of a disposition counts."),
                number(defaults.disposition_weights.get(kind)),
            ),
        )
    });
    let disposition = object(
        [
            (
                "affinity",
                curve(
                    Some(-AXIS_LIMIT),
                    Some(AXIS_LIMIT),
                    "How alignment distance turns into liking, within -100 to 100 (DESIGN.md §8.1).",
                ),
            ),
            ("bands", json!({ "$ref": "#/$defs/bands" })),
            (
                "weights",
                object(
                    weights,
                    &[],
                    Some("The weight of each part of a disposition."),
                ),
            ),
            (
                "same_faction",
                with_default(
                    within_axis("What sharing a faction counts for, in kinship."),
                    number(defaults.same_faction),
                ),
            ),
            (
                "hysteresis",
                with_default(
                    at_least_zero(
                        "How far past a band's edge a watched score must go to leave the band.",
                    ),
                    number(defaults.hysteresis),
                ),
            ),
        ],
        &[],
        Some("How characters regard each other (DESIGN.md §8)."),
    );
    let standing = object(
        [(
            "spillover",
            curve(
                Some(-Fixed::ONE),
                Some(Fixed::ONE),
                "How much of a standing change with one faction spills to another, by how that one regards the first, within -1 to 1 (DESIGN.md §7.1).",
            ),
        )],
        &[],
        Some("Standing."),
    );
    let directions = |axis: Axis| {
        let towards: Vec<(&str, Value)> = Toward::ALL
            .into_iter()
            .filter(|toward| toward.axis() == axis)
            .map(|toward| {
                (
                    toward.key(),
                    curve(
                        Some(Fixed::ZERO),
                        None,
                        "A multiplier read at the character's position before the act, at least 0; left out, 1.",
                    ),
                )
            })
            .collect();
        let [first, second]: [(&str, Value); 2] = towards.try_into().expect("two per axis");
        object([first, second], &[], None)
    };
    let profile = object(
        [Axis::Law, Axis::Good].map(|axis| (axis.key(), directions(axis))),
        &[],
        None,
    );
    let inertia = object(
        [
            (
                "default_profile",
                with_default(
                    id("The profile of a character without their own."),
                    defaults.inertia.default_profile.as_str().into(),
                ),
            ),
            (
                "profiles",
                id_table(
                    profile,
                    "The profiles, by id; steady, where every act counts in full, is always there.",
                ),
            ),
        ],
        &[],
        Some("How hard alignment is to move (DESIGN.md §5.3)."),
    );
    let [defectors, deserters] = rule_table_keys();
    let membership = object(
        [
            ("default_drift", json!({ "$ref": "#/$defs/drift" })),
            ("conflict", conflict()),
            defectors,
            deserters,
        ],
        &[],
        Some("Joining, leaving and drifting (DESIGN.md §9)."),
    );
    let relations = object(
        [
            (
                "conflict_threshold",
                with_default(
                    within_axis(
                        "Two factions are in conflict when either regards the other at or below this.",
                    ),
                    number(defaults.conflict_threshold),
                ),
            ),
            ("bands", json!({ "$ref": "#/$defs/bands" })),
        ],
        &[],
        Some("How factions regard each other (DESIGN.md §9.4)."),
    );
    let strength: Vec<Value> = defaults
        .ripple
        .strength
        .iter()
        .copied()
        .map(number)
        .collect();
    let ripple = object(
        [
            (
                "strength",
                with_default(
                    json!({
                        "type": "array",
                        "minItems": 1,
                        "items": ranged(
                            Some(Fixed::from_hundredths(1)),
                            Some(Fixed::ONE),
                            "How strongly news arrives at this hop.",
                        ),
                        "description": "How strongly news arrives at each hop beyond those who saw it, first hop first; none stronger than the one before. It goes no further than the last.",
                    }),
                    Value::Array(strength),
                ),
            ),
            (
                "hop_ticks",
                with_default(
                    json!({ "type": "integer", "minimum": 1, "description": "How many ticks each hop takes." }),
                    defaults.ripple.hop_ticks.into(),
                ),
            ),
        ],
        &[],
        Some("How news travels under the ripple model (DESIGN.md §10.2)."),
    );
    let knowledge = object(
        [
            (
                "model",
                with_default(
                    one_of(
                        KnowledgeModel::ALL.map(KnowledgeModel::key),
                        "Who learns of an act: omniscient (everyone, whoever saw it), witnessed (the witnesses, the parties it names, and their factions) or ripple (those, then onward through contacts and factions) (DESIGN.md §10).",
                    ),
                    defaults.knowledge.key().into(),
                ),
            ),
            ("ripple", ripple),
        ],
        &[],
        Some("Who learns of what (DESIGN.md §10)."),
    );
    object(
        [
            ("alignment", alignment),
            ("disposition", disposition),
            ("standing", standing),
            ("inertia", inertia),
            ("membership", membership),
            ("relations", relations),
            ("knowledge", knowledge),
        ],
        &[],
        None,
    )
}

fn rank() -> Value {
    object(
        [
            ("id", id("The rank's id, such as sergeant.")),
            (
                "requires",
                object(
                    [(
                        "standing",
                        within_axis(
                            "The standing with the faction a promotion to this rank needs.",
                        ),
                    )],
                    &[],
                    Some("What a promotion to this rank needs."),
                ),
            ),
            (
                "tolerance",
                at_least_zero(
                    "A stricter tolerance for this rank than the faction's member tolerance.",
                ),
            ),
        ],
        &["id"],
        None,
    )
}

fn faction() -> Value {
    let [defectors, deserters] = rule_table_keys();
    object(
        [
            ("name", text("The faction's name, as shown.")),
            ("alignment", json!({ "$ref": "#/$defs/alignment" })),
            ("weights", json!({ "$ref": "#/$defs/weights" })),
            (
                "tolerance",
                at_least_zero("How close to the faction's alignment someone must be to join it."),
            ),
            (
                "member_tolerance",
                at_least_zero(
                    "How far a member may drift before it matters; at least the tolerance, which it defaults to.",
                ),
            ),
            (
                "leave_standing_change",
                with_default(
                    within_axis(
                        "The change in standing with the faction when someone leaves of their own accord.",
                    ),
                    number(Fixed::ZERO),
                ),
            ),
            (
                "expel_standing_change",
                with_default(
                    within_axis("The change in standing with the faction when it expels someone."),
                    number(Faction::DEFAULT_EXPEL_STANDING_CHANGE),
                ),
            ),
            (
                "secret_members",
                with_default(
                    json!({ "type": "boolean", "description": "Whether characters may belong to it secretly, unknown to anyone outside it. Needs knowledge.model witnessed or ripple (DESIGN.md §10.4)." }),
                    false.into(),
                ),
            ),
            (
                "ranks",
                json!({
                    "type": "array",
                    "minItems": 1,
                    "items": { "$ref": "#/$defs/rank" },
                    "description": "The rank ladder, lowest first; new members start on the first rung.",
                }),
            ),
            ("drift", json!({ "$ref": "#/$defs/drift" })),
            defectors,
            deserters,
        ],
        &["name", "alignment", "tolerance", "ranks"],
        None,
    )
}

fn character() -> Value {
    let membership = object(
        [
            ("faction", id("The faction's id.")),
            ("rank", id("A rank on its ladder; left out, the lowest.")),
            (
                "secret",
                with_default(
                    json!({ "type": "boolean", "description": "Whether they start in it secretly, known only to the faction, its members and themself. The faction must allow secret members (DESIGN.md §10.4)." }),
                    false.into(),
                ),
            ),
        ],
        &["faction"],
        None,
    );
    object(
        [
            ("name", text("The character's name, as shown.")),
            ("alignment", json!({ "$ref": "#/$defs/alignment" })),
            ("weights", json!({ "$ref": "#/$defs/weights" })),
            (
                "inertia",
                id("Their inertia profile, from balance.toml; left out, the default."),
            ),
            (
                "memberships",
                list(membership, "The factions they start in."),
            ),
            ("standing", json!({ "$ref": "#/$defs/standing" })),
            (
                "contacts",
                list(
                    id("A character's id."),
                    "The characters they pass news to under the ripple model. A contact works both ways, so list it on one side only (DESIGN.md §10.2).",
                ),
            ),
        ],
        &["name", "alignment"],
        None,
    )
}

fn action() -> Value {
    let standing = object(
        [
            (
                "target",
                within_axis("The change in the target's regard for the actor."),
            ),
            (
                "target_factions",
                within_axis("The change in the regard of each of the target's factions."),
            ),
            (
                "factions",
                id_table(within_axis("-100 to 100."), "By faction id."),
            ),
            (
                "characters",
                id_table(within_axis("-100 to 100."), "By character id."),
            ),
        ],
        &[],
        Some("How the act changes others' regard for the actor."),
    );
    let by_target: Vec<(&str, Value)> = TargetCurve::ALL
        .into_iter()
        .map(|which| {
            let read_at = match which {
                TargetCurve::Law => "the target's law",
                TargetCurve::Good => "the target's good",
                TargetCurve::Relation => {
                    "the most hostile relation from the actor's factions toward the target's"
                }
            };
            (
                which.key(),
                curve(
                    Some(Fixed::ZERO),
                    None,
                    &format!("A multiplier read at {read_at}, at least 0."),
                ),
            )
        })
        .collect();
    let [law, good, relation]: [(&str, Value); 3] =
        by_target.try_into().expect("three target curves");
    object(
        [
            ("alignment", json!({ "$ref": "#/$defs/delta" })),
            ("standing", standing),
            (
                "by_target",
                object(
                    [law, good, relation],
                    &[],
                    Some("How the act's alignment change depends on its target (DESIGN.md §5.4)."),
                ),
            ),
        ],
        &[],
        None,
    )
}

fn outcome() -> Value {
    object(
        [
            ("alignment", json!({ "$ref": "#/$defs/delta" })),
            ("standing", json!({ "$ref": "#/$defs/standing" })),
        ],
        &[],
        None,
    )
}

fn relation() -> Value {
    let mut relation = object(
        [
            (
                "between",
                json!({
                    "type": "array",
                    "items": { "type": "string", "pattern": ID_PATTERN },
                    "minItems": 2,
                    "maxItems": 2,
                    "description": "Two factions that regard each other the same way.",
                }),
            ),
            ("from", id("The faction doing the regarding.")),
            ("to", id("The faction regarded.")),
            ("value", within_axis("-100 to 100.")),
        ],
        &["value"],
        Some("Either between = [a, b], or from and to."),
    );
    relation["oneOf"] = json!([
        { "required": ["between"], "not": { "anyOf": [{ "required": ["from"] }, { "required": ["to"] }] } },
        { "required": ["from", "to"], "not": { "required": ["between"] } },
    ]);
    relation
}

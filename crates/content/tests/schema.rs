//! The JSON Schema for each content file (`factional schema <file>`) stays in step with the
//! readers: every key in the sample world and the complete example is in it, and every key it
//! names is in one of the two, or a test world, which read cleanly, so the readers accept it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use factional_content::{SCHEMA_FILES, Sources, parse_quests, schema, schema_text};
use serde_json::Value;

/// Settings in `docs/examples/riverhold` that the engine doesn't read yet, by file and key
/// path, with the increment that will read them. `*` in a path stands for every table, such
/// as every character. The schema leaves them out, since loading refuses them; this list
/// says so rather than letting them through unnoticed.
const NOT_READ_YET: [(&str, &str, &str); 0] = [];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A content file as JSON, as an editor checking it against the schema would see it; an
/// empty table if the file isn't there.
fn json(dir: &str, file: &str) -> Value {
    let path = repo().join(dir).join(format!("{file}.toml"));
    let text = fs::read_to_string(&path).unwrap_or_default();
    let table: toml::Table = text.parse().expect("valid TOML");
    serde_json::to_value(table).expect("TOML converts to JSON")
}

fn the_schema(file: &str) -> Value {
    schema(file).unwrap_or_else(|| panic!("a schema for {file}"))
}

/// The subschemas `node` applies through: itself, and whatever it refers to or combines.
/// Each comes with the path it's at: `path`, or for a definition it refers to, the
/// definition's own, such as `$defs.drift`, since the same reader reads it wherever it is.
fn branches<'s>(root: &'s Value, node: &'s Value, path: &str) -> Vec<(&'s Value, String)> {
    let mut found = vec![(node, path.to_owned())];
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let pointer = reference.strip_prefix('#').expect("a local reference");
        let target = root.pointer(pointer).expect("the reference resolves");
        let at = pointer
            .trim_start_matches('/')
            .replace("/properties/", ".")
            .replace('/', ".");
        found.extend(branches(root, target, &at));
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        for each in node
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            found.extend(branches(root, each, path));
        }
    }
    for key in ["then", "else"] {
        if let Some(each) = node.get(key) {
            found.extend(branches(root, each, path));
        }
    }
    found
}

/// Where a path is, for comparing across files: a definition's paths are the same in
/// every file that has it.
fn place(file: &str, path: &str) -> String {
    if path.starts_with("$defs.") {
        path.to_owned()
    } else {
        format!("{file}: {path}")
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Every key path the schema names, with `*` for an id and `[]` for a list's items.
fn named(root: &Value, node: &Value, path: &str, out: &mut BTreeSet<String>) {
    for (branch, path) in branches(root, node, path) {
        if let Some(properties) = branch.get("properties").and_then(Value::as_object) {
            for (key, sub) in properties {
                let at = join(&path, key);
                out.insert(at.clone());
                named(root, sub, &at, out);
            }
        }
        if let Some(sub) = branch.get("additionalProperties").filter(|a| a.is_object()) {
            let at = join(&path, "*");
            out.insert(at.clone());
            named(root, sub, &at, out);
        }
        if let Some(sub) = branch.get("items") {
            named(root, sub, &format!("{path}[]"), out);
        }
    }
}

/// Walks `data` with the schema: each key path it uses goes in `found`, and each key the
/// schema doesn't name goes in `unknown`.
fn walk(
    root: &Value,
    node: &Value,
    data: &Value,
    path: &str,
    found: &mut BTreeSet<String>,
    unknown: &mut Vec<String>,
) {
    let branches = branches(root, node, path);
    match data {
        Value::Object(map) => {
            for (key, value) in map {
                let mut subs: Vec<(&Value, String)> = branches
                    .iter()
                    .filter_map(|(branch, at)| {
                        Some((branch.get("properties")?.get(key)?, join(at, key)))
                    })
                    .collect();
                if subs.is_empty() {
                    subs = branches
                        .iter()
                        .filter_map(|(branch, at)| {
                            let sub = branch.get("additionalProperties")?;
                            sub.is_object().then(|| (sub, join(at, "*")))
                        })
                        .collect();
                }
                if subs.is_empty() {
                    unknown.push(join(path, key));
                    continue;
                }
                for (sub, at) in subs {
                    found.insert(at.clone());
                    walk(root, sub, value, &at, found, unknown);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                for (sub, at) in branches
                    .iter()
                    .filter_map(|(branch, at)| Some((branch.get("items")?, at)))
                {
                    walk(root, sub, item, &format!("{at}[]"), found, unknown);
                }
            }
        }
        _ => {}
    }
}

/// The keys of `dir` that the schema names, and those it doesn't, each as `file: path`.
fn keys(dir: &str) -> (BTreeSet<String>, Vec<String>) {
    let (mut found, mut unknown) = (BTreeSet::new(), Vec::new());
    for file in SCHEMA_FILES {
        let root = the_schema(file);
        let (mut here, mut missing) = (BTreeSet::new(), Vec::new());
        walk(&root, &root, &json(dir, file), "", &mut here, &mut missing);
        found.extend(here.iter().map(|path| place(file, path)));
        unknown.extend(missing.into_iter().map(|path| format!("{file}: {path}")));
    }
    (found, unknown)
}

/// Every problem the schema finds in `data`, as `pointer: message`.
fn violations(schema: &Value, data: &Value) -> Vec<String> {
    let validator = jsonschema::validator_for(schema).expect("a valid schema");
    validator
        .iter_errors(data)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// The complete example's text, without the settings the engine doesn't read yet.
fn riverhold_as_read(file: &str) -> Value {
    let mut data = json("docs/examples/riverhold", file);
    for (_, path, _) in NOT_READ_YET.iter().filter(|(owner, ..)| *owner == file) {
        let steps: Vec<&str> = path.split('.').collect();
        assert!(remove(&mut data, &steps) > 0, "{file}: {path} is there");
    }
    data
}

/// Removes the setting at `steps` from `data`, where `*` is every table, and says how many
/// it removed.
fn remove(data: &mut Value, steps: &[&str]) -> usize {
    match steps {
        [] => 0,
        [last] => usize::from(
            data.as_object_mut()
                .and_then(|table| table.remove(*last))
                .is_some(),
        ),
        ["*", rest @ ..] => data.as_object_mut().map_or(0, |table| {
            table.values_mut().map(|value| remove(value, rest)).sum()
        }),
        [step, rest @ ..] => data.get_mut(*step).map_or(0, |value| remove(value, rest)),
    }
}

#[test]
fn there_is_a_schema_for_each_content_file() {
    assert_eq!(
        SCHEMA_FILES,
        [
            "balance",
            "factions",
            "characters",
            "actions",
            "relations",
            "outcomes",
            "quests",
            "questlines"
        ]
    );
    assert_eq!(
        schema("factions.toml"),
        None,
        "a file is named without .toml"
    );
    assert_eq!(schema("quest"), None);
}

#[test]
fn every_schema_is_valid_json_schema() {
    for file in SCHEMA_FILES {
        let schema = the_schema(file);
        if let Err(error) = jsonschema::meta::validate(&schema) {
            panic!("{file}'s schema isn't valid: {error}");
        }
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["title"], format!("{file}.toml"));
        assert_eq!(schema["type"], "object");
    }
}

/// The schema for the key at `path` under `node`, `*` standing for any id.
fn at<'s>(root: &'s Value, node: &'s Value, path: &[&str]) -> &'s Value {
    let Some((key, rest)) = path.split_first() else {
        return node;
    };
    let sub = branches(root, node, "")
        .into_iter()
        .find_map(|(branch, _)| match *key {
            "*" => branch.get("additionalProperties"),
            "[]" => branch.get("items"),
            _ => branch.get("properties")?.get(*key),
        })
        .unwrap_or_else(|| panic!("no '{key}' in the schema"));
    at(root, sub, rest)
}

#[test]
fn the_factions_schema_enumerates_the_drift_policies_and_consequences() {
    let root = the_schema("factions");
    let drift = |key: &str| -> Vec<Value> {
        at(&root, &root, &["*", "drift", key])["enum"]
            .as_array()
            .cloned()
            .expect("an enumeration")
    };
    assert_eq!(
        drift("policy"),
        ["ignore", "flag", "demote", "expel", "probation"]
    );
    assert_eq!(drift("then"), ["demote", "expel"]);
}

#[test]
fn the_balance_schema_enumerates_metrics_and_rule_outcomes() {
    let root = the_schema("balance");
    let metric = &at(&root, &root, &["alignment", "metric"])["enum"];
    assert_eq!(
        *metric,
        serde_json::json!(["euclidean", "manhattan", "chebyshev"])
    );
    let then = |table: &str| {
        at(&root, &root, &["membership", table, "rules", "[]", "then"])["enum"].clone()
    };
    assert_eq!(then("defectors"), serde_json::json!(["accept", "refuse"]));
    assert_eq!(then("deserters"), serde_json::json!(["release", "refuse"]));
}

#[test]
fn the_sample_world_matches_the_schema() {
    for file in SCHEMA_FILES {
        let data = json("content/sample", file);
        assert_eq!(
            violations(&the_schema(file), &data),
            Vec::<String>::new(),
            "{file}"
        );
    }
    let (_, unknown) = keys("content/sample");
    assert_eq!(unknown, Vec::<String>::new());
}

#[test]
fn the_complete_example_matches_the_schema_but_for_settings_not_read_yet() {
    let (_, unknown) = keys("docs/examples/riverhold");
    let unknown: BTreeSet<String> = unknown.into_iter().collect();
    let expected: BTreeSet<String> = NOT_READ_YET
        .iter()
        .map(|(file, path, _)| format!("{file}: {path}"))
        .collect();
    assert_eq!(unknown, expected, "settings the schema doesn't name");
    for file in SCHEMA_FILES {
        let data = riverhold_as_read(file);
        assert_eq!(
            violations(&the_schema(file), &data),
            Vec::<String>::new(),
            "{file}"
        );
    }
}

/// The complete example's content and quests read and check cleanly, as a world that loads.
#[test]
fn the_complete_example_reads_cleanly_but_for_settings_not_read_yet() {
    let texts: Vec<String> = SCHEMA_FILES
        .iter()
        .map(|file| {
            let data = riverhold_as_read(file);
            let table: toml::Table = serde_json::from_value(data).expect("JSON converts back");
            toml::to_string(&table).expect("TOML writes")
        })
        .collect();
    let content = parse_quests(Sources {
        balance: Some(&texts[0]),
        factions: Some(&texts[1]),
        characters: Some(&texts[2]),
        actions: Some(&texts[3]),
        relations: Some(&texts[4]),
        outcomes: Some(&texts[5]),
        quests: Some(&texts[6]),
        questlines: Some(&texts[7]),
    });
    if let Err(error) = content {
        panic!("the complete example doesn't read cleanly:\n{error}");
    }
}

/// The test worlds that load, from `crates/cli/tests/fixtures/worlds`.
fn fixture_worlds() -> Vec<String> {
    let dir = repo().join("crates/cli/tests/fixtures/worlds");
    let mut worlds: Vec<String> = fs::read_dir(&dir)
        .expect("the fixtures exist")
        .map(|entry| entry.expect("a readable entry").path())
        .filter(|path| factional_content::load_dir(path).is_ok())
        .map(|path| {
            let name = path.file_name().expect("a name").to_string_lossy();
            format!("crates/cli/tests/fixtures/worlds/{name}")
        })
        .collect();
    worlds.sort();
    worlds
}

#[test]
fn every_key_the_schema_names_is_in_a_world_that_loads() {
    let mut used = keys("docs/examples/riverhold").0;
    for dir in ["content/sample".to_owned()]
        .into_iter()
        .chain(fixture_worlds())
    {
        used.extend(keys(&dir).0);
    }
    let mut unused = BTreeSet::new();
    for file in SCHEMA_FILES {
        let root = the_schema(file);
        let mut paths = BTreeSet::new();
        named(&root, &root, "", &mut paths);
        unused.extend(
            paths
                .iter()
                .map(|path| place(file, path))
                .filter(|path| !used.contains(path)),
        );
    }
    assert_eq!(
        unused,
        BTreeSet::<String>::new(),
        "keys no world that loads uses"
    );
}

#[test]
fn the_checked_in_schemas_are_the_engines() {
    for file in SCHEMA_FILES {
        let path = repo().join(format!("schema/{file}.schema.json"));
        let checked_in = fs::read_to_string(&path).unwrap_or_default();
        let text = schema_text(file).expect("a schema");
        assert!(
            checked_in == text,
            "{} is out of date: run `cargo run -q -p factional-cli -- schema {file} > schema/{file}.schema.json`",
            path.display()
        );
    }
}

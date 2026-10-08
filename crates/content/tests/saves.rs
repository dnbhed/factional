//! Saves (T4, P-54): a session's journal and events as JSON, with the content they came from
//! named by directory and fingerprinted, restored only onto that content as it was.

use std::fs;
use std::path::{Path, PathBuf};

use factional_content::{SAVE_VERSION, load_dir_fingerprinted, restore, save};
use factional_quests::QuestLog;
use factional_reputation::{CharacterId, Command, Witnesses, World};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// A fresh copy of content/sample, at `<tmp>/<name>`.
fn sample_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a fresh directory");
    for entry in fs::read_dir(Path::new(REPO).join("content/sample")).expect("the sample") {
        let path = entry.expect("an entry").path();
        fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
    }
    dir
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("a valid id")
}

/// The copy `name`, loaded, with two thefts and a watch played; and the save's text.
fn played(name: &str) -> (World, String) {
    sample_copy(name);
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (content, quests, fingerprint) = load_dir_fingerprinted(&base.join(name)).expect("loads");
    let mut world = World::new(content).expect("a world");
    let theft = Command::PerformAction {
        actor: id("player"),
        action: factional_reputation::ActionId::new("steal").expect("valid"),
        target: Some(id("merchant_ava")),
        scale: factional_core::Fixed::ONE,
        witnesses: Witnesses::Everyone,
    };
    world.execute(theft.clone()).expect("accepted");
    world.execute(theft).expect("accepted");
    let _ = world.execute(Command::AdvanceTime { ticks: 0 });
    world
        .execute(Command::Watch {
            subject: id("player"),
        })
        .expect("accepted");
    let text = save(&world, &QuestLog::new(quests), name, &fingerprint);
    (world, text)
}

fn base() -> &'static Path {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
}

#[test]
fn a_save_restores_the_same_world() {
    let (world, text) = played("save_round_trip");
    let restored = restore(&text, base()).expect("restores");
    assert_eq!(restored.dir, "save_round_trip");
    assert_eq!(restored.world.events(), world.events());
    assert_eq!(restored.world.journal(), world.journal());
    assert_eq!(
        restored.world.alignment(&id("player")),
        world.alignment(&id("player"))
    );
}

#[test]
fn a_save_starts_with_its_format_and_version() {
    let (_, text) = played("save_header");
    assert_eq!(SAVE_VERSION, 2);
    assert!(
        text.starts_with("{\n  \"format\": \"factional-save\",\n  \"version\": 2,\n"),
        "{text}"
    );
    let json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(json["content"]["dir"], "save_header");
    // Each file's fingerprint, and null for a file that isn't there.
    let files = json["content"]["files"].as_object().expect("files");
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "actions.toml",
            "balance.toml",
            "characters.toml",
            "factions.toml",
            "outcomes.toml",
            "questlines.toml",
            "quests.toml",
            "relations.toml"
        ]
    );
    assert!(
        files["factions.toml"]
            .as_str()
            .is_some_and(|f| f.starts_with("fnv1a64:"))
    );
}

#[test]
fn restoring_refuses_a_version_it_doesnt_know_naming_both() {
    let (_, text) = played("save_version");
    let newer = text.replacen("\"version\": 2,", "\"version\": 3,", 1);
    assert_eq!(
        restore(&newer, base()).map(|_| ()).unwrap_err().to_string(),
        "this save is version 3, but this build reads versions 1 and 2"
    );
}

#[test]
fn a_version_1_save_restores_with_no_quest_progress() {
    let (world, text) = played("save_version_1");
    let mut json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let file = json.as_object_mut().expect("an object");
    assert!(
        file.remove("quests").is_some(),
        "a version 2 save holds quests"
    );
    file.insert("version".to_owned(), 1.into());
    let restored = restore(&json.to_string(), base()).expect("restores");
    assert_eq!(restored.world.events(), world.events());
}

#[test]
fn restoring_refuses_what_isnt_a_save() {
    assert_eq!(
        restore("{}", base()).map(|_| ()).unwrap_err().to_string(),
        "this isn't a Factional save"
    );
    let error = restore("not json", base())
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("this save isn't valid JSON: "), "{error}");
    let (_, text) = played("save_mangled");
    let mangled = text.replacen("\"events\": [", "\"events\": 3, \"ignored\": [", 1);
    let error = restore(&mangled, base())
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("this save can't be read: "), "{error}");
}

#[test]
fn restoring_refuses_content_that_has_changed_naming_the_files() {
    let (_, text) = played("save_changed");
    let dir = base().join("save_changed");
    let factions = dir.join("factions.toml");
    let edited = fs::read_to_string(&factions).expect("readable") + "\n# edited\n";
    fs::write(&factions, edited).expect("written");
    fs::remove_file(dir.join("relations.toml")).expect("removed");
    assert_eq!(
        restore(&text, base()).map(|_| ()).unwrap_err().to_string(),
        "the content in save_changed has changed since this save: factions.toml, relations.toml"
    );
}

#[test]
fn a_save_fingerprints_the_quest_files() {
    let (_, text) = played("save_quests_changed");
    let quests = base().join("save_quests_changed/quests.toml");
    let edited = fs::read_to_string(&quests).expect("the sample has quests") + "\n# edited\n";
    fs::write(&quests, edited).expect("written");
    assert_eq!(
        restore(&text, base()).map(|_| ()).unwrap_err().to_string(),
        "the content in save_quests_changed has changed since this save: quests.toml"
    );
}

#[test]
fn a_save_from_before_quests_reads_as_having_none() {
    let (_, text) = played("save_before_quests");
    let mut json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let files = json["content"]["files"]
        .as_object_mut()
        .expect("fingerprints");
    files.remove("quests.toml");
    files.remove("questlines.toml");
    let older = json.to_string();
    assert_eq!(
        restore(&older, base()).map(|_| ()).unwrap_err().to_string(),
        "the content in save_before_quests has changed since this save: questlines.toml, quests.toml"
    );
    let (_, text) = played("save_without_quests");
    let dir = base().join("save_without_quests");
    fs::remove_file(dir.join("quests.toml")).expect("removed");
    fs::remove_file(dir.join("questlines.toml")).expect("removed");
    let mut json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let files = json["content"]["files"]
        .as_object_mut()
        .expect("fingerprints");
    files.remove("quests.toml");
    files.remove("questlines.toml");
    assert!(restore(&json.to_string(), base()).is_ok());
}

#[test]
fn restoring_reports_content_that_no_longer_loads() {
    let (_, text) = played("save_unloadable");
    fs::remove_dir_all(base().join("save_unloadable")).expect("removed");
    let error = restore(&text, base()).map(|_| ()).unwrap_err().to_string();
    assert!(
        error.starts_with("the content in save_unloadable doesn't load: "),
        "{error}"
    );
}

#[test]
fn restoring_reports_a_journal_that_doesnt_match_its_events() {
    let (_, text) = played("save_tampered");
    let json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let mut json = json;
    json["events"].as_array_mut().expect("events").pop();
    let error = restore(&json.to_string(), base())
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("this save doesn't fit its content: the journal accounts for "),
        "{error}"
    );
}

#[test]
fn fingerprints_follow_the_fnv_1a_test_vectors() {
    assert_eq!(
        factional_content::fingerprint_of(b""),
        "fnv1a64:cbf29ce484222325"
    );
    assert_eq!(
        factional_content::fingerprint_of(b"a"),
        "fnv1a64:af63dc4c8601ec8c"
    );
    assert_eq!(
        factional_content::fingerprint_of(b"foobar"),
        "fnv1a64:85944171f73967e8"
    );
}

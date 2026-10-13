//! A session handed a world (G2b): what each command played, in order; the world given back;
//! and what works on content directories refused, so a game's world can't be replaced.

use std::path::Path;

use factional_quests::{ChoiceId, Played, QuestCommand, QuestEvent, QuestId, QuestLog, StageId};
use factional_reputation::{Change, CharacterId, World};
use factional_session::{Outcome, Session};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn riverhold() -> (World, QuestLog) {
    let (content, quests) =
        factional_content::load_quests(&Path::new(REPO).join("content/sample")).expect("loads");
    (
        World::new(content).expect("complete"),
        QuestLog::new(quests),
    )
}

fn loaded() -> Session {
    let mut session = Session::new(REPO);
    session.execute("load content/sample").expect("a command");
    session
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn run(session: &mut Session, line: &str) -> String {
    session.execute(line).expect("a command").render()
}

#[test]
fn a_world_command_plays_its_events_in_order() {
    let mut session = loaded();
    run(&mut session, "act player steal --target merchant_ava");
    let played: Vec<&Change> = session
        .played()
        .iter()
        .map(|played| match played {
            Played::World(event) => &event.payload,
            Played::Quest(event) => panic!("a quest event from a theft: {event:?}"),
        })
        .collect();
    assert_eq!(played.len(), 3, "{played:?}");
    assert!(matches!(played[0], Change::ActionPerformed { .. }));
    assert!(matches!(played[1], Change::AlignmentChanged { .. }));
    assert!(matches!(played[2], Change::StandingChanged { .. }));
}

#[test]
fn starting_a_quest_plays_it_starting_and_reaching_its_first_stage() {
    let mut session = loaded();
    run(&mut session, "start player lost_ring");
    let quest = QuestId::new("lost_ring").expect("an id");
    assert_eq!(
        session.played(),
        [
            Played::Quest(QuestEvent::QuestStarted {
                character: id("player"),
                quest: quest.clone(),
            }),
            Played::Quest(QuestEvent::StageReached {
                character: id("player"),
                quest,
                stage: StageId::new("search").expect("an id"),
            }),
        ]
    );
}

#[test]
fn a_choice_plays_exactly_what_the_quest_log_played_in_its_order() {
    let mut session = loaded();
    run(&mut session, "start player lost_ring");
    // The same, played straight on the quest log.
    let (mut world, mut log) = riverhold();
    let quest = QuestId::new("lost_ring").expect("an id");
    log.execute(
        &mut world,
        QuestCommand::StartQuest {
            character: id("player"),
            quest: quest.clone(),
        },
    )
    .expect("starts");
    let expected = log
        .execute(
            &mut world,
            QuestCommand::MakeChoice {
                character: id("player"),
                quest,
                choice: ChoiceId::new("pawn_it").expect("an id"),
                witnesses: factional_reputation::Witnesses::Everyone,
            },
        )
        .expect("chosen");
    run(&mut session, "choose player lost_ring pawn_it");
    assert!(expected.iter().any(|p| matches!(p, Played::World(_))));
    assert_eq!(session.played(), expected);
}

#[test]
fn a_command_that_changes_nothing_or_loads_plays_nothing() {
    let mut session = loaded();
    run(&mut session, "act player steal --target merchant_ava");
    run(&mut session, "characters");
    assert!(session.played().is_empty());
    run(&mut session, "act player steal --target merchant_ava");
    run(&mut session, "load content/sample");
    assert!(session.played().is_empty());
}

#[test]
fn a_handed_world_is_played_on_and_given_back() {
    let (world, quests) = riverhold();
    let mut session = Session::on(world, quests);
    let said = run(&mut session, "act player steal --target merchant_ava");
    assert!(said.starts_with("#1 at tick 0: player did steal"), "{said}");
    let (world, _) = session.into_world().expect("the world back");
    assert_eq!(world.events().len(), 3);
}

#[test]
fn handed_a_world_what_works_on_content_directories_is_refused() {
    let (world, quests) = riverhold();
    let mut session = Session::on(world, quests);
    for line in [
        "load content/sample",
        "reload",
        "save target/handed.json",
        "restore target/handed.json",
        "validate content/sample",
        "quests content/sample",
        "outline content/sample",
        "graph content/sample",
        "references content/sample",
        "form content/sample",
        "fixes content/sample",
        "keys content/sample",
        "words content/sample factions.toml temple",
    ] {
        let name = line.split_whitespace().next().expect("a word");
        assert_eq!(
            session.execute(line).expect("a command"),
            Outcome::Error(format!(
                "{name} works on a content directory, and this session was handed its world rather than loading one"
            )),
            "{line}"
        );
    }
    // Its own quests are still listed.
    assert!(run(&mut session, "quests").contains("lost_ring"));
    let (world, _) = session.into_world().expect("the world back");
    assert!(world.events().is_empty());
}

#[test]
fn handed_a_world_tab_offers_no_command_it_refuses() {
    let (world, quests) = riverhold();
    let session = Session::on(world, quests);
    assert!(session.complete("lo").is_empty());
    assert_eq!(Session::new(REPO).complete("lo"), ["load"]);
    // Every other command is offered as a loading session offers it.
    let refused = ["reload", "references", "restore"];
    let offered: Vec<String> = Session::new(REPO)
        .complete("re")
        .into_iter()
        .filter(|command| !refused.contains(&command.as_str()))
        .collect();
    assert!(offered.contains(&"relate".to_owned()), "{offered:?}");
    assert_eq!(session.complete("re"), offered);
}

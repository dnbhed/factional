//! Invariants 14 and 15 (DESIGN.md §14, Q6), and 11 for quests (Q7): random quest and world
//! commands against Riverhold's quests in `content/sample`.

use std::path::Path;

use factional_core::Fixed;
use factional_quests::{ChoiceId, QuestCommand, QuestId, QuestLog, QuestState, Record};
use factional_reputation::{ActionId, CharacterId, Command, OutcomeId, Witnesses, World};
use proptest::prelude::*;

const CHARACTERS: [&str; 3] = ["player", "vex", "captain_hale"];

/// Every choice id in the sample's quests, and one that isn't.
const CHOICES: [&str; 18] = [
    "report",
    "look_away",
    "swear",
    "by_the_book",
    "seize",
    "wave_through",
    "follow_the_lights",
    "arrest_them",
    "deliver",
    "found_it",
    "accept",
    "return_it",
    "pawn_it",
    "share",
    "hoard",
    "refuse",
    "set_them_at_war",
    "whatever",
];

fn sample() -> (World, QuestLog) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    let (content, quests) = factional_content::load_quests(&dir).expect("the sample loads");
    let world = World::new(content).expect("a valid world");
    (world, QuestLog::new(quests))
}

#[derive(Debug, Clone)]
enum Step {
    Quest(QuestCommand),
    World(Command),
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("valid id")
}

fn step(quests: Vec<String>) -> impl Strategy<Value = Step> {
    let count = quests.len();
    let starts = (0..CHARACTERS.len(), 0..=count).prop_map({
        let quests = quests.clone();
        move |(who, what)| {
            let quest = quests.get(what).map_or("nowhere", String::as_str);
            Step::Quest(QuestCommand::StartQuest {
                character: id(CHARACTERS[who]),
                quest: QuestId::new(quest).expect("valid id"),
            })
        }
    });
    let choices = (0..CHARACTERS.len(), 0..count, 0..CHOICES.len(), 0..3usize).prop_map(
        move |(who, what, choice, seen)| {
            let witnesses = match seen {
                0 => Witnesses::Everyone,
                1 => Witnesses::Nobody,
                _ => Witnesses::These([id("nobody_here")].into()),
            };
            Step::Quest(QuestCommand::MakeChoice {
                character: id(CHARACTERS[who]),
                quest: QuestId::new(&quests[what]).expect("valid id"),
                choice: ChoiceId::new(CHOICES[choice]).expect("valid id"),
                witnesses,
            })
        },
    );
    let acts = (0..CHARACTERS.len(), 0..3usize).prop_map(|(who, which)| {
        let character = id(CHARACTERS[who]);
        Step::World(match which {
            0 => Command::PerformAction {
                actor: character,
                action: ActionId::new("report_crime").expect("valid id"),
                target: None,
                scale: Fixed::ONE,
                witnesses: Witnesses::Everyone,
            },
            1 => Command::ApplyOutcome {
                outcome: OutcomeId::new("fined_by_watch").expect("valid id"),
                character,
                witnesses: Witnesses::Everyone,
            },
            _ => Command::AdvanceTime { ticks: 10 },
        })
    });
    prop_oneof![3 => starts, 4 => choices, 2 => acts]
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let (_, log) = sample();
    let quests: Vec<String> = log
        .quests()
        .quests
        .keys()
        .map(ToString::to_string)
        .collect();
    prop::collection::vec(step(quests), 0..60)
}

/// Whether `after` keeps everything `before` had: a quest started stays started, finished or
/// closed, and one finished or closed never changes; stages reached and choices made stay.
fn grew(before: &Record, after: &Record) -> bool {
    let kept = before
        .states
        .iter()
        .all(|(quest, was)| match (was, after.states.get(quest)) {
            (QuestState::Active { stage }, Some(QuestState::Active { stage: now })) => now >= stage,
            (QuestState::Active { .. }, Some(QuestState::Finished)) => true,
            (was, now) => Some(was) == now,
        });
    kept && before.reached.is_subset(&after.reached) && before.chosen.is_subset(&after.chosen)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn a_refused_quest_command_changes_nothing_and_progress_only_grows(steps in steps()) {
        let (mut world, mut log) = sample();
        for step in steps {
            let records: Vec<Record> = CHARACTERS.iter().map(|c| log.record(&id(c))).collect();
            match step {
                Step::World(command) => {
                    let _ = world.execute(command);
                }
                Step::Quest(command) => {
                    let before = (log.records().clone(), log.events().to_vec());
                    let events = world.events().to_vec();
                    if log.execute(&mut world, command).is_err() {
                        // Invariant 14: nothing changes but the journals.
                        prop_assert_eq!((log.records().clone(), log.events().to_vec()), before);
                        prop_assert_eq!(world.events(), &events[..]);
                    }
                }
            }
            // Invariant 15.
            for (character, before) in CHARACTERS.iter().zip(&records) {
                prop_assert!(grew(before, &log.record(&id(character))), "{character}");
            }
        }
    }

    #[test]
    fn restoring_a_save_gives_the_same_quest_log(steps in steps()) {
        let (mut world, mut log) = sample();
        for step in steps {
            match step {
                Step::World(command) => {
                    let _ = world.execute(command);
                }
                Step::Quest(command) => {
                    let _ = log.execute(&mut world, command);
                }
            }
        }
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (_, _, fingerprint) = factional_content::load_dir_fingerprinted(&repo.join("content/sample"))
            .expect("the sample loads");
        let text = factional_content::save(&world, &log, "content/sample", &fingerprint);
        let restored = factional_content::restore(&text, &repo).expect("restores");
        prop_assert_eq!(&restored.quests, &log);
        prop_assert_eq!(restored.world.events(), world.events());
    }
}

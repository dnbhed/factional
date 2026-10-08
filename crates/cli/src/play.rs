//! Playing quests in the REPL (Q6): `can-start`, `start`, `choose` and `progress`. The quest
//! log decides and explains; this only reads arguments and renders what it says.

use factional_quests::{
    ChoiceId, Played, QuestCommand, QuestId, QuestLog, QuestState, QuestlineId,
};
use factional_reputation::{CharacterId, Witnesses, World};

use crate::session::{Outcome, describe_event, is_flag, lines};

/// `<character> <quest>`, as ids.
fn character_and_quest(args: &str, usage: &str) -> Result<(CharacterId, QuestId), Outcome> {
    let [character, quest] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Err(Outcome::Error(usage.to_owned()));
    };
    let character =
        CharacterId::new(character).map_err(|invalid| Outcome::Error(invalid.to_string()))?;
    let quest = QuestId::new(quest).map_err(|invalid| Outcome::Error(invalid.to_string()))?;
    Ok((character, quest))
}

/// `can-start <character> <quest>`: whether they can start it now, and every reason not.
pub(crate) fn can_start(world: &World, log: &QuestLog, args: &str) -> Outcome {
    let (character, quest) = match character_and_quest(
        args,
        "can-start needs the form: can-start <character> <quest>",
    ) {
        Ok(ids) => ids,
        Err(usage) => return usage,
    };
    match log.assess_start(world, &character, &quest) {
        Ok(assessment) => Outcome::Output(assessment.to_string()),
        Err(error) => Outcome::Error(error.to_string()),
    }
}

/// `start <character> <quest>`.
pub(crate) fn start(world: &mut World, log: &mut QuestLog, args: &str) -> Outcome {
    let (character, quest) =
        match character_and_quest(args, "start needs the form: start <character> <quest>") {
            Ok(ids) => ids,
            Err(usage) => return usage,
        };
    played(log.execute(world, QuestCommand::StartQuest { character, quest }))
}

/// `choose <character> <quest> <choice> [--seen-by <id>,... | --unseen]`.
pub(crate) fn choose(world: &mut World, log: &mut QuestLog, args: &str) -> Outcome {
    const USAGE: &str = "choose needs the form: choose <character> <quest> <choice> [--seen-by <id>,... | --unseen]";
    let words: Vec<&str> = args.split_whitespace().collect();
    let (ids, seen_by) = match words[..] {
        [character, quest, choice] => ([character, quest, choice], None),
        [character, quest, choice, "--unseen"] => ([character, quest, choice], Some(None)),
        [character, quest, choice, "--seen-by", seen] if !is_flag(seen) => {
            ([character, quest, choice], Some(Some(seen)))
        }
        _ => return Outcome::Error(USAGE.to_owned()),
    };
    let witnesses = match seen_by {
        None => Witnesses::Everyone,
        Some(None) => Witnesses::Nobody,
        Some(Some(seen)) => match seen.split(',').map(CharacterId::new).collect() {
            Ok(seen) => Witnesses::These(seen),
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        },
    };
    let [character, quest, choice] = ids;
    let command = match (
        CharacterId::new(character),
        QuestId::new(quest),
        ChoiceId::new(choice),
    ) {
        (Ok(character), Ok(quest), Ok(choice)) => QuestCommand::MakeChoice {
            character,
            quest,
            choice,
            witnesses,
        },
        (Err(invalid), ..) | (_, Err(invalid), _) | (.., Err(invalid)) => {
            return Outcome::Error(invalid.to_string());
        }
    };
    played(log.execute(world, command))
}

/// What a quest command did, one line each, in order; or why it was refused.
fn played(result: Result<Vec<Played>, factional_quests::QuestError>) -> Outcome {
    match result {
        Ok(played) => Outcome::Output(lines(played.iter().map(|each| match each {
            Played::Quest(event) => event.to_string(),
            Played::World(event) => describe_event(event),
        }))),
        Err(error) => Outcome::Error(error.to_string()),
    }
}

/// `progress <character>`: each quest they've started or had closed, in id order, then how far
/// each questline those are in has opened for them.
pub(crate) fn progress(world: &World, log: &QuestLog, args: &str) -> Outcome {
    let [character] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("progress needs the form: progress <character>".to_owned());
    };
    let character = match CharacterId::new(character) {
        Ok(id) => id,
        Err(invalid) => return Outcome::Error(invalid.to_string()),
    };
    if world.character(&character).is_none() {
        let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
        return Outcome::Error(format!(
            "unknown character '{character}'{}",
            crate::session::hint(character.as_str(), ids)
        ));
    }
    let record = log.record(&character);
    if record.states.is_empty() {
        return Outcome::Output(format!("{character} hasn't started any quests"));
    }
    let quests = log.quests();
    let mut shown: Vec<String> = record
        .states
        .iter()
        .map(|(quest, state)| match state {
            QuestState::Active { stage } => {
                format!("{quest} — at {}", quests.quests[quest].stages[*stage].id)
            }
            QuestState::Finished => format!("{quest} — finished"),
            QuestState::Closed { questline, step } => format!(
                "{quest} — closed when {questline} moved on from step {}",
                step + 1
            ),
        })
        .collect();
    let lines_in: std::collections::BTreeSet<&QuestlineId> = record
        .states
        .keys()
        .filter_map(|quest| quests.place_of(quest).map(|(line, _)| line))
        .collect();
    for line in lines_in {
        let open = log.open_step(&character, line).unwrap_or_default();
        shown.push(format!(
            "{line} — up to step {} of {}",
            open + 1,
            quests.questlines[line].steps.len()
        ));
    }
    Outcome::Output(lines(shown.into_iter()))
}

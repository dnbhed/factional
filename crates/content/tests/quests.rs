//! Reading and checking quests and questlines (DESIGN.md §17.1, PLAN.md Q1), against
//! Riverhold's complete example in `docs/examples/riverhold`.

use std::fs;
use std::path::{Path, PathBuf};

use factional_content::{ContentError, Sources, parse_content, parse_quests, quest_warnings};
use factional_core::Fixed;
use factional_quests::{
    ChoiceEffects, Leftovers, Next, PartyRef, Progress, QuestId, QuestlineId, Quests, StageId,
};
use factional_reputation::{FactionId, OutcomeId, RankId, RelationEnds, RelationShift};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/riverhold")
}

/// The example's text for `file`.
fn text(file: &str) -> String {
    fs::read_to_string(example().join(file)).expect("the example has the file")
}

/// Every file of the example, with `quests.toml` and `questlines.toml` as given.
fn with(quests: &str, questlines: &str, read: impl Fn(Sources<'_>) -> Vec<String>) -> Vec<String> {
    let files = [
        "balance.toml",
        "factions.toml",
        "characters.toml",
        "actions.toml",
        "relations.toml",
        "outcomes.toml",
    ]
    .map(text);
    read(Sources {
        balance: Some(&files[0]),
        factions: Some(&files[1]),
        characters: Some(&files[2]),
        actions: Some(&files[3]),
        relations: Some(&files[4]),
        outcomes: Some(&files[5]),
        quests: Some(quests),
        questlines: Some(questlines),
    })
}

fn diagnostics(result: Result<impl Sized, ContentError>) -> Vec<String> {
    match result {
        Ok(_) => Vec::new(),
        Err(error) => error.diagnostics.iter().map(ToString::to_string).collect(),
    }
}

/// The problems with the example once `from` is replaced by `to` in `file`.
fn problems_after(file: &str, from: &str, to: &str) -> Vec<String> {
    let (mut quests, mut questlines) = (text("quests.toml"), text("questlines.toml"));
    let edited = if file == "quests.toml" {
        &mut quests
    } else {
        &mut questlines
    };
    assert_eq!(
        edited.matches(from).count(),
        1,
        "'{from}' is in {file} once"
    );
    *edited = edited.replacen(from, to, 1);
    with(&quests, &questlines, |sources| {
        diagnostics(parse_quests(sources))
    })
}

fn riverhold() -> Quests {
    let files = [
        "balance.toml",
        "factions.toml",
        "characters.toml",
        "actions.toml",
        "relations.toml",
        "outcomes.toml",
        "quests.toml",
        "questlines.toml",
    ]
    .map(text);
    match parse_quests(Sources {
        balance: Some(&files[0]),
        factions: Some(&files[1]),
        characters: Some(&files[2]),
        actions: Some(&files[3]),
        relations: Some(&files[4]),
        outcomes: Some(&files[5]),
        quests: Some(&files[6]),
        questlines: Some(&files[7]),
    }) {
        Ok((_, quests)) => quests,
        Err(error) => panic!("the example's quests read cleanly:\n{error}"),
    }
}

fn quest(id: &str) -> QuestId {
    QuestId::new(id).expect("valid id")
}

fn party(id: &str) -> PartyRef {
    PartyRef::new(id).expect("valid id")
}

fn faction(id: &str) -> FactionId {
    FactionId::new(id).expect("valid id")
}

const fn h(hundredths: i64) -> Fixed {
    Fixed::from_hundredths(hundredths)
}

// Reading

#[test]
fn reads_a_faction_quest_with_its_gate_stages_and_choices() {
    let quests = riverhold();
    let oath = &quests.quests[&quest("watch_oath")];
    assert_eq!(oath.name, "The Watch's Oath");
    assert_eq!(oath.giver, Some(party("city_watch")));
    assert_eq!(oath.requires.not_member, [faction("lantern_guild")]);
    let stages: Vec<&str> = oath.stages.iter().map(|stage| stage.id.as_str()).collect();
    assert_eq!(stages, ["patrol", "oath"]);
    let report = &oath.stages[0].choices[0];
    assert_eq!(report.id.as_str(), "report");
    assert_eq!(
        report.effects,
        ChoiceEffects::Outcome(OutcomeId::new("turned_in_vex").expect("valid id"))
    );
    assert_eq!(
        report.next,
        Next::Stage(StageId::new("oath").expect("valid id"))
    );
    assert_eq!(oath.stages[0].choices[1].next, Next::End);
    assert_eq!(
        oath.stages[1].requires.standing.get(&party("city_watch")),
        Some(&h(10_00))
    );
    let ChoiceEffects::Inline(swear) = &oath.stages[1].choices[0].effects else {
        panic!("swearing has inline effects");
    };
    assert_eq!((swear.alignment.law, swear.alignment.good), (h(5_00), h(0)));
}

#[test]
fn reads_givers_that_are_a_character_or_no_one() {
    let quests = riverhold();
    assert_eq!(
        quests.quests[&quest("lost_ring")].giver,
        Some(party("merchant_ava"))
    );
    assert_eq!(quests.quests[&quest("the_long_winter")].giver, None);
    let gate = &quests.quests[&quest("smugglers_cove")].requires;
    assert_eq!(
        gate.done,
        [Progress::parse("watch_oath.patrol.report").expect("valid")]
    );
}

#[test]
fn reads_a_choices_relation_shifts() {
    let quests = riverhold();
    let winter = &quests.quests[&quest("the_long_winter")];
    let ChoiceEffects::Inline(share) = &winter.stages[0].choices[0].effects else {
        panic!("sharing has inline effects");
    };
    assert_eq!(
        share.relations,
        [RelationShift {
            ends: RelationEnds::Between(faction("temple"), faction("city_watch")),
            by: h(5_00),
        }]
    );
}

#[test]
fn a_choices_relation_shifts_are_checked_where_they_are() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "relations = [{ between = [\"temple\", \"city_watch\"], by = 5.0 }]",
            "relations = [{ between = [\"temple\", \"city_wach\"], by = 5.0 }, { from = \"temple\", to = \"free_company\", by = -201.0 }]"
        ),
        [
            "quests.toml: the_long_winter.stages[0].choices[0].effects.relations[0].between[1]: unknown faction 'city_wach' (did you mean 'city_watch'?)",
            "quests.toml: the_long_winter.stages[0].choices[0].effects.relations[1].by: -201.00 is outside -200.00..200.00",
        ]
    );
}

#[test]
fn reads_a_questline_of_steps_with_need_leftovers_and_gates() {
    let quests = riverhold();
    let career = &quests.questlines[&QuestlineId::new("watch_career").expect("valid id")];
    assert_eq!(career.name, "A Life in the Watch");
    assert_eq!(career.giver, Some(party("city_watch")));
    let steps: Vec<(Vec<&str>, Option<usize>, Leftovers)> = career
        .steps
        .iter()
        .map(|step| {
            (
                step.quests.iter().map(QuestId::as_str).collect(),
                step.need,
                step.leftovers,
            )
        })
        .collect();
    assert_eq!(
        steps,
        [
            (vec!["watch_oath"], None, Leftovers::Open),
            (
                vec!["night_patrol", "dock_inspection", "smugglers_cove"],
                Some(2),
                Leftovers::Close
            ),
            (
                vec!["harbour_errands", "lost_dog"],
                Some(0),
                Leftovers::Open
            ),
            (vec!["watch_captain"], None, Leftovers::Open),
        ]
    );
    let gate = &career.steps[3].requires;
    assert_eq!(
        gate.rank_at_least.get(&faction("city_watch")),
        Some(&RankId::new("sergeant").expect("valid id"))
    );
    assert_eq!(gate.standing.get(&party("city_watch")), Some(&h(40_00)));
    assert_eq!(career.steps[0].needed(), 1);
    assert_eq!(career.steps[2].needed(), 0);
}

#[test]
fn riverholds_quests_have_no_warnings() {
    let (content, quests) =
        factional_content::load_quests(&example()).expect("the example reads cleanly");
    assert_eq!(quest_warnings(&content, &quests), []);
}

// Loading

#[test]
fn quests_load_from_a_directory_with_its_content() {
    let (content, quests) =
        factional_content::load_quests(&example()).expect("the example reads cleanly");
    assert_eq!(content.factions.len(), 5);
    assert_eq!((quests.quests.len(), quests.questlines.len()), (10, 1));
    let missing = factional_content::load_quests(&example().join("nowhere"));
    assert!(missing.is_err());
}

#[test]
fn a_world_whose_quests_reconcile_loads() {
    let found = with(&text("quests.toml"), &text("questlines.toml"), |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_world_whose_quests_dont_reconcile_doesnt_load() {
    let quests = text("quests.toml").replacen(", locks = [\"circle_rite\"]", "", 1);
    let found = with(&quests, &text("questlines.toml"), |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(
        found,
        [
            "quests.toml: the_long_winter.stages[0].choices[0]: may lock out circle_rite: it can lower standing with ashen_circle, which no action raises, below the 10.00 its gate needs; declare it in locks"
        ]
    );
}

#[test]
fn questlines_alone_are_checked_against_the_quests_there_are() {
    let found = with("", &text("questlines.toml"), |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(
        found.first().map(String::as_str),
        Some("questlines.toml: watch_career.steps[0].quests[0]: unknown quest 'watch_oath'")
    );
}

#[test]
fn empty_quest_files_load() {
    let found = with("# none yet", "", |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn quest_problems_stop_a_world_loading() {
    let (quests, questlines) = (
        text("quests.toml").replacen("giver = \"merchant_ava\"", "giver = \"merchant_eva\"", 1),
        text("questlines.toml"),
    );
    let found = with(&quests, &questlines, |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(
        found,
        [
            "quests.toml: lost_ring.giver: unknown faction or character 'merchant_eva' (did you mean 'merchant_ava'?)"
        ]
    );
}

// Checks, each at its file and key

#[test]
fn an_unknown_giver_is_reported_with_a_suggestion() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "giver = \"city_watch\"\nrequires",
            "giver = \"city_wach\"\nrequires"
        ),
        [
            "quests.toml: watch_oath.giver: unknown faction or character 'city_wach' (did you mean 'city_watch'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "giver = \"city_watch\"",
            "giver = \"temple_x\""
        ),
        [
            "questlines.toml: watch_career.giver: unknown faction or character 'temple_x' (did you mean 'temple'?)"
        ]
    );
}

#[test]
fn an_unknown_rank_is_reported_with_a_suggestion() {
    assert_eq!(
        problems_after(
            "questlines.toml",
            "city_watch = \"sergeant\"",
            "city_watch = \"sargeant\""
        ),
        [
            "questlines.toml: watch_career.steps[3].requires.rank_at_least.city_watch: unknown rank 'sargeant' for city_watch (did you mean 'sergeant'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "{ city_watch = \"sergeant\" }",
            "{ city_wach = \"sergeant\" }"
        ),
        [
            "questlines.toml: watch_career.steps[3].requires.rank_at_least.city_wach: unknown faction 'city_wach' (did you mean 'city_watch'?)"
        ]
    );
}

#[test]
fn a_standing_requirement_must_name_a_party_within_range() {
    assert_eq!(
        problems_after("questlines.toml", "city_watch = 40.0", "city_watch = 120.0"),
        [
            "questlines.toml: watch_career.steps[3].requires.standing.city_watch: 120.00 is outside -100.00..100.00"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "standing = { city_watch = 10.0 }",
            "standing = { capt_hale = 10.0 }"
        ),
        [
            "quests.toml: watch_oath.stages[1].requires.standing.capt_hale: unknown faction or character 'capt_hale' (did you mean 'captain_hale'?)"
        ]
    );
}

#[test]
fn requirement_lists_must_name_factions_once_each() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "not_member = [\"lantern_guild\"]",
            "not_member = [\"lantern_gild\"]"
        ),
        [
            "quests.toml: watch_oath.requires.not_member[0]: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "member = [\"city_watch\"], within",
            "member = [\"city_watch\", \"city_watch\"], within"
        ),
        ["quests.toml: watch_captain.stages[0].requires.member[1]: 'city_watch' is listed twice"]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "within_tolerance = [\"city_watch\"]",
            "within_tolerance = [\"watch\"]"
        ),
        [
            "quests.toml: watch_captain.stages[0].requires.within_tolerance[0]: unknown faction 'watch'"
        ]
    );
}

#[test]
fn done_must_name_a_quest_stage_or_choice_that_exists() {
    let done = "done = [\"watch_oath.patrol.report\"]";
    assert_eq!(
        problems_after("quests.toml", done, "done = [\"watch_oath.vault\"]"),
        ["quests.toml: smugglers_cove.requires.done[0]: unknown stage 'vault' in watch_oath"]
    );
    assert_eq!(
        problems_after("quests.toml", done, "done = [\"watch_oath.patrol.reprot\"]"),
        [
            "quests.toml: smugglers_cove.requires.done[0]: unknown choice 'reprot' in watch_oath.patrol (did you mean 'report'?)"
        ]
    );
    assert_eq!(
        problems_after("quests.toml", done, "done = [\"wach_oath\"]"),
        [
            "quests.toml: smugglers_cove.requires.done[0]: unknown quest 'wach_oath' (did you mean 'watch_oath'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            done,
            "done = [\"watch_oath\", \"lost_ring\", \"watch_oath\"]"
        ),
        ["quests.toml: smugglers_cove.requires.done[2]: 'watch_oath' is listed twice"]
    );
}

#[test]
fn done_is_written_as_quest_stage_or_choice() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "done = [\"watch_oath.patrol.report\"]",
            "done = [\"watch_oath.patrol.report.now\", \"Watch\"]"
        ),
        [
            "quests.toml: smugglers_cove.requires.done[0]: 'watch_oath.patrol.report.now' has too many parts: write quest, quest.stage or quest.stage.choice",
            "quests.toml: smugglers_cove.requires.done[1]: 'Watch' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
        ]
    );
}

#[test]
fn choices_only_lead_forward_to_stages_of_their_quest() {
    let swear = "{ id = \"swear\", effects = { alignment = { law = 5.0 } }, next = \"end\" }";
    assert_eq!(
        problems_after(
            "quests.toml",
            swear,
            "{ id = \"swear\", effects = { alignment = { law = 5.0 } }, next = \"patrol\" }"
        ),
        [
            "quests.toml: watch_oath.stages[1].choices[0].next: choices only lead forward, and 'patrol' isn't after this stage"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            swear,
            "{ id = \"swear\", effects = { alignment = { law = 5.0 } }, next = \"oath\" }"
        ),
        [
            "quests.toml: watch_oath.stages[1].choices[0].next: choices only lead forward, and 'oath' isn't after this stage"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "outcome = \"turned_in_vex\", next = \"oath\"",
            "outcome = \"turned_in_vex\", next = \"oaht\""
        ),
        [
            "quests.toml: watch_oath.stages[0].choices[0].next: unknown stage 'oaht' in this quest (did you mean 'oath'?)"
        ]
    );
}

#[test]
fn a_choice_names_an_outcome_that_exists_or_effects_within_range() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "outcome = \"turned_in_vex\"",
            "outcome = \"turned_in_vx\""
        ),
        [
            "quests.toml: watch_oath.stages[0].choices[0].outcome: unknown outcome 'turned_in_vx' (did you mean 'turned_in_vex'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "characters = { captain_hale = 10.0 }",
            "characters = { captain_hal = 110.0 }"
        ),
        [
            "quests.toml: lost_dog.stages[0].choices[0].effects.standing.characters.captain_hal: unknown character 'captain_hal' (did you mean 'captain_hale'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "factions = { lantern_guild = 5.0 } } }, next = \"end\" },\n]\n\n# Only",
            "factions = { lantern_guild = -101.0 } } }, next = \"end\" },\n]\n\n# Only"
        ),
        [
            "quests.toml: dock_inspection.stages[0].choices[1].effects.standing.factions.lantern_guild: -101.00 is outside -100.00..100.00"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "{ id = \"return_it\", outcome = \"rescued_merchant\", next = \"end\" }",
            "{ id = \"return_it\", outcome = \"rescued_merchant\", effects = { alignment = { good = 1.0 } }, next = \"end\" }"
        ),
        [
            "quests.toml: lost_ring.stages[0].choices[0]: a choice has an outcome or effects, not both"
        ]
    );
}

#[test]
fn stage_and_choice_ids_are_unique_and_no_stage_is_called_end() {
    assert_eq!(
        problems_after("quests.toml", "id = \"oath\"", "id = \"patrol\""),
        [
            "quests.toml: watch_oath.stages[0].choices[0].next: unknown stage 'oath' in this quest",
            "quests.toml: watch_oath.stages[1].id: another stage is already called 'patrol'",
        ]
    );
    assert_eq!(
        problems_after("quests.toml", "{ id = \"pawn_it\"", "{ id = \"return_it\""),
        [
            "quests.toml: lost_ring.stages[0].choices[1].id: another choice in this stage is already called 'return_it'"
        ]
    );
    assert_eq!(
        problems_after("quests.toml", "id = \"run\"", "id = \"end\""),
        [
            "quests.toml: harbour_errands.stages[0].id: a stage can't be called 'end': next = \"end\" ends the quest"
        ]
    );
}

#[test]
fn a_quest_has_stages_and_a_stage_has_choices() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "[[harbour_errands.stages]]\nid = \"run\"\nchoices = [\n  { id = \"deliver\", effects = { standing = { factions = { city_watch = 3.0 } } }, next = \"end\" },\n]\n",
            ""
        ),
        ["quests.toml: harbour_errands.stages: a quest needs at least one stage"]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "choices = [{ id = \"follow_the_lights\", next = \"raid\" }]",
            "choices = []"
        ),
        ["quests.toml: smugglers_cove.stages[0].choices: a stage needs at least one choice"]
    );
}

#[test]
fn reading_mistakes_are_reported_where_they_are() {
    assert_eq!(
        problems_after("quests.toml", "id = \"run\"", "id = \"run\"\nrequire = {}"),
        [
            "quests.toml: harbour_errands.stages[0]: unknown key 'require' (did you mean 'requires'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { not_member = [\"lantern_guild\"] }",
            "requires = { not_members = [\"lantern_guild\"] }"
        ),
        [
            "quests.toml: watch_oath.requires: unknown key 'not_members' (did you mean 'not_member'?)"
        ]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "leftovers = \"close\"",
            "leftovers = \"shut\""
        ),
        [
            "questlines.toml: watch_career.steps[1].leftovers: unknown leftovers 'shut': use open or close"
        ]
    );
    assert_eq!(
        problems_after("questlines.toml", "need = 2", "need = -1"),
        ["questlines.toml: watch_career.steps[1].need: expected a whole number, like 100"]
    );
    assert_eq!(
        problems_after("quests.toml", "next = \"raid\"", "next = \"Raid\""),
        [
            "quests.toml: smugglers_cove.stages[0].choices[0].next: 'Raid' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "{ id = \"follow_the_lights\", next = \"raid\" }",
            "{ id = \"follow_the_lights\" }"
        ),
        ["quests.toml: smugglers_cove.stages[0].choices[0]: missing 'next'"]
    );
}

// Questlines

#[test]
fn a_step_names_quests_that_exist() {
    assert_eq!(
        problems_after(
            "questlines.toml",
            "\"dock_inspection\"",
            "\"dock_inspektion\""
        ),
        [
            "questlines.toml: watch_career.steps[1].quests[1]: unknown quest 'dock_inspektion' (did you mean 'dock_inspection'?)"
        ]
    );
}

#[test]
fn a_quest_is_in_at_most_one_questline_at_one_step() {
    assert_eq!(
        problems_after(
            "questlines.toml",
            "quests = [\"watch_captain\"]",
            "quests = [\"watch_captain\"]\n\n[young_career]\nname = \"Young\"\n\n[[young_career.steps]]\nquests = [\"lost_ring\", \"night_patrol\"]"
        ),
        [
            "questlines.toml: young_career.steps[0].quests[1]: 'night_patrol' is already at watch_career.steps[1]: a quest is in at most one questline, at one step"
        ]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "quests = [\"watch_captain\"]",
            "quests = [\"watch_captain\", \"watch_oath\"]"
        ),
        [
            "questlines.toml: watch_career.steps[3].quests[1]: 'watch_oath' is already at watch_career.steps[0]: a quest is in at most one questline, at one step"
        ]
    );
}

#[test]
fn a_step_needs_no_more_quests_than_it_has() {
    assert_eq!(
        problems_after("questlines.toml", "need = 2", "need = 4"),
        ["questlines.toml: watch_career.steps[1].need: need is 4, but the step has only 3 quests"]
    );
    assert_eq!(
        problems_after("questlines.toml", "need = 0", "need = 3"),
        ["questlines.toml: watch_career.steps[2].need: need is 3, but the step has only 2 quests"]
    );
}

#[test]
fn a_questline_has_steps_and_a_step_has_quests() {
    assert_eq!(
        problems_after(
            "questlines.toml",
            "quests = [\"watch_oath\"]",
            "quests = []"
        ),
        ["questlines.toml: watch_career.steps[0].quests: a step needs at least one quest"]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "giver = \"city_watch\"\n",
            "giver = \"city_watch\"\n\n[empty_line]\nname = \"Nothing\"\n"
        ),
        ["questlines.toml: empty_line.steps: a questline needs at least one step"]
    );
}

#[test]
fn closing_leftovers_on_a_step_that_needs_every_quest_warns() {
    let questlines = text("questlines.toml").replacen("need = 2\n", "", 1);
    let found = with(&text("quests.toml"), &questlines, |sources| {
        let (content, quests) = parse_quests(sources).expect("it reads cleanly");
        quest_warnings(&content, &quests)
            .iter()
            .map(ToString::to_string)
            .collect()
    });
    assert_eq!(
        found,
        [
            "questlines.toml: watch_career.steps[1].leftovers: leftovers = \"close\" has no effect: the step needs all its quests, so none are left over"
        ]
    );
}

#[test]
fn quest_checks_wait_until_every_file_reads_cleanly() {
    // A quest file that can't be read is reported alone, not with every reference into it.
    assert_eq!(
        problems_after("quests.toml", "name = \"The Watch's Oath\"", "name = 3"),
        ["quests.toml: watch_oath.name: expected text in quotes"]
    );
}

// Reachability (Q3)

/// The problems with the example once each `(file, from, to)` replacement is made, in any
/// of its files.
fn problems_after_all(edits: &[(&str, &str, &str)]) -> Vec<String> {
    let names = [
        "balance.toml",
        "factions.toml",
        "characters.toml",
        "actions.toml",
        "relations.toml",
        "outcomes.toml",
        "quests.toml",
        "questlines.toml",
    ];
    let mut files = names.map(text);
    for (file, from, to) in edits {
        let place = names
            .iter()
            .position(|name| name == file)
            .expect("one of the example's files");
        let edited = &mut files[place];
        assert_eq!(
            edited.matches(from).count(),
            1,
            "'{from}' is in {file} once"
        );
        *edited = edited.replacen(from, to, 1);
    }
    diagnostics(parse_quests(Sources {
        balance: Some(&files[0]),
        factions: Some(&files[1]),
        characters: Some(&files[2]),
        actions: Some(&files[3]),
        relations: Some(&files[4]),
        outcomes: Some(&files[5]),
        quests: Some(&files[6]),
        questlines: Some(&files[7]),
    }))
}

#[test]
fn a_stage_no_choice_leads_to_can_never_be_reached() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "{ id = \"follow_the_lights\", next = \"raid\" }",
            "{ id = \"follow_the_lights\", next = \"end\" }"
        ),
        [
            "quests.toml: smugglers_cove.stages[1]: no choice leads to this stage, so it can never be reached"
        ]
    );
}

#[test]
fn a_gate_that_can_never_hold_is_reported_at_its_not_member() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { not_member = [\"lantern_guild\"] }",
            "requires = { not_member = [\"lantern_guild\"], member = [\"lantern_guild\"] }"
        ),
        [
            "quests.toml: watch_oath.requires.not_member[0]: it also needs to be in lantern_guild, so it can never hold"
        ]
    );
    // A quest's start gate is its own requires and its step's.
    assert_eq!(
        problems_after_all(&[(
            "quests.toml",
            "name = \"Captain of the Watch\"",
            "name = \"Captain of the Watch\"\nrequires = { not_member = [\"temple\", \"city_watch\"] }"
        ),]),
        [
            "quests.toml: watch_captain.requires.not_member[1]: it also needs a rank in city_watch, so it can never hold"
        ]
    );
}

#[test]
fn a_quest_can_only_need_its_own_progress_where_it_could_have_happened() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { standing = { city_watch = 10.0 } }",
            "requires = { standing = { city_watch = 10.0 }, done = [\"watch_oath.patrol.look_away\"] }"
        ),
        [
            "quests.toml: watch_oath.stages[1].requires.done[0]: watch_oath.patrol.look_away doesn't lead to this stage, so it can't be done here"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { standing = { city_watch = 10.0 } }",
            "requires = { standing = { city_watch = 10.0 }, done = [\"watch_oath.patrol.report\", \"watch_oath.patrol\", \"watch_oath\"] }"
        ),
        [
            "quests.toml: watch_oath.stages[1].requires.done[2]: watch_oath can't be over while one of its own stages is under way"
        ]
    );
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { not_member = [\"lantern_guild\"] }",
            "requires = { not_member = [\"lantern_guild\"], done = [\"watch_oath\"] }"
        ),
        ["quests.toml: watch_oath.requires.done[0]: a quest can't need its own progress to start"]
    );
    assert_eq!(
        problems_after(
            "questlines.toml",
            "quests = [\"harbour_errands\", \"lost_dog\"]",
            "quests = [\"harbour_errands\", \"lost_dog\"]\nrequires = { done = [\"lost_dog.search\"] }"
        ),
        [
            "questlines.toml: watch_career.steps[2].requires.done[0]: lost_dog is in this step, and a quest can't need its own progress to start"
        ]
    );
}

#[test]
fn quests_that_wait_on_each_other_can_never_start() {
    assert_eq!(
        problems_after_all(&[
            (
                "quests.toml",
                "name = \"Ava's Lost Ring\"",
                "name = \"Ava's Lost Ring\"\nrequires = { done = [\"the_long_winter\"] }"
            ),
            (
                "quests.toml",
                "name = \"The Long Winter\"",
                "name = \"The Long Winter\"\nrequires = { done = [\"lost_ring.search\"] }"
            ),
        ]),
        [
            "quests.toml: lost_ring: it can never start: it needs the_long_winter done, which can never happen",
            "quests.toml: the_long_winter: it can never start: it needs lost_ring.search done, which can never happen",
        ]
    );
}

#[test]
fn a_questline_waiting_on_its_own_end_never_gets_going() {
    // The oath needs the captaincy, which needs every step before it.
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { not_member = [\"lantern_guild\"] }",
            "requires = { not_member = [\"lantern_guild\"], done = [\"watch_captain\"] }"
        ),
        [
            "quests.toml: dock_inspection: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: harbour_errands: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: lost_dog: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: night_patrol: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: smugglers_cove: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: watch_captain: it can never start: watch_career.steps[0] can never be complete",
            "quests.toml: watch_oath: it can never start: it needs watch_captain done, which can never happen",
        ]
    );
}

#[test]
fn a_quest_whose_needs_come_only_after_its_step_closes_can_never_start() {
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { done = [\"watch_oath.patrol.report\"] }",
            "requires = { done = [\"watch_captain\"] }"
        ),
        [
            "quests.toml: smugglers_cove: it can never start: what it needs only comes after watch_career moves on from steps[1], which closes it"
        ]
    );
    // With the leftovers kept open, it can start later.
    assert_eq!(
        problems_after_all(&[
            (
                "quests.toml",
                "requires = { done = [\"watch_oath.patrol.report\"] }",
                "requires = { done = [\"watch_captain\"] }"
            ),
            (
                "questlines.toml",
                "leftovers = \"close\"",
                "leftovers = \"open\""
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_stage_that_needs_its_quest_over_first_can_never_be_reached() {
    assert_eq!(
        problems_after_all(&[
            (
                "quests.toml",
                "requires = { standing = { city_watch = 10.0 } }",
                "requires = { standing = { city_watch = 10.0 }, done = [\"the_long_winter\"] }"
            ),
            (
                "quests.toml",
                "name = \"The Long Winter\"",
                "name = \"The Long Winter\"\nrequires = { done = [\"watch_oath\"] }"
            ),
        ]),
        [
            "quests.toml: watch_oath.stages[1].requires.done[0]: the_long_winter can only happen once watch_oath is over, so this stage can never be reached"
        ]
    );
    // Needing an earlier stage of its own quest, through another quest, is fine.
    assert_eq!(
        problems_after_all(&[
            (
                "quests.toml",
                "requires = { standing = { city_watch = 10.0 } }",
                "requires = { standing = { city_watch = 10.0 }, done = [\"the_long_winter\"] }"
            ),
            (
                "quests.toml",
                "name = \"The Long Winter\"",
                "name = \"The Long Winter\"\nrequires = { done = [\"watch_oath.patrol.report\"] }"
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_stage_waiting_on_a_quest_that_never_starts_is_reported_with_it() {
    assert_eq!(
        problems_after_all(&[
            (
                "quests.toml",
                "name = \"Ava's Lost Ring\"",
                "name = \"Ava's Lost Ring\"\nrequires = { done = [\"the_long_winter.stores\"] }"
            ),
            (
                "quests.toml",
                "id = \"stores\"",
                "id = \"stores\"\nrequires = { done = [\"lost_ring\"] }"
            ),
        ]),
        [
            "quests.toml: lost_ring: it can never start: it needs the_long_winter.stores done, which can never happen",
            "quests.toml: the_long_winter.stages[0].requires.done[0]: lost_ring can never happen, so this stage can never be reached",
        ]
    );
}

#[test]
fn reachability_waits_until_the_structure_is_sound() {
    // The oath can't start, but only that is reported, not every quest waiting on it.
    assert_eq!(
        problems_after(
            "quests.toml",
            "requires = { not_member = [\"lantern_guild\"] }",
            "requires = { not_member = [\"lantern_guild\"], member = [\"lantern_guild\"] }"
        )
        .len(),
        1
    );
}

// Lockouts (Q4)

/// The war the Ashen Rite can start, as the example declares it.
const RITE_LOCKS: &str = ", locks = [\"watch_captain\", \"watch_captain.command\"]";

#[test]
fn the_example_declares_every_lockout() {
    assert_eq!(problems_after_all(&[]), Vec::<String>::new());
}

#[test]
fn an_undeclared_war_locks_out_whatever_needs_either_membership() {
    // The Watch and the Temple start at 60; `sowed_discord` can take them to -100, and
    // -100 - 120 stops at -100, at or below the conflict threshold of -50.
    assert_eq!(
        problems_after("quests.toml", RITE_LOCKS, ""),
        [
            "quests.toml: circle_rite.stages[0].choices[1]: may lock out watch_captain: it can start a war between city_watch and temple, ending the sergeant rank in city_watch that its gate needs; declare it in locks",
            "quests.toml: circle_rite.stages[0].choices[1]: may lock out watch_captain.command: it can start a war between city_watch and temple, ending the membership of city_watch that the stage needs; declare it in locks",
        ]
    );
}

#[test]
fn lowering_standing_no_action_raises_is_a_lockout() {
    // Sharing gives the Temple 10; the Circle regards the Temple at -90, where spillover is
    // -0.30 + 10 / 50 × 0.30 = -0.24, so 10 × -0.24 = -2.40 spills to the Circle, and no
    // action raises standing with the Circle.
    assert_eq!(
        problems_after("quests.toml", ", locks = [\"circle_rite\"]", ""),
        [
            "quests.toml: the_long_winter.stages[0].choices[0]: may lock out circle_rite: it can lower standing with ashen_circle, which no action raises, below the 10.00 its gate needs; declare it in locks"
        ]
    );
}

#[test]
fn lowering_standing_an_action_raises_is_no_lockout() {
    // `report_crime` raises standing with the Watch, so the oath's 10 can always be won back.
    assert_eq!(
        problems_after(
            "quests.toml",
            "{ id = \"by_the_book\", effects = { standing = { factions = { city_watch = 5.0 } } }",
            "{ id = \"by_the_book\", effects = { standing = { factions = { city_watch = -40.0 } } }"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn drift_on_probation_locks_out_only_without_a_way_back() {
    // The Watch puts drifting members on probation, and every way each axis moves has an
    // action moving it back, until a profile can't move toward lawful at -100.
    assert_eq!(
        problems_after_all(&[(
            "balance.toml",
            "[inertia.profiles.hardening]",
            "[inertia.profiles.stubborn]\nlaw.toward_lawful = [[-100.0, 0.0], [100.0, 1.0]]\n\n[inertia.profiles.hardening]"
        )]),
        [
            "quests.toml: watch_oath.stages[0].choices[1]: may lock out watch_captain: it moves alignment toward chaotic, which no action moves back toward lawful, so city_watch's probation can run out, ending the sergeant rank in city_watch that its gate needs; declare it in locks",
            "quests.toml: watch_oath.stages[0].choices[1]: may lock out watch_captain.command: it moves alignment toward chaotic, which no action moves back toward lawful, so city_watch's probation can run out, ending the membership of city_watch that the stage needs; declare it in locks",
        ]
    );
}

/// A Temple quest whose ordination needs membership of the Temple, added to the example.
const TEMPLE_VOWS: &str = "[temple_vows]\nname = \"Temple Vows\"\ngiver = \"temple\"\n\n[[temple_vows.stages]]\nid = \"ordination\"\nrequires = { member = [\"temple\"] }\nchoices = [{ id = \"kneel\", next = \"end\" }]\n\n[circle_rite]";

#[test]
fn drift_that_demotes_at_once_locks_out_whatever_needs_the_membership() {
    // The Temple weighs both axes and demotes members who drift, expelling them from the
    // lowest rank: every choice that moves alignment can end the membership, and so can the
    // war.
    let found = problems_after_all(&[("quests.toml", "[circle_rite]", TEMPLE_VOWS)]);
    let choices: Vec<&str> = found
        .iter()
        .map(|problem| problem.split(": may lock out").next().unwrap_or(problem))
        .collect();
    assert_eq!(
        choices,
        [
            "quests.toml: circle_rite.stages[0].choices[1]",
            "quests.toml: dock_inspection.stages[0].choices[0]",
            "quests.toml: lost_ring.stages[0].choices[0]",
            "quests.toml: lost_ring.stages[0].choices[1]",
            "quests.toml: the_long_winter.stages[0].choices[0]",
            "quests.toml: the_long_winter.stages[0].choices[1]",
            "quests.toml: watch_captain.stages[0].choices[0]",
            "quests.toml: watch_oath.stages[0].choices[0]",
            "quests.toml: watch_oath.stages[0].choices[1]",
            "quests.toml: watch_oath.stages[1].choices[0]",
        ]
    );
    assert_eq!(
        found[0],
        "quests.toml: circle_rite.stages[0].choices[1]: may lock out temple_vows.ordination: it can start a war between temple and city_watch, ending the membership of temple that the stage needs; declare it in locks"
    );
    assert_eq!(
        found[1],
        "quests.toml: dock_inspection.stages[0].choices[0]: may lock out temple_vows.ordination: it moves alignment toward lawful, and temple demotes members who drift out of its tolerance, which can end the membership of temple that the stage needs; declare it in locks"
    );
}

#[test]
fn quests_finished_before_the_choices_quest_starts_are_not_locked_out() {
    // The oath is the questline's first step, needing all of it, so every later step's
    // choices come after it's over; only the lost ring, the winter and the rite can reach it.
    let found = problems_after(
        "quests.toml",
        "requires = { standing = { city_watch = 10.0 } }",
        "requires = { standing = { city_watch = 10.0 }, member = [\"temple\"] }",
    );
    let choices: Vec<&str> = found
        .iter()
        .map(|problem| problem.split(": it ").next().unwrap_or(problem))
        .collect();
    assert_eq!(
        choices,
        [
            "quests.toml: circle_rite.stages[0].choices[1]: may lock out watch_oath.oath",
            "quests.toml: lost_ring.stages[0].choices[0]: may lock out watch_oath.oath",
            "quests.toml: lost_ring.stages[0].choices[1]: may lock out watch_oath.oath",
            "quests.toml: the_long_winter.stages[0].choices[0]: may lock out watch_oath.oath",
            "quests.toml: the_long_winter.stages[0].choices[1]: may lock out watch_oath.oath",
        ]
    );
}

#[test]
fn locks_name_other_quests_and_their_stages_once_each() {
    assert_eq!(
        problems_after(
            "quests.toml",
            RITE_LOCKS,
            ", locks = [\"watch_captan\", \"watch_captain.comand\", \"circle_rite\", \"watch_captain.command\", \"watch_captain.command\", \"watch_captain\"]"
        ),
        [
            "quests.toml: circle_rite.stages[0].choices[1].locks[0]: unknown quest 'watch_captan' (did you mean 'watch_captain'?)",
            "quests.toml: circle_rite.stages[0].choices[1].locks[1]: unknown stage 'comand' in watch_captain (did you mean 'command'?)",
            "quests.toml: circle_rite.stages[0].choices[1].locks[2]: a choice can't lock out its own quest: its choices exclude each other by design",
            "quests.toml: circle_rite.stages[0].choices[1].locks[4]: 'watch_captain.command' is listed twice",
        ]
    );
}

#[test]
fn a_lock_is_written_as_quest_or_quest_stage() {
    assert_eq!(
        problems_after(
            "quests.toml",
            RITE_LOCKS,
            ", locks = [\"watch_captain.command.accept\", 3]"
        ),
        [
            "quests.toml: circle_rite.stages[0].choices[1].locks[0]: 'watch_captain.command.accept' has too many parts: write quest, for its gate, or quest.stage",
            "quests.toml: circle_rite.stages[0].choices[1].locks[1]: expected text in quotes",
        ]
    );
}

#[test]
fn a_declared_lock_that_cannot_happen_warns() {
    // The oath is over before any patrol starts; and patrolling raises standing with the
    // Watch, and what spills from it lowers only the Temple's, which donating raises, and the
    // Guild's, which nothing needs.
    let quests = text("quests.toml").replacen(
        "{ factions = { city_watch = 5.0 } } }, next = \"end\" }",
        "{ factions = { city_watch = 5.0 } } }, next = \"end\", locks = [\"watch_oath.oath\", \"watch_captain\"] }",
        1,
    );
    let found = with(&quests, &text("questlines.toml"), |sources| {
        let (content, quests) = parse_quests(sources).expect("it reads cleanly");
        quest_warnings(&content, &quests)
            .iter()
            .map(ToString::to_string)
            .collect()
    });
    assert_eq!(
        found,
        [
            "quests.toml: night_patrol.stages[0].choices[0].locks[0]: it can't lock out watch_oath.oath: nothing it does can make that stage's requirements false for good; remove it",
            "quests.toml: night_patrol.stages[0].choices[0].locks[1]: it can't lock out watch_captain: nothing it does can make that quest's gate false for good; remove it",
        ]
    );
}

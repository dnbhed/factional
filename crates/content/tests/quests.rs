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
    assert_eq!(quest_warnings(&riverhold()), []);
}

// Loading

#[test]
fn quests_load_from_a_directory_with_its_content() {
    let (content, quests) =
        factional_content::load_quests(&example()).expect("the example reads cleanly");
    assert_eq!(content.factions.len(), 5);
    assert_eq!((quests.quests.len(), quests.questlines.len()), (9, 1));
    let missing = factional_content::load_quests(&example().join("nowhere"));
    assert!(missing.is_err());
}

#[test]
fn a_world_with_quests_does_not_load_until_quests_reconcile() {
    let found = with(&text("quests.toml"), &text("questlines.toml"), |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(
        found,
        [
            "quests.toml: quests are read and checked, but a world with quests can't load until the check that they reconcile is built (DESIGN.md §17.2)"
        ]
    );
}

#[test]
fn questlines_alone_are_reported_at_their_own_file() {
    let found = with("", &text("questlines.toml"), |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(
        found.last().map(String::as_str),
        Some(
            "questlines.toml: quests are read and checked, but a world with quests can't load until the check that they reconcile is built (DESIGN.md §17.2)"
        )
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
fn quest_problems_are_reported_before_the_quests_cannot_load() {
    let (quests, questlines) = (
        text("quests.toml").replacen("giver = \"merchant_ava\"", "giver = \"merchant_eva\"", 1),
        text("questlines.toml"),
    );
    let found = with(&quests, &questlines, |sources| {
        diagnostics(parse_content(sources))
    });
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(
        found[0],
        "quests.toml: lost_ring.giver: unknown faction or character 'merchant_eva' (did you mean 'merchant_ava'?)"
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
        let (_, quests) = parse_quests(sources).expect("it reads cleanly");
        quest_warnings(&quests)
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

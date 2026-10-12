//! Adding requirements and effects by kind (U11b): a requirements or effects table offers
//! "Requirement" or "Effect", a menu of each kind it takes, with each id's choices, each made in
//! one change.

use std::path::Path;

use factional_content::{
    ContentTexts, EditError, Offer, Offered, ValuePath, entry_form, make_offer, place_kinds,
    read_texts,
};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn path(text: &str) -> ValuePath {
    ValuePath::parse(text).expect("a path")
}

/// The offer the section `group` of `entry` makes labelled `label`.
fn section_offer(file: &str, entry: &str, group: &str, label: &str) -> Offer {
    entry_form(&sample(), file, entry)
        .into_iter()
        .find(|section| section.name.as_deref() == Some(group))
        .unwrap_or_else(|| panic!("{group} in {entry}'s form"))
        .offers
        .into_iter()
        .find(|offer| offer.label == label)
        .unwrap_or_else(|| panic!("{label} offered in {group}"))
}

/// The kinds `offer` is a menu of.
fn kinds(offer: &Offer) -> &[Offer] {
    match &offer.what {
        Offered::Kinds(kinds) => kinds,
        _ => panic!("{} is a menu of kinds", offer.label),
    }
}

/// Each kind as `label at what`.
fn said(offer: &Offer) -> Vec<String> {
    kinds(offer)
        .iter()
        .map(|kind| {
            let what = match &kind.what {
                Offered::Key(key) => format!("key {key}"),
                Offered::Item => "an item".to_owned(),
                Offered::Id(choices) | Offered::Pick(choices) => {
                    let ids: Vec<&str> = choices.iter().map(|choice| choice.id.as_str()).collect();
                    format!("one of {}", ids.join(", "))
                }
                Offered::Kinds(_) => "kinds".to_owned(),
            };
            format!("{} at {}: {what}", kind.label, kind.at)
        })
        .collect()
}

fn kind<'o>(offer: &'o Offer, label: &str) -> &'o Offer {
    kinds(offer)
        .iter()
        .find(|kind| kind.label == label)
        .unwrap_or_else(|| panic!("{label} is a kind"))
}

const FACTIONS: &str = "ashen_circle, city_watch, free_company, lantern_guild, temple";
const PARTIES: &str = "ashen_circle, brother_ash, captain_hale, city_watch, free_company, \
                       lantern_guild, merchant_ava, player, sister_mira, temple, vex";
const QUESTS: &str = "circle_rite, dock_inspection, harbour_errands, lost_dog, lost_ring, \
                      night_patrol, smugglers_cove, the_long_winter, watch_captain, watch_oath";

#[test]
fn a_quests_requirements_offer_each_kind_of_requirement() {
    let offer = section_offer("quests.toml", "watch_oath", "Requirements", "Requirement");
    assert_eq!(offer.at, path("watch_oath.requires"));
    let not_lantern = "ashen_circle, city_watch, free_company, temple";
    assert_eq!(
        said(&offer),
        [
            format!("Done at watch_oath.requires.done: one of {QUESTS}"),
            format!("Member of at watch_oath.requires.member: one of {FACTIONS}"),
            // The Guild is there already.
            format!("Not a member of at watch_oath.requires.not_member: one of {not_lantern}"),
            format!("Rank at least at watch_oath.requires.rank_at_least: one of {FACTIONS}"),
            format!("Standing at watch_oath.requires.standing: one of {PARTIES}"),
            format!(
                "Within tolerance of at watch_oath.requires.within_tolerance: one of {FACTIONS}"
            ),
        ]
    );
}

#[test]
fn requirements_not_written_yet_offer_their_kinds_not_the_table() {
    let groups = entry_form(&sample(), "quests.toml", "dock_inspection");
    let requirements = groups
        .iter()
        .find(|group| group.name.as_deref() == Some("Requirements"))
        .expect("a Requirements section");
    let labels: Vec<&str> = requirements
        .offers
        .iter()
        .map(|offer| offer.label.as_str())
        .collect();
    assert_eq!(labels, ["Requirement"]);
    assert_eq!(kinds(&requirements.offers[0]).len(), 6);
}

#[test]
fn a_choices_effects_offer_each_kind_of_effect_not_yet_there() {
    let at = path("dock_inspection.stages[0].choices[0].effects");
    let offer = place_kinds(&sample(), "quests.toml", &at).expect("effects take kinds");
    assert_eq!(offer.label, "Effect");
    assert_eq!(offer.at, at);
    let everyone = "brother_ash, captain_hale, merchant_ava, player, sister_mira, vex";
    assert_eq!(
        said(&offer),
        [
            // Law is moved already; good isn't.
            "Alignment › Good at dock_inspection.stages[0].choices[0].effects.alignment: key good"
                .to_owned(),
            "Relations at dock_inspection.stages[0].choices[0].effects.relations: an item"
                .to_owned(),
            format!(
                "Standing › Characters at \
                 dock_inspection.stages[0].choices[0].effects.standing.characters: one of {everyone}"
            ),
            "Standing › Factions at \
             dock_inspection.stages[0].choices[0].effects.standing.factions: one of \
             ashen_circle, free_company, temple"
                .to_owned(),
        ]
    );
}

#[test]
fn a_stages_requirements_take_kinds_and_other_tables_dont() {
    let texts = sample();
    let oath = place_kinds(
        &texts,
        "quests.toml",
        &path("watch_oath.stages[1].requires"),
    )
    .expect("a stage's requirements take kinds");
    assert_eq!(oath.label, "Requirement");
    let standing = kind(&oath, "Standing");
    let Offered::Id(choices) = &standing.what else {
        panic!("standing is by id");
    };
    // The Watch's standing is needed already, and the rest are offered with their names.
    assert!(choices.iter().all(|choice| choice.id != "city_watch"));
    let guild = choices.iter().find(|choice| choice.id == "lantern_guild");
    assert_eq!(
        guild.and_then(|guild| guild.name.as_deref()),
        Some("The Lantern Guild")
    );
    let step = path("watch_career.steps[3].requires");
    assert!(place_kinds(&texts, "questlines.toml", &step).is_some());
    for (file, at) in [
        ("quests.toml", "watch_oath.stages[1]"),
        (
            "quests.toml",
            "dock_inspection.stages[0].choices[0].effects.alignment",
        ),
        ("factions.toml", "city_watch.ranks[1].requires"),
        ("outcomes.toml", "turned_in_vex"),
    ] {
        assert_eq!(place_kinds(&texts, file, &path(at)), None, "{at}");
    }
}

#[test]
fn a_kind_is_made_with_what_its_within_in_one_change() {
    let texts = sample();
    let place = factional_content::CONTENT_FILES
        .iter()
        .position(|file| *file == "quests.toml")
        .expect("quests.toml");
    let text = texts[place].as_deref().expect("quests.toml");
    let requirements = section_offer(
        "quests.toml",
        "dock_inspection",
        "Requirements",
        "Requirement",
    );
    let member = kind(&requirements, "Member of");
    let (made, at) = make_offer("quests.toml", text, member, Some("temple")).expect("made");
    assert_eq!(at, path("dock_inspection.requires.member[0]"));
    let quests: toml::Table = made.parse().expect("toml");
    assert_eq!(
        quests["dock_inspection"]["requires"]["member"],
        toml::Value::Array(vec!["temple".into()])
    );

    let effects = path("dock_inspection.stages[0].choices[0].effects");
    let effects = place_kinds(&texts, "quests.toml", &effects).expect("kinds");
    let good = kind(&effects, "Alignment › Good");
    let (_, at) = make_offer("quests.toml", text, good, None).expect("made");
    assert_eq!(
        at,
        path("dock_inspection.stages[0].choices[0].effects.alignment.good")
    );

    // The menu itself isn't made; one of its kinds is.
    assert_eq!(
        make_offer("quests.toml", text, &effects, None),
        Err(EditError::CantAdd(effects.at.clone()))
    );
}

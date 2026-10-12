//! Every section, with its adding in it (U10d): each group of an entry's schema in its form,
//! even with nothing written in it, with what can be added there, each made in one change.

use std::path::Path;

use factional_content::{
    ContentTexts, EditError, FormGroup, Offer, Offered, ValuePath, entry_form, make_offer,
    read_texts,
};

fn sample() -> ContentTexts {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    read_texts(&dir).expect("the sample is there")
}

fn form(file: &str, entry: &str) -> Vec<FormGroup> {
    entry_form(&sample(), file, entry)
}

/// Each group's name and its offers, as `label at what`.
fn offered(groups: &[FormGroup]) -> Vec<(&str, Vec<String>)> {
    groups
        .iter()
        .map(|group| {
            let offers = group.offers.iter().map(said).collect();
            (group.name.as_deref().unwrap_or_default(), offers)
        })
        .collect()
}

fn said(offer: &Offer) -> String {
    let what = match &offer.what {
        Offered::Key(key) => format!("key {key}"),
        Offered::Item => "an item".to_owned(),
        Offered::Id(choices) | Offered::Pick(choices) => {
            let ids: Vec<&str> = choices.iter().map(|choice| choice.id.as_str()).collect();
            format!("one of {}", ids.join(", "))
        }
        Offered::Kinds(kinds) => format!("{} kinds", kinds.len()),
    };
    format!("{} at {}: {what}", offer.label, offer.at)
}

fn offer<'f>(groups: &'f [FormGroup], label: &str) -> &'f Offer {
    groups
        .iter()
        .flat_map(|group| &group.offers)
        .find(|offer| offer.label == label)
        .unwrap_or_else(|| panic!("{label} is offered"))
}

#[test]
fn every_section_is_there_with_what_can_be_added_in_it() {
    let everyone = "brother_ash, captain_hale, merchant_ava, player, sister_mira, vex";
    assert_eq!(
        offered(&form("characters.toml", "vex")),
        [
            ("Identity", vec![]),
            (
                "Where they stand",
                // In the schema's order of keys, which is alphabetical.
                vec![
                    "Inertia at vex: key inertia".to_owned(),
                    "Weights at vex: key weights".to_owned(),
                ]
            ),
            (
                "Memberships",
                vec!["Membership at vex.memberships: an item".to_owned()]
            ),
            (
                "Standing",
                vec![
                    format!("Character at vex.standing.characters: one of {everyone}"),
                    // The Guild's standing is there already.
                    "Faction at vex.standing.factions: one of ashen_circle, city_watch, \
                     free_company, temple"
                        .to_owned(),
                ]
            ),
            // Vex has no contacts, and the section is there to add them.
            (
                "Contacts",
                vec![format!("Contact at vex.contacts: one of {everyone}")]
            ),
        ]
    );
    let groups = form("characters.toml", "vex");
    assert!(groups[4].rows.is_empty());
}

#[test]
fn a_key_with_a_default_is_set_rather_than_offered() {
    // The Watch's expulsion and leaving costs and secret members have defaults, shown with
    // Set; its own rule tables have none.
    assert_eq!(
        offered(&form("factions.toml", "city_watch")),
        [
            ("Identity", vec![]),
            ("Where it stands", vec![]),
            ("Membership", vec![]),
            ("Drift", vec![]),
            (
                "Ranks",
                vec!["Rank at city_watch.ranks: an item".to_owned()]
            ),
            (
                "Its own rules",
                vec![
                    "Defectors at city_watch: key defectors".to_owned(),
                    "Deserters at city_watch: key deserters".to_owned(),
                    "Exposed at city_watch: key exposed".to_owned(),
                ]
            ),
        ]
    );
}

#[test]
fn a_list_of_ids_offers_those_not_in_it_yet() {
    let groups = form("characters.toml", "merchant_ava");
    assert_eq!(
        said(offer(&groups, "Contact")),
        "Contact at merchant_ava.contacts: one of brother_ash, merchant_ava, player, vex"
    );
}

fn text(file: &str) -> String {
    let place = factional_content::CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file");
    sample()[place].clone().expect("the file is there")
}

fn path(key: &str) -> ValuePath {
    ValuePath::parse(key).expect("a key")
}

#[test]
fn an_id_is_added_with_what_its_within_in_one_change() {
    let groups = form("characters.toml", "merchant_ava");
    let (changed, added) = make_offer(
        "characters.toml",
        &text("characters.toml"),
        offer(&groups, "Faction"),
        Some("city_watch"),
    )
    .expect("made");
    assert_eq!(added, path("merchant_ava.standing.factions.city_watch"));
    assert!(
        changed.contains("standing = { factions = { city_watch = 0.0 } }\n"),
        "{changed}"
    );
}

#[test]
fn an_item_is_added_with_its_list_if_it_isnt_there() {
    let groups = form("characters.toml", "merchant_ava");
    let (changed, added) = make_offer(
        "characters.toml",
        &text("characters.toml"),
        offer(&groups, "Membership"),
        None,
    )
    .expect("made");
    assert_eq!(added, path("merchant_ava.memberships[0]"));
    assert!(
        changed.contains("memberships = [{ faction = \"\" }]\n"),
        "{changed}"
    );
    // A list that's there takes one more.
    let groups = form("factions.toml", "city_watch");
    let (changed, added) = make_offer(
        "factions.toml",
        &text("factions.toml"),
        offer(&groups, "Rank"),
        None,
    )
    .expect("made");
    assert_eq!(added, path("city_watch.ranks[3]"));
    assert_eq!(changed.matches("[[city_watch.ranks]]").count(), 4);
}

#[test]
fn a_picked_id_and_a_key_are_added_too() {
    let groups = form("characters.toml", "vex");
    let (changed, added) = make_offer(
        "characters.toml",
        &text("characters.toml"),
        offer(&groups, "Contact"),
        Some("captain_hale"),
    )
    .expect("made");
    assert_eq!(added, path("vex.contacts[0]"));
    assert!(
        changed.contains("contacts = [\"captain_hale\"]\n"),
        "{changed}"
    );
    let (changed, added) = make_offer(
        "characters.toml",
        &text("characters.toml"),
        offer(&groups, "Weights"),
        None,
    )
    .expect("made");
    assert_eq!(added, path("vex.weights"));
    assert!(
        changed.contains("weights = { law = 0.0, good = 0.0 }\n"),
        "{changed}"
    );
}

#[test]
fn an_id_must_be_given() {
    let groups = form("characters.toml", "merchant_ava");
    let made = make_offer(
        "characters.toml",
        &text("characters.toml"),
        offer(&groups, "Faction"),
        None,
    );
    assert_eq!(made, Err(EditError::Blank));
}

#[test]
fn a_key_the_entrys_form_rules_out_isnt_offered() {
    // A relation is between two factions, or from one to another, never both.
    assert_eq!(
        offered(&form("relations.toml", "relation[0]")),
        [("Factions", vec![]), ("Regard", vec![])]
    );
    // One from and to has neither a between to add, as it's ruled out, nor more of either.
    assert_eq!(
        offered(&form("relations.toml", "relation[5]")),
        [("Factions", vec![]), ("Regard", vec![])]
    );
}

/// The sample, with `text` written at the end of `file`.
fn with(file: &str, added: &str) -> ContentTexts {
    let mut texts = sample();
    let place = factional_content::CONTENT_FILES
        .iter()
        .position(|name| *name == file)
        .expect("a content file");
    texts[place].as_mut().expect("there").push_str(added);
    texts
}

#[test]
fn a_new_list_that_must_have_an_item_is_made_with_only_that_one() {
    let texts = with("factions.toml", "\n[newcomers]\nname = \"Newcomers\"\n");
    let groups = entry_form(&texts, "factions.toml", "newcomers");
    let text = texts[1].clone().expect("factions.toml");
    let (changed, added) =
        make_offer("factions.toml", &text, offer(&groups, "Rank"), None).expect("made");
    // A ladder has at least one rung, so making it makes the first.
    assert_eq!(added, path("newcomers.ranks[0]"));
    assert!(changed.contains("ranks = [{ id = \"\" }]\n"), "{changed}");
}

#[test]
fn an_offer_for_an_entry_that_isnt_there_is_refused() {
    let offer = Offer {
        label: "Weights".to_owned(),
        at: path("nobody"),
        what: Offered::Key("weights".to_owned()),
    };
    let text = text("characters.toml");
    assert_eq!(
        make_offer("characters.toml", &text, &offer, None),
        Err(EditError::NotThere(path("nobody")))
    );
}

#[test]
fn a_list_with_room_offers_what_fits_it() {
    // Half a pair has room for its second.
    let texts = with(
        "relations.toml",
        "\n[[relation]]\nbetween = [\"temple\"]\nvalue = 10.0\n",
    );
    let groups = entry_form(&texts, "relations.toml", "relation[7]");
    assert_eq!(
        said(offer(&groups, "Faction")),
        "Faction at relation[7].between: one of ashen_circle, city_watch, free_company, \
         lantern_guild"
    );
}

#[test]
fn a_misspelt_key_is_in_a_group_of_its_own() {
    let texts = with("factions.toml", "");
    let mut texts = texts;
    let factions = texts[1].as_mut().expect("factions.toml");
    *factions = factions.replacen("tolerance = 40.0", "tolerence = 40.0", 1);
    let groups = entry_form(&texts, "factions.toml", "city_watch");
    let last = groups.last().expect("groups");
    assert_eq!(last.name, None);
    assert_eq!(last.rows[0].key, "tolerence");
}

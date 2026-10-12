//! Marked ids (U14c, P-95): what the session says, with each id of the loaded world in it
//! marked with what it names, so the editor links them without searching the text itself; the
//! CLI prints the same text, unmarked.

use factional_session::{Mark, Named, Session};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn said(session: &mut Session, line: &str) -> String {
    session.execute(line).expect("a command").render()
}

/// Each mark as the text it covers and what it names.
fn marked<'t>(text: &'t str, marks: &[Mark]) -> Vec<(&'t str, Named)> {
    marks
        .iter()
        .map(|mark| (&text[mark.start..mark.end], mark.named))
        .collect()
}

#[test]
fn each_id_said_is_marked_with_what_it_names() {
    let mut session = Session::new(REPO);
    said(&mut session, "load content/sample");
    let text = said(&mut session, "relations free_company");
    assert_eq!(
        marked(&text, &session.marks(&text)),
        [
            ("city_watch", Named::Faction),
            ("free_company", Named::Faction),
            ("free_company", Named::Faction),
            ("city_watch", Named::Faction),
            ("free_company", Named::Faction),
            ("lantern_guild", Named::Faction),
            ("lantern_guild", Named::Faction),
            ("free_company", Named::Faction),
        ]
    );
    let text = "vex steals from merchant_ava for watch_oath, an outcome of turned_in_vex; steal.";
    assert_eq!(
        marked(text, &session.marks(text)),
        [
            ("vex", Named::Character),
            ("merchant_ava", Named::Character),
            ("watch_oath", Named::Quest),
            ("turned_in_vex", Named::Outcome),
            ("steal", Named::Action),
        ]
    );
}

#[test]
fn only_whole_ids_are_marked() {
    let mut session = Session::new(REPO);
    said(&mut session, "load content/sample");
    // Within a longer word, or a key path's step that isn't an id, nothing is.
    let text = "vexing city_watchers, city_watch.relations.temple";
    assert_eq!(
        marked(text, &session.marks(text)),
        [("city_watch", Named::Faction), ("temple", Named::Faction)]
    );
}

#[test]
fn nothing_is_marked_without_a_world() {
    let session = Session::new(REPO);
    assert_eq!(session.marks("city_watch and vex"), []);
}

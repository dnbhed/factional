//! Completion (U15c, P-96): the last word typed, completed from the commands, or after the
//! first word from the loaded world's ids.

use factional_session::Session;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[test]
fn the_first_word_completes_to_a_command() {
    let session = Session::new(REPO);
    assert_eq!(session.complete("chara"), ["characters"]);
    let s = session.complete("s");
    assert!(
        s.contains(&"save".to_owned()) && s.contains(&"show".to_owned()),
        "{s:?}"
    );
    assert!(
        s.windows(2).all(|pair| pair[0] < pair[1]),
        "in order, once each: {s:?}"
    );
    assert!(s.iter().all(|command| command.starts_with('s')));
    assert_eq!(session.complete("zzz"), Vec::<String>::new());
}

#[test]
fn a_later_word_completes_to_an_id_of_the_world() {
    let mut session = Session::new(REPO);
    assert_eq!(
        session.complete("show character merch"),
        Vec::<String>::new(),
        "no world, no ids"
    );
    session.execute("load content/sample").expect("loads");
    assert_eq!(session.complete("show character merch"), ["merchant_ava"]);
    assert_eq!(session.complete("act player ste"), ["steal"]);
    assert_eq!(
        session.complete("relations city_watch t"),
        ["temple", "the_long_winter", "took_a_bribe", "turned_in_vex"]
    );
    // After a space, every id is a completion.
    assert!(session.complete("show character ").len() > 20);
}

//! The console (G2a, P-122): what was said in words and ids, completion, history, and its
//! line named for it. Its drawing in the editor is Play's, tested there (U15b, U15c, U19).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use factional_console::{Asked, Console, Piece, pieces, shared};
use factional_session::{Mark, Named};

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn what_was_said_is_its_words_and_its_ids_a_line_at_a_time() {
    let said = "vex steals\n\nfrom merchant_ava, for vex";
    let mark = |start: usize, end: usize| Mark {
        start,
        end,
        named: Named::Character,
    };
    let marks = [mark(0, 3), mark(17, 29), mark(35, 38)];
    assert_eq!(
        pieces(said, &marks),
        [
            vec![Piece::Id("vex", Named::Character), Piece::Words(" steals")],
            vec![],
            vec![
                Piece::Words("from "),
                Piece::Id("merchant_ava", Named::Character),
                Piece::Words(", for "),
                Piece::Id("vex", Named::Character),
            ],
        ]
    );
}

#[test]
fn what_completions_share_is_their_common_start() {
    assert_eq!(shared(&words(&["temple", "the_long_winter", "took"])), "t");
    assert_eq!(shared(&words(&["save", "saves"])), "save");
    assert_eq!(shared(&words(&["show"])), "show");
    assert_eq!(shared(&[]), "");
}

#[test]
fn one_completion_is_written_in_and_more_are_offered() {
    let mut console = Console::new("Play");
    console.set_input("show char");
    console.complete(&words(&["character"]));
    assert_eq!(console.input(), "show character ");
    assert_eq!(console.offered(), None);
    console.set_input("show character s");
    console.complete(&words(&["sister_mira", "steal"]));
    assert_eq!(console.input(), "show character s");
    assert_eq!(console.offered(), Some("sister_mira, steal"));
}

#[test]
fn tab_with_nothing_to_complete_to_leaves_what_was_typed() {
    let mut console = Console::new("Play");
    console.set_input("zzz");
    console.complete(&[]);
    assert_eq!(console.input(), "zzz");
    assert_eq!(console.offered(), None);
}

#[test]
fn the_arrows_go_back_and_forth_through_what_was_run() {
    let mut console = Console::new("Play");
    console.recall(false);
    assert_eq!(console.input(), "", "nothing run, nothing to recall");
    let mut harness = Harness::new_ui_state(
        |ui, console: &mut Console| {
            console.ui(ui, None, |_| Vec::new());
        },
        console,
    );
    for line in ["a", "b", "c"] {
        harness.get_by_label("Play command").focus();
        harness.run();
        harness.get_by_label("Play command").type_text(line);
        harness.key_press(egui::Key::Enter);
        harness.run();
    }
    let console = harness.state_mut();
    let mut walk = |down: bool| {
        console.recall(down);
        console.input().to_owned()
    };
    assert_eq!(walk(false), "c");
    assert_eq!(walk(false), "b");
    assert_eq!(walk(false), "a");
    assert_eq!(walk(false), "a", "the first stays");
    assert_eq!(walk(true), "b");
    assert_eq!(walk(true), "c");
    assert_eq!(walk(true), "", "after the last, nothing");
    assert_eq!(walk(true), "");
}

#[test]
fn what_is_entered_is_handed_back_to_run() {
    let mut asked = Vec::new();
    {
        let mut harness = Harness::new_ui_state(
            |ui, (console, asked): &mut (Console, Vec<String>)| {
                if let Some(line) = console.ui(ui, None, |_| Vec::new()).run {
                    asked.push(line);
                }
            },
            (Console::new("Play"), Vec::new()),
        );
        harness.get_by_label("Play command").focus();
        harness.run();
        harness
            .get_by_label("Play command")
            .type_text("  characters  ");
        harness.key_press(egui::Key::Enter);
        harness.run();
        asked.append(&mut harness.state_mut().1);
    }
    assert_eq!(asked, ["characters"]);
}

#[test]
fn a_console_names_its_line_by_its_name() {
    let mut harness = Harness::new_ui_state(
        |ui, console: &mut Console| {
            console.ui(ui, None, |_| Vec::new());
        },
        Console::new("Console"),
    );
    harness.run();
    harness.get_by_label("Console command");
    assert!(harness.query_by_label("Play command").is_none());
}

/// A console in a harness, with what it asked each frame kept beside it.
fn harness(console: Console, found: Vec<String>) -> Harness<'static, (Console, Vec<Asked>)> {
    Harness::new_ui_state(
        move |ui, (console, asked): &mut (Console, Vec<Asked>)| {
            let found = found.clone();
            let this = console.ui(ui, None, move |_| found.clone());
            if this != Asked::default() {
                asked.push(this);
            }
        },
        (console, Vec::new()),
    )
}

#[test]
fn what_is_said_is_logged_and_shown_after_its_command() {
    let mut console = Console::new("Play");
    let marks = vec![Mark {
        start: 0,
        end: 3,
        named: Named::Character,
    }];
    console.say(None, "loaded".to_owned(), Vec::new());
    console.say(
        Some("show character vex".to_owned()),
        "vex — Vex".to_owned(),
        marks.clone(),
    );
    assert_eq!(
        console.log(),
        [
            (None, "loaded".to_owned(), Vec::new()),
            (
                Some("show character vex".to_owned()),
                "vex — Vex".to_owned(),
                marks
            ),
        ]
    );
    let mut harness = harness(console, Vec::new());
    harness.run();
    harness.get_by_label("loaded");
    let command = harness.get_by_label("> show character vex").rect().top();
    let said = harness.get_by_label("Go to vex").rect().top();
    assert!(command < said, "the command comes before what it said");
}

#[test]
fn an_id_said_is_a_link_handed_back_when_followed() {
    let mut console = Console::new("Play");
    let marks = vec![Mark {
        start: 0,
        end: 3,
        named: Named::Character,
    }];
    console.say(Some("characters".to_owned()), "vex — Vex".to_owned(), marks);
    let mut harness = harness(console, Vec::new());
    harness.run();
    harness.get_by_label("Go to vex").click();
    harness.run();
    let followed: Vec<_> = harness
        .state()
        .1
        .iter()
        .filter_map(|asked| asked.followed.clone())
        .collect();
    assert_eq!(followed, [(Named::Character, "vex".to_owned())]);
}

#[test]
fn tab_completes_the_line_keeping_no_tab_in_it() {
    let mut harness = harness(Console::new("Play"), words(&["character"]));
    harness.get_by_label("Play command").focus();
    harness.run();
    harness.get_by_label("Play command").type_text("show char");
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(harness.state().0.input(), "show character ");
}

#[test]
fn enter_runs_only_what_is_typed_in_the_line() {
    let mut console = Console::new("Play");
    console.set_input("characters");
    let mut harness = harness(console, Vec::new());
    harness.run();
    // Enter pressed elsewhere, the line not in focus, runs nothing.
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(harness.state().1.is_empty());
    assert_eq!(harness.state().0.input(), "characters");
}

#[test]
fn a_console_asked_to_take_the_focus_puts_it_on_its_line() {
    let mut harness = harness(Console::new("Console"), Vec::new());
    harness.run();
    assert!(!harness.get_by_label("Console command").is_focused());
    harness.state_mut().0.focus();
    harness.run();
    assert!(harness.get_by_label("Console command").is_focused());
}

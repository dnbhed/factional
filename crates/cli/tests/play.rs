//! Playing quests in the REPL (Q6): `can-start`, `start`, `choose` and `progress`, against
//! Riverhold's quests in `content/sample`. Every number is worked out by hand from DESIGN.md's
//! formulas; see PLAN.md's Q6.

use factional_cli::Session;

/// A session with `content/sample` loaded.
fn riverhold() -> Session {
    let mut session = Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    run(&mut session, "load content/sample");
    session
}

/// What the command prints; an unknown command shows as its script error.
fn run(session: &mut Session, command: &str) -> String {
    match session.execute(command) {
        Ok(outcome) => outcome.render(),
        Err(error) => format!("script error: {error}"),
    }
}

/// Runs each command, checking that none fails.
fn play(session: &mut Session, commands: &[&str]) {
    for command in commands {
        let shown = run(session, command);
        assert!(
            !shown.starts_with("error") && !shown.starts_with("script error"),
            "{command}: {shown}"
        );
    }
}

#[test]
fn a_quest_starts_once_its_gate_and_its_step_allow() {
    let mut session = riverhold();
    assert_eq!(
        run(&mut session, "can-start player watch_oath"),
        "player can start watch_oath"
    );
    assert_eq!(
        run(&mut session, "can-start player watch_captain"),
        "player can't start watch_captain:\n\
         - step 1 of watch_career isn't complete: 0 of 1 done\n\
         - it needs standing 40.00 with city_watch, and player has 0.00\n\
         - it needs rank sergeant or higher in city_watch, and player isn't in city_watch"
    );
    assert_eq!(
        run(&mut session, "start player watch_oath"),
        "player started watch_oath\nplayer reached watch_oath.patrol"
    );
    assert_eq!(
        run(&mut session, "start player watch_oath"),
        "error: player can't start watch_oath:\n- player has already started it"
    );
}

#[test]
fn a_choice_applies_its_effects_then_leads_on() {
    let mut session = riverhold();
    play(&mut session, &["start player watch_oath"]);
    let shown = run(&mut session, "choose player watch_oath report");
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines.first(),
        Some(&"player chose watch_oath.patrol.report")
    );
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("outcome turned_in_vex applied to player")),
        "{shown}"
    );
    assert_eq!(lines.last(), Some(&"player reached watch_oath.oath"));
    // City_watch +15, and 4.50 spilt from lantern_guild's -25 (it regards the Guild at -80,
    // where spillover is -0.18); the Guild -25 and -2.70 from the Watch's +15; the Temple
    // 1.50 (it regards the Watch at 60, where spillover is 0.10).
    assert_eq!(run(&mut session, "standing player city_watch"), "19.50");
    assert_eq!(run(&mut session, "standing player lantern_guild"), "-27.70");
    assert_eq!(run(&mut session, "standing player temple"), "1.50");
    assert_eq!(
        run(&mut session, "show character player"),
        "player — The Player — law 4.00, good 0.00 — True Neutral"
    );
}

#[test]
fn a_character_waits_at_a_stage_until_its_requirements_hold() {
    let mut session = riverhold();
    play(
        &mut session,
        &[
            "start player watch_oath",
            "choose player watch_oath report",
            "outcome fined_by_watch player",
        ],
    );
    // 19.50 - 20.
    assert_eq!(run(&mut session, "standing player city_watch"), "-0.50");
    assert_eq!(
        run(&mut session, "choose player watch_oath swear"),
        "error: player can't choose at watch_oath.oath yet: it needs standing 10.00 with city_watch, and player has -0.50"
    );
    play(
        &mut session,
        &[
            "act player report_crime",
            "act player report_crime",
            "act player report_crime",
        ],
    );
    assert_eq!(run(&mut session, "standing player city_watch"), "14.50");
    let shown = run(&mut session, "choose player watch_oath swear");
    assert_eq!(
        shown.lines().last(),
        Some("player finished watch_oath"),
        "{shown}"
    );
    // Law 4, then 4 for each report, then 5 for swearing; good 1 for each report.
    assert_eq!(
        run(&mut session, "show character player"),
        "player — The Player — law 21.00, good 3.00 — True Neutral"
    );
}

/// The player through the oath and two odd jobs, finishing the questline's second step.
fn two_odd_jobs() -> Session {
    let mut session = riverhold();
    play(
        &mut session,
        &[
            "start player watch_oath",
            "choose player watch_oath report",
            "choose player watch_oath swear",
            "start player night_patrol",
            "choose player night_patrol by_the_book",
            "start player dock_inspection",
            "choose player dock_inspection seize",
        ],
    );
    session
}

#[test]
fn starting_a_later_step_closes_the_leftovers_not_yet_started() {
    let mut session = two_odd_jobs();
    assert_eq!(
        run(&mut session, "start player harbour_errands"),
        "player started harbour_errands\n\
         player reached harbour_errands.run\n\
         smugglers_cove closed for player: watch_career moved on from step 2"
    );
    assert_eq!(
        run(&mut session, "can-start player smugglers_cove"),
        "player can't start smugglers_cove:\n- it closed when watch_career moved on from step 2"
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "dock_inspection — finished\n\
         harbour_errands — at run\n\
         night_patrol — finished\n\
         smugglers_cove — closed when watch_career moved on from step 2\n\
         watch_oath — finished\n\
         watch_career — up to step 4 of 4"
    );
}

#[test]
fn progress_says_when_a_character_has_started_nothing() {
    let mut session = riverhold();
    assert_eq!(
        run(&mut session, "progress player"),
        "player hasn't started any quests"
    );
}

#[test]
fn choices_are_refused_with_the_reason() {
    let mut session = two_odd_jobs();
    assert_eq!(
        run(&mut session, "choose player watch_oath swear"),
        "error: player has finished watch_oath"
    );
    assert_eq!(
        run(&mut session, "choose player lost_ring return_it"),
        "error: player hasn't started lost_ring"
    );
    play(&mut session, &["start player lost_ring"]);
    assert_eq!(
        run(&mut session, "choose player lost_ring retrun_it"),
        "error: unknown choice 'retrun_it' at lost_ring.search (did you mean 'return_it'?)"
    );
    assert_eq!(
        run(&mut session, "start player watch_oth"),
        "error: unknown quest 'watch_oth' (did you mean 'watch_oath'?)"
    );
}

#[test]
fn a_refused_choice_changes_nothing() {
    let mut session = riverhold();
    play(&mut session, &["start player lost_ring"]);
    let before = run(&mut session, "events");
    let refused = run(
        &mut session,
        "choose player lost_ring return_it --seen-by nobody_here",
    );
    assert!(refused.starts_with("error: "), "{refused}");
    assert_eq!(run(&mut session, "events"), before);
    assert_eq!(
        run(&mut session, "progress player"),
        "lost_ring — at search"
    );
}

#[test]
fn choose_says_who_saw_it_as_outcome_does() {
    let mut session = riverhold();
    play(
        &mut session,
        &[
            "start player lost_ring",
            "choose player lost_ring pawn_it --unseen",
            "start player the_long_winter",
            "choose player the_long_winter hoard --seen-by vex,merchant_ava",
        ],
    );
    assert_eq!(
        run(&mut session, "progress player"),
        "lost_ring — finished\nthe_long_winter — finished"
    );
    let usage = "error: choose needs the form: choose <character> <quest> <choice> [--seen-by <id>,... | --unseen]";
    assert_eq!(
        run(
            &mut session,
            "choose player lost_ring pawn_it --seen-by --unseen"
        ),
        usage
    );
    assert_eq!(run(&mut session, "choose player lost_ring"), usage);
    assert_eq!(
        run(
            &mut session,
            "choose player lost_ring pawn_it --seen-by Vex"
        ),
        "error: 'Vex' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
    );
}

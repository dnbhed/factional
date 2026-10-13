//! Regard on the tokens (G6a): how each character regards the one played, from their own
//! picture of them, so news unseen changes nothing until it arrives (DESIGN.md §10.2).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::time::TimeUpdateStrategy;
use factional_bevy::{Factional, FactionalCommand, FactionalPlugin};
use factional_builder::{BuilderPlugin, Regards, regards};
use factional_core::Fixed;
use factional_reputation::{ActionId, CharacterId, Command, Observer, Witnesses, World};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn riverhold() -> World {
    let (content, _) = factional_content::load_quests(&sample()).expect("loads");
    World::new(content).expect("complete")
}

/// DESIGN.md §10.2: the player picks Captain Hale's pocket, seen only by Merchant Ava.
fn pickpocket() -> Command {
    Command::PerformAction {
        actor: id("player"),
        action: ActionId::new("steal").expect("an id"),
        target: Some(id("captain_hale")),
        scale: Fixed::ONE,
        witnesses: Witnesses::These(BTreeSet::from([id("merchant_ava")])),
    }
}

#[test]
fn each_regards_the_played_as_their_disposition_says() {
    let world = riverhold();
    let found = regards(&world, Some(&id("player")));
    // DESIGN.md §8: Hale toward the player.
    assert_eq!(found[&id("captain_hale")].said(), "-5.49, neutral");
    assert!(
        !found.contains_key(&id("player")),
        "no regard of themselves"
    );
    assert_eq!(found.len(), 5);
    for (character, regard) in &found {
        let disposition = world
            .disposition(&Observer::Character(character.clone()), &id("player"))
            .expect("a disposition");
        assert_eq!(
            (regard.score, regard.band.as_str()),
            (disposition.score, disposition.band.as_str())
        );
        let bands: Vec<&str> = world
            .balance()
            .bands
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(regard.bands, bands.len());
        assert_eq!(bands[regard.index], regard.band);
    }
    assert!(regards(&world, None).is_empty());
}

#[test]
fn a_theft_unseen_changes_hales_regard_only_when_the_news_reaches_him() {
    let mut world = riverhold();
    let player = id("player");
    let before = regards(&world, Some(&player))[&id("captain_hale")].score;
    world.execute(pickpocket()).expect("steals");
    assert_eq!(
        regards(&world, Some(&player))[&id("captain_hale")].score,
        before,
        "at tick 0 Hale doesn't know"
    );
    world
        .execute(Command::AdvanceTime { ticks: 10 })
        .expect("time passes");
    assert!(
        regards(&world, Some(&player))[&id("captain_hale")].score < before,
        "at tick 10 he's heard"
    );
}

#[test]
fn regards_follow_the_world_and_stay_put_when_nothing_changes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: "player".to_owned(),
        });
    app.update();
    let hale = |app: &App| app.world().resource::<Regards>().0[&id("captain_hale")].clone();
    assert_eq!(hale(&app).said(), "-5.49, neutral");
    let changed = |app: &App| {
        app.world()
            .get_resource_change_ticks::<Regards>()
            .expect("regards")
            .changed
    };
    let before = changed(&app);
    app.update();
    assert_eq!(changed(&app), before, "a frame that changes nothing");
    for command in [pickpocket(), Command::AdvanceTime { ticks: 10 }] {
        app.world_mut()
            .write_message(FactionalCommand::World(command));
        app.update();
    }
    let world = &app.world().resource::<Factional>().world;
    let now = world
        .disposition(&Observer::Character(id("captain_hale")), &id("player"))
        .expect("a disposition");
    assert_eq!(hale(&app).score, now.score, "current after the commands");
    assert!(now.score < Fixed::from_hundredths(-549), "{}", now.score);
}

//! What came out differently between two runs: `factional compare` and `reload` (DESIGN.md
//! §12.3).

use std::collections::BTreeMap;
use std::path::Path;

use factional_reputation::{Axis, CharacterId, Event, World};

use crate::session::{
    Outcome, ScriptError, Session, describe_event, is_blank_or_comment, split_assert, split_command,
};

/// `factional compare`: runs the scenario `source`, from the file `scenario`, once on the
/// content in `content` and once on `against`, each read from `base`, and reports what came
/// out differently. The scenario must load exactly one world; that load is what's swapped.
/// Asserts run without being checked, since the two worlds are expected to disagree, and a
/// command that fails is part of the result, not a reason to stop. An error says why the
/// two couldn't be compared.
pub fn compare(
    scenario: &str,
    source: &str,
    content: &str,
    against: &str,
    base: &Path,
) -> Result<String, String> {
    let loads = source
        .lines()
        .map(str::trim)
        .filter(|line| !is_blank_or_comment(line) && split_command(line).0 == "load")
        .count();
    let loads = match loads {
        1 => None,
        0 => Some("no world".to_owned()),
        many => Some(format!("{many} worlds")),
    };
    if let Some(loads) = loads {
        return Err(format!(
            "{scenario} loads {loads}; compare needs a scenario that loads exactly one"
        ));
    }
    let before = run_on(source, base, "--content", content)?;
    let after = run_on(source, base, "--against", against)?;
    let lines = report(
        (content, before.world().expect("the scenario loaded")),
        (against, after.world().expect("the scenario loaded")),
    );
    Ok(format!(
        "comparing {content} → {against} over {scenario}\n{}\n",
        lines.join("\n")
    ))
}

/// Runs the scenario with `dir` in place of the directory its `load` names; `flag` says which
/// side this is, in any error.
fn run_on(source: &str, base: &Path, flag: &str, dir: &str) -> Result<Session, String> {
    let mut session = Session::new(base);
    for (index, line) in source.lines().map(str::trim).enumerate() {
        if is_blank_or_comment(line) {
            continue;
        }
        let (name, rest) = split_command(line);
        let ran = match name {
            "load" => match session.execute(&format!("load {dir}")) {
                Ok(Outcome::Error(problems)) => {
                    return Err(format!("{flag} {dir} doesn't load:\n{problems}"));
                }
                ran => ran,
            },
            "assert" => match split_assert(rest) {
                Some((command, _)) => session.execute(command),
                None => Err(ScriptError::MalformedAssert),
            },
            _ => session.execute(line),
        };
        match ran {
            Ok(Outcome::Quit) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("{flag} {dir}: line {}: {error}", index + 1)),
        }
    }
    Ok(session)
}

/// What's compared about each character, in the order it's listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Law,
    Good,
    /// How a faction or character regards them.
    Standing,
    /// A faction they're in, and their rank there.
    Membership,
    /// An observer's disposition toward them, while they're watched.
    Watched,
}

/// One thing compared: whose it is, what kind, and which faction, party or observer.
type Key = (CharacterId, Kind, String);

/// Everything compared in `world`, each with its value as shown.
fn snapshot(world: &World) -> BTreeMap<Key, String> {
    let mut values = BTreeMap::new();
    for character in world.characters() {
        let id = &character.id;
        let alignment = world.alignment(id).expect("a character has an alignment");
        values.insert(
            (id.clone(), Kind::Law, String::new()),
            alignment.on(Axis::Law).to_string(),
        );
        values.insert(
            (id.clone(), Kind::Good, String::new()),
            alignment.on(Axis::Good).to_string(),
        );
        for (party, value) in world.standings(id).into_iter().flatten() {
            values.insert(
                (id.clone(), Kind::Standing, party.to_string()),
                value.to_string(),
            );
        }
        for (faction, membership) in world.memberships(id).into_iter().flatten() {
            values.insert(
                (id.clone(), Kind::Membership, faction.to_string()),
                membership.rank.to_string(),
            );
        }
    }
    for (subject, observers) in world.watched() {
        for (observer, band) in observers {
            let score = world
                .disposition(observer, subject)
                .expect("watched by someone who exists")
                .score;
            values.insert(
                (subject.clone(), Kind::Watched, observer.to_string()),
                format!("{score} {band}"),
            );
        }
    }
    values
}

/// What a missing value shows as: no standing is 0, no membership is `no`.
fn absent(kind: Kind) -> &'static str {
    match kind {
        Kind::Law | Kind::Good | Kind::Standing => "0.00",
        Kind::Membership => "no",
        Kind::Watched => "not watched",
    }
}

fn label((character, kind, which): &Key) -> String {
    match kind {
        Kind::Law => format!("{character} law"),
        Kind::Good => format!("{character} good"),
        Kind::Standing => format!("{character} standing with {which}"),
        Kind::Membership => format!("{character} in {which}"),
        Kind::Watched => format!("{which} toward {character}"),
    }
}

/// The differences in what two worlds hold now, one per line as `<what>: <before> → <after>`:
/// each character's alignment, standings and memberships, in id order, and each watcher's
/// disposition toward the subjects they watch.
pub(crate) fn differences(before: &World, after: &World) -> Vec<String> {
    let (before, after) = (snapshot(before), snapshot(after));
    let mut keys: Vec<&Key> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|key| {
            let shown = |values: &BTreeMap<Key, String>| {
                values
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| absent(key.1).to_owned())
            };
            let (was, now) = (shown(&before), shown(&after));
            (was != now).then(|| format!("{}: {was} → {now}", label(key)))
        })
        .collect()
}

/// The report on two runs: their differences, then where their events first diverge, each
/// side's event under its label; or `no differences`.
pub(crate) fn report(before: (&str, &World), after: (&str, &World)) -> Vec<String> {
    let mut lines = differences(before.1, after.1);
    let (earlier, later) = (before.1.events(), after.1.events());
    if let Some(index) = first_difference(earlier, later) {
        let event = |events: &[Event]| {
            events
                .get(index)
                .map_or_else(|| "no event".to_owned(), describe_event)
        };
        lines.push(format!("first difference at event #{}:", index + 1));
        lines.push(format!("  {}: {}", before.0, event(earlier)));
        lines.push(format!("  {}: {}", after.0, event(later)));
    }
    if lines.is_empty() {
        lines.push("no differences".to_owned());
    }
    lines
}

/// Where two event logs first differ, as an index; `None` if they're the same.
fn first_difference(before: &[Event], after: &[Event]) -> Option<usize> {
    (0..before.len().max(after.len())).find(|&index| before.get(index) != after.get(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    /// A session on content/sample after `commands`, each of which must succeed.
    fn after(commands: &[&str]) -> Session {
        let mut session = Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        for command in std::iter::once("load content/sample").chain(commands.iter().copied()) {
            let outcome = session.execute(command).expect("a known command");
            assert!(
                matches!(outcome, crate::Outcome::Output(_)),
                "{command}: {outcome:?}"
            );
        }
        session
    }

    fn world(session: &Session) -> &World {
        session.world().expect("a world")
    }

    #[test]
    fn lists_changed_alignment_standing_and_membership_by_character() {
        let before = after(&[]);
        // Helping Ava moves the player 4.00 toward good and raises her regard by 10.00; she's
        // in no faction, so nothing spills.
        let later = after(&[
            "join player free_company",
            "act player help_stranger --target merchant_ava",
        ]);
        assert_eq!(
            differences(world(&before), world(&later)),
            [
                "player good: 0.00 → 4.00",
                "player standing with merchant_ava: 0.00 → 10.00",
                "player in free_company: no → sellsword",
            ]
        );
        assert_eq!(
            differences(world(&later), world(&before)),
            [
                "player good: 4.00 → 0.00",
                "player standing with merchant_ava: 10.00 → 0.00",
                "player in free_company: sellsword → no",
            ]
        );
        assert_eq!(
            differences(world(&later), world(&later)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn lists_changed_dispositions_toward_watched_subjects() {
        // Two thefts and the Watch's fine leave Hale at -29.10 toward the player; a bribe of
        // 20.00 lifts him to -9.10, from unfriendly to neutral (M10).
        let fined = [
            "act player steal --target merchant_ava",
            "act player steal --target merchant_ava",
            "outcome fined_by_watch player",
            "watch player",
        ];
        let before = after(&fined);
        let mut bribed = fined.to_vec();
        bribed.push("modify captain_hale player bribed 20");
        let later = after(&bribed);
        assert_eq!(
            differences(world(&before), world(&later)),
            ["captain_hale toward player: -29.10 unfriendly → -9.10 neutral"]
        );
        // Unwatched, the same bribe makes no difference to report.
        let unwatched = after(&fined[..3]);
        let mut quiet = fined[..3].to_vec();
        quiet.push("modify captain_hale player bribed 20");
        assert_eq!(
            differences(world(&unwatched), world(&after(&quiet))),
            Vec::<String>::new()
        );
    }

    #[test]
    fn reports_where_the_events_first_diverge() {
        let one = after(&["act player steal --target merchant_ava"]);
        let two = after(&["act player steal --target merchant_ava", "advance 5"]);
        // The first three events match; the second run has a fourth.
        assert_eq!(
            report(("one", world(&one)), ("two", world(&two))),
            [
                "first difference at event #4:",
                "  one: no event",
                "  two: #4 at tick 0: time advanced from 0 to 5",
            ]
        );
        assert_eq!(
            report(("two", world(&two)), ("one", world(&one))),
            [
                "first difference at event #4:",
                "  two: #4 at tick 0: time advanced from 0 to 5",
                "  one: no event",
            ]
        );
        assert_eq!(
            report(("one", world(&one)), ("again", world(&one))),
            ["no differences"]
        );
    }
}

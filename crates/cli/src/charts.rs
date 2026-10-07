//! Pictures of a world for tuning (DESIGN.md §12.3): `map` draws who could join a faction,
//! `matrix` how everyone regards someone, and `curve` what a knob does across its range.

use std::collections::{BTreeMap, BTreeSet};

use factional_core::{Curve, Fixed, ParseFixedError};
use factional_reputation::{Alignment, Observer, TargetCurve, Toward, World};

use crate::session::{Outcome, hint, lines, no_world};

/// The map's cells are 10 apart on each axis, from -100 to 100: 21 by 21.
const CELLS: i64 = 10;

/// Which cell along an axis a value falls in, from 0 at -100 to 20 at 100: the nearest 10,
/// halves away from zero.
fn cell(value: Fixed) -> i64 {
    let hundredths = value.hundredths();
    let tens = (hundredths.abs() + 500) / 1000;
    hundredths.signum() * tens + CELLS
}

/// A cell's centre on an axis, such as -100 for cell 0.
fn centre(index: i64) -> i64 {
    (index - CELLS) * 10
}

/// The letter for the `index`th character on the map: A to Z, then a to z, then `?`.
fn letter(index: usize) -> char {
    let letters: Vec<char> = ('A'..='Z').chain('a'..='z').collect();
    letters.get(index).copied().unwrap_or('?')
}

/// `map <faction>`: the alignment plane, law across and good up, marking the cells within
/// the faction's tolerance, the faction, and each character where it pictures them, with a
/// key (P-53, DESIGN.md §10.3).
pub(crate) fn map(world: &World, args: &str) -> Outcome {
    let id = match args.split_whitespace().collect::<Vec<_>>()[..] {
        [id] => id,
        _ => return Outcome::Error("map needs the form: map <faction>".to_owned()),
    };
    let Some(faction) = world.factions().find(|faction| faction.id.as_str() == id) else {
        let ids = world
            .factions()
            .map(|faction| faction.id.as_str())
            .collect();
        return Outcome::Error(format!("unknown faction '{id}'{}", hint(id, ids)));
    };
    let at = world
        .faction_alignment(&faction.id)
        .expect("a faction has an alignment");
    let tolerance = faction.tolerances.tolerance();
    let mut marks: BTreeMap<(i64, i64), Vec<char>> = BTreeMap::new();
    let mut place = |alignment: Alignment, mark: char| {
        let row = 2 * CELLS - cell(alignment.on(factional_reputation::Axis::Good));
        let column = cell(alignment.on(factional_reputation::Axis::Law));
        marks.entry((row, column)).or_default().push(mark);
    };
    place(at, '@');
    let observer = Observer::Faction(faction.id.clone());
    let mut key = Vec::new();
    // Each character where the faction pictures them (DESIGN.md §10.3).
    for (index, character) in world.characters().enumerate() {
        let mark = letter(index);
        let measured = world
            .distance(&observer, &character.id)
            .expect("both exist");
        place(measured.subject, mark);
        let distance = measured.value;
        let within = if distance <= tolerance {
            "within"
        } else {
            "outside"
        };
        let pictured = if measured.subject == measured.truth {
            String::new()
        } else {
            let axes = |alignment: Alignment| {
                format!(
                    "law {}, good {}",
                    alignment.on(factional_reputation::Axis::Law),
                    alignment.on(factional_reputation::Axis::Good)
                )
            };
            format!(
                "; pictured at {}, truly {}",
                axes(measured.subject),
                axes(measured.truth)
            )
        };
        key.push(format!(
            "{mark} {}: {distance} away, {within}{pictured}",
            character.id
        ));
    }
    let mut output = vec![format!(
        "{} ({}): law {}, good {}, tolerance {tolerance}",
        faction.name,
        faction.id,
        at.on(factional_reputation::Axis::Law),
        at.on(factional_reputation::Axis::Good)
    )];
    for row in 0..=2 * CELLS {
        let good = -centre(row);
        let cells: Vec<String> = (0..=2 * CELLS)
            .map(
                |column| match marks.get(&(row, column)).map(Vec::as_slice) {
                    Some([mark]) => mark.to_string(),
                    Some(_) => "*".to_owned(),
                    None => {
                        let point = Alignment::new(
                            Fixed::from_hundredths(centre(column) * 100),
                            Fixed::from_hundredths(good * 100),
                        )
                        .expect("every cell is on the plane");
                        let distance = world
                            .distance_to_point(&faction.id, point)
                            .expect("the faction exists");
                        if distance <= tolerance { "+" } else { "." }.to_owned()
                    }
                },
            )
            .collect();
        let label = if good % 50 == 0 {
            good.to_string()
        } else {
            String::new()
        };
        output.push(format!("{label:>4} {}", cells.join(" ")));
    }
    output.push(format!("{:5}{:<19}{:^3}{:>19}", "", "-100", "law", "100"));
    output.push(format!(
        "@ {}, + within its tolerance, . outside; law runs across, good up",
        faction.name
    ));
    output.extend(key);
    for ((row, column), shared) in &marks {
        if shared.len() > 1 {
            let shared: Vec<String> = shared.iter().map(char::to_string).collect();
            output.push(format!(
                "* at law {}, good {}: {}",
                centre(*column),
                -centre(*row),
                shared.join(", ")
            ));
        }
    }
    Outcome::Output(output.join("\n"))
}

/// `matrix [<subject>...] [--csv]`: every observer's disposition score toward each subject,
/// observers as rows, factions first; every character if no subject is named. An observer's
/// view of themselves is left blank.
pub(crate) fn matrix(world: &World, args: &str) -> Outcome {
    let words: Vec<&str> = args.split_whitespace().collect();
    let csv = words.contains(&"--csv");
    let mut subjects = Vec::new();
    for word in words.into_iter().filter(|word| *word != "--csv") {
        match world
            .characters()
            .find(|character| character.id.as_str() == word)
        {
            Some(character) => subjects.push(character.id.clone()),
            None if world.factions().any(|faction| faction.id.as_str() == word) => {
                return Outcome::Error("a matrix's subjects must be characters".to_owned());
            }
            None => {
                let ids = world.characters().map(|c| c.id.as_str()).collect();
                return Outcome::Error(format!("unknown subject '{word}'{}", hint(word, ids)));
            }
        }
    }
    if subjects.is_empty() {
        subjects = world
            .characters()
            .map(|character| character.id.clone())
            .collect();
    }
    let observers = world
        .factions()
        .map(|faction| Observer::Faction(faction.id.clone()))
        .chain(
            world
                .characters()
                .map(|character| Observer::Character(character.id.clone())),
        );
    let mut rows = vec![
        std::iter::once("observer".to_owned())
            .chain(subjects.iter().map(ToString::to_string))
            .collect::<Vec<_>>(),
    ];
    for observer in observers {
        let mut row = vec![observer.to_string()];
        for subject in &subjects {
            let themselves = observer == Observer::Character(subject.clone());
            row.push(match (themselves, csv) {
                (true, true) => String::new(),
                (true, false) => "—".to_owned(),
                (false, _) => world
                    .disposition(&observer, subject)
                    .expect("both exist")
                    .score
                    .to_string(),
            });
        }
        rows.push(row);
    }
    if csv {
        return Outcome::Output(lines(rows.into_iter().map(|row| row.join(","))));
    }
    let widths: Vec<usize> = (0..rows[0].len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    Outcome::Output(lines(rows.into_iter().map(|row| {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(column, text)| {
                let pad = widths[column] - text.chars().count();
                if column == 0 {
                    format!("{text}{}", " ".repeat(pad))
                } else {
                    format!("{}{text}", " ".repeat(pad))
                }
            })
            .collect();
        cells.join("  ").trim_end().to_owned()
    })))
}

/// Every named knob that's a curve, with its curve; `None` for one left out, which is 1.00
/// everywhere.
fn knobs(world: &World) -> Vec<(String, Option<Curve>)> {
    let balance = world.balance();
    let mut knobs = vec![
        (
            "disposition.affinity".to_owned(),
            Some(balance.affinity.clone()),
        ),
        (
            "standing.spillover".to_owned(),
            Some(balance.spillover.clone()),
        ),
    ];
    for (profile, curves) in &balance.inertia.profiles {
        for toward in Toward::ALL {
            knobs.push((
                format!("inertia.{profile}.{}.{}", toward.axis().key(), toward.key()),
                curves.curves.get(&toward).cloned(),
            ));
        }
    }
    for action in world.actions() {
        for which in TargetCurve::ALL {
            knobs.push((
                format!("{}.by_target.{}", action.id, which.key()),
                action.by_target.get(&which).cloned(),
            ));
        }
    }
    knobs
}

/// `curve <curve> [at <x>]`: a curve's value at `x`, or without `at`, the whole curve as a
/// table. `<curve>` is a named knob, such as `disposition.affinity`, or a curve written as in
/// a content file, to try a shape before using it (DESIGN.md §4.2).
pub(crate) fn curve(world: Option<&World>, args: &str) -> Outcome {
    match describe_curve(world, args) {
        Ok(text) => Outcome::Output(text),
        Err(failed) => failed,
    }
}

fn describe_curve(world: Option<&World>, args: &str) -> Result<String, Outcome> {
    const USAGE: &str = "curve needs the form: curve <curve> [at <x>], where <curve> is a knob, such as disposition.affinity, or a curve, such as [[0, 1.0], [100, 0.5]]";
    let usage = || Outcome::Error(USAGE.to_owned());
    let args = args.trim();
    if args.is_empty() || args == "at" || args.starts_with("at ") {
        return Err(usage());
    }
    let (spec, x) = match args.rsplit_once(" at ") {
        Some((spec, x)) => (spec.trim(), Some(x.trim())),
        None => (args, None),
    };
    let (name, curve) = if spec.starts_with(|c: char| c.is_ascii_lowercase()) {
        let world = world.ok_or_else(no_world)?;
        let knobs = knobs(world);
        let Some((name, curve)) = knobs.iter().find(|(name, _)| name == spec) else {
            let names = knobs.iter().map(|(name, _)| name.as_str()).collect();
            return Err(Outcome::Error(format!(
                "unknown curve '{spec}'{}",
                hint(spec, names)
            )));
        };
        (Some(name.clone()), curve.clone())
    } else {
        let curve = factional_content::parse_curve(spec).map_err(Outcome::Error)?;
        (None, Some(curve))
    };
    if let Some(x) = x {
        let x: Fixed = x
            .parse()
            .map_err(|error: ParseFixedError| Outcome::Error(error.to_string()))?;
        let curve = curve.unwrap_or_else(|| Curve::constant(Fixed::ONE));
        return Ok(curve.at(x).to_string());
    }
    let title = name
        .as_ref()
        .map(|name| format!("{name}: "))
        .unwrap_or_default();
    let Some(curve) = curve else {
        return Ok(format!("{title}left out, so {} everywhere", Fixed::ONE));
    };
    let points = curve.points();
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Ok(format!("{title}{} everywhere", curve.at(Fixed::ZERO)));
    };
    // Every point, and every multiple of 10 between the first and the last.
    const TEN: i64 = 1000;
    let mut xs: BTreeSet<Fixed> = points.iter().map(|(x, _)| *x).collect();
    let tens = first.0.hundredths().div_euclid(TEN)..=last.0.hundredths().div_euclid(TEN);
    xs.extend(
        tens.map(|tens| Fixed::from_hundredths(tens * TEN))
            .filter(|x| *x >= first.0),
    );
    let mut table = Vec::new();
    if let Some(name) = name {
        table.push(format!("{name}:"));
    }
    table.push(format!("{:>8} {:>8}", "x", "y"));
    for x in xs {
        let point = if points.iter().any(|(px, _)| *px == x) {
            "  point"
        } else {
            ""
        };
        table.push(format!(
            "{:>8} {:>8}{point}",
            x.to_string(),
            curve.at(x).to_string()
        ));
    }
    Ok(table.join("\n"))
}

#[cfg(test)]
mod tests {
    use crate::{Outcome, Session};

    fn session(world: &str) -> Session {
        let mut session = Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let loaded = session
            .execute(&format!("load {world}"))
            .expect("a command");
        assert!(matches!(loaded, Outcome::Output(_)), "{loaded:?}");
        session
    }

    fn run(session: &mut Session, command: &str) -> String {
        session.execute(command).expect("a known command").render()
    }

    /// The row of `map` output for `good`, as its cells.
    fn row(map: &str, good: i64) -> Vec<char> {
        let label = format!("{good:>4} ");
        let index = usize::try_from((100 - good) / 10).expect("on the grid") + 1;
        let line = map.lines().nth(index).expect("a row");
        assert!(good % 50 != 0 || line.starts_with(&label), "{line}");
        line[5..]
            .split(' ')
            .map(|cell| cell.chars().next().expect("a cell"))
            .collect()
    }

    #[test]
    fn map_draws_the_tolerance_region_the_faction_and_each_character() {
        let mut session = session("crates/cli/tests/fixtures/worlds/ring");
        let map = run(&mut session, "map ring");
        // With weights of 1 and the straight-line metric, a cell is within 30.00 when
        // law² + good² ≤ 900. Ada (A) is at 30 / 0, Bo (B) shares the Ring's cell, and Cy
        // (C) is in the top-left corner.
        let mut expected = vec!["The Ring (ring): law 0.00, good 0.00, tolerance 30.00".to_owned()];
        for good in (-10..=10).rev().map(|g: i64| g * 10) {
            let cells: Vec<String> = (-10..=10)
                .map(|l: i64| l * 10)
                .map(|law| {
                    match (law, good) {
                        (-100, 100) => "C",
                        (0, 0) => "*",
                        (30, 0) => "A",
                        _ if law * law + good * good <= 900 => "+",
                        _ => ".",
                    }
                    .to_owned()
                })
                .collect();
            let label = if good % 50 == 0 {
                good.to_string()
            } else {
                String::new()
            };
            expected.push(format!("{label:>4} {}", cells.join(" ")));
        }
        expected.extend([
            "     -100               law                100".to_owned(),
            "@ The Ring, + within its tolerance, . outside; law runs across, good up".to_owned(),
            "A ada: 30.00 away, within".to_owned(),
            "B bo: 0.00 away, within".to_owned(),
            "C cy: 141.42 away, outside".to_owned(),
            "* at law 0, good 0: @, B".to_owned(),
        ]);
        assert_eq!(map, expected.join("\n"));
        // Exactly at the tolerance is within, as it is for joining: (30, 0) is 30.00 away and
        // (20, 20) 28.28, but (30, 10) is 31.62 and (40, 0) 40.00.
        assert_eq!(row(&map, 20)[12], '+');
        assert_eq!(row(&map, 10)[13], '.');
        assert_eq!(row(&map, 0)[14], '.');
    }

    #[test]
    fn map_places_riverholds_characters_inside_and_outside_the_watch() {
        let mut session = session("content/sample");
        let map = run(&mut session, "map city_watch");
        assert!(
            map.starts_with(
                "The City Watch (city_watch): law 70.00, good 20.00, tolerance 40.00\n"
            ),
            "{map}"
        );
        // In id order: brother_ash A, captain_hale B, merchant_ava C, player D, sister_mira
        // E, vex F. Hale's 75 / 30 rounds to the cell at 80 / 30; the Watch is at 70 / 20.
        assert_eq!(row(&map, 30)[18], 'B');
        assert_eq!(row(&map, 20)[17], '@');
        assert_eq!(row(&map, 0)[10], 'D');
        // The Watch weighs good by a quarter, so its region runs the height of the plane.
        assert_eq!(row(&map, 100)[17], '+');
        assert_eq!(row(&map, -100)[17], '+');
        assert_eq!(row(&map, 20)[12], '.', "50 away on law");
        assert!(
            map.contains("\nB captain_hale: 5.59 away, within\n"),
            "{map}"
        );
        assert!(map.contains("\nD player: 70.18 away, outside\n"), "{map}");
    }

    #[test]
    fn map_places_each_character_where_the_faction_pictures_them() {
        let mut session = session("content/sample");
        // Three thefts no one saw: the player is truly at −15 / −9, but the Guild still
        // pictures 0 / 0, 60.21 away (law Δ60, good Δ10 × 0.5).
        for _ in 0..3 {
            run(&mut session, "act player steal --unseen");
        }
        let map = run(&mut session, "map lantern_guild");
        assert_eq!(row(&map, 0)[10], 'D', "{map}");
        assert_eq!(
            row(&map, -10)[8],
            '+',
            "not where the player truly is, which is within the Guild's tolerance"
        );
        assert!(
            map.contains(
                "\nD player: 60.21 away, outside; pictured at law 0.00, good 0.00, truly law -15.00, good -9.00\n"
            ),
            "{map}"
        );
    }

    #[test]
    fn map_needs_a_faction_that_exists() {
        let mut session = session("content/sample");
        assert_eq!(
            run(&mut session, "map"),
            "error: map needs the form: map <faction>"
        );
        assert_eq!(
            run(&mut session, "map city_wach"),
            "error: unknown faction 'city_wach' (did you mean 'city_watch'?)"
        );
        assert_eq!(
            run(&mut Session::default(), "map city_watch"),
            "error: no world is loaded yet: use load <dir> first"
        );
    }

    #[test]
    fn matrix_gives_every_observers_disposition_toward_each_subject() {
        let mut session = session("content/sample");
        let hale = run(&mut session, "disposition captain_hale player");
        let hale = hale.split(' ').next().expect("a score").to_owned();
        let matrix = run(&mut session, "matrix player");
        let lines: Vec<&str> = matrix.lines().collect();
        let observers: Vec<&str> = lines[1..]
            .iter()
            .map(|line| line.split_whitespace().next().expect("an observer"))
            .collect();
        assert_eq!(
            observers,
            [
                "ashen_circle",
                "city_watch",
                "free_company",
                "lantern_guild",
                "temple",
                "brother_ash",
                "captain_hale",
                "merchant_ava",
                "player",
                "sister_mira",
                "vex",
            ],
            "factions first, each in id order"
        );
        assert_eq!(
            lines[0].split_whitespace().collect::<Vec<_>>(),
            ["observer", "player"]
        );
        assert_eq!(
            lines[7].split_whitespace().collect::<Vec<_>>(),
            ["captain_hale", hale.as_str()]
        );
        assert_eq!(
            lines[9].split_whitespace().collect::<Vec<_>>(),
            ["player", "—"],
            "not themselves"
        );
        let csv = run(&mut session, "matrix player --csv");
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(rows[0], "observer,player");
        assert_eq!(rows[7], format!("captain_hale,{hale}"));
        assert_eq!(rows[9], "player,");
    }

    #[test]
    fn matrix_without_subjects_has_every_character() {
        let mut session = session("content/sample");
        let csv = run(&mut session, "matrix --csv");
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(
            rows[0],
            "observer,brother_ash,captain_hale,merchant_ava,player,sister_mira,vex"
        );
        assert_eq!(rows.len(), 12);
        let hale: Vec<&str> = rows[7].split(',').collect();
        assert_eq!(hale[0], "captain_hale");
        assert_eq!(hale[2], "", "not himself");
        let to_vex = run(&mut session, "disposition captain_hale vex");
        assert_eq!(hale[6], to_vex.split(' ').next().expect("a score"));
    }

    #[test]
    fn matrix_needs_subjects_that_are_characters() {
        let mut session = session("content/sample");
        assert_eq!(
            run(&mut session, "matrix playr"),
            "error: unknown subject 'playr' (did you mean 'player'?)"
        );
        assert_eq!(
            run(&mut session, "matrix city_watch"),
            "error: a matrix's subjects must be characters"
        );
        assert_eq!(
            run(&mut Session::default(), "matrix"),
            "error: no world is loaded yet: use load <dir> first"
        );
    }

    #[test]
    fn curve_lists_a_curve_as_a_table() {
        assert_eq!(
            run(&mut Session::default(), "curve [[0, 1.0], [20, 0.5]]"),
            "       x        y\n    0.00     1.00  point\n   10.00     0.75\n   20.00     0.50  point"
        );
        assert_eq!(run(&mut Session::default(), "curve 0.5"), "0.50 everywhere");
        // Ends that aren't multiples of 10: every multiple of 10 between them, and no more.
        assert_eq!(
            run(&mut Session::default(), "curve [[-15, 0], [15, 1.5]]"),
            "       x        y\n  -15.00     0.00  point\n  -10.00     0.25\n    0.00     0.75\n   10.00     1.25\n   15.00     1.50  point"
        );
        assert_eq!(
            run(
                &mut Session::default(),
                "curve [[0, 1.0], [100, 0.5]] at 25"
            ),
            "0.88"
        );
    }

    #[test]
    fn curve_shows_a_named_knob() {
        let mut session = session("content/sample");
        let table = run(&mut session, "curve disposition.affinity");
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines[0], "disposition.affinity:");
        assert_eq!(
            lines.len(),
            2 + 21,
            "a title, a header, and 0 to 200 in tens"
        );
        // 50 at 0, falling to 0 at 60, then to -50 at 200 (DESIGN.md §8.1).
        for line in [
            "    0.00    50.00  point",
            "   10.00    41.67",
            "   60.00     0.00  point",
            "  130.00   -25.00",
            "  200.00   -50.00  point",
        ] {
            assert!(lines.contains(&line), "{line}\n{table}");
        }
        assert_eq!(
            run(&mut session, "curve disposition.affinity at 130"),
            "-25.00"
        );
        assert_eq!(run(&mut session, "curve standing.spillover at 75"), "0.25");
        // Halfway between 1.0 at 0 and 0.3 at 100.
        assert_eq!(
            run(
                &mut session,
                "curve inertia.hardening.good.toward_good at 50"
            ),
            "0.65"
        );
        assert_eq!(
            run(&mut session, "curve murder.by_target.good at 100"),
            "1.50"
        );
        assert_eq!(
            run(&mut session, "curve inertia.steady.law.toward_lawful"),
            "inertia.steady.law.toward_lawful: left out, so 1.00 everywhere"
        );
        assert_eq!(
            run(&mut session, "curve steal.by_target.relation at 10"),
            "1.00"
        );
    }

    #[test]
    fn curve_suggests_a_knob_for_a_misspelt_name() {
        let mut session = session("content/sample");
        assert_eq!(
            run(&mut session, "curve disposition.affinty"),
            "error: unknown curve 'disposition.affinty' (did you mean 'disposition.affinity'?)"
        );
        assert_eq!(
            run(&mut Session::default(), "curve disposition.affinity"),
            "error: no world is loaded yet: use load <dir> first"
        );
    }
}

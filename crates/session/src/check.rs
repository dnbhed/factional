//! Reading and checking a content directory, as `load` and `factional validate` both do, so
//! the two always report the same problems and warnings.

use std::path::Path;

use factional_content::Fingerprint;
use factional_quests::Quests;
use factional_reputation::World;

/// A content directory that would load: its world and quests, how much is in them, and their
/// warnings.
pub(crate) struct Checked {
    pub(crate) world: World,
    pub(crate) quests: Quests,
    /// Characters, factions, actions, relations and outcomes.
    pub(crate) counts: [usize; 5],
    pub(crate) warnings: Vec<String>,
    /// Exactly what was read, for saves.
    pub(crate) fingerprint: Fingerprint,
}

/// Reads and checks the content in `path`; every problem, one per line, if it wouldn't load.
pub(crate) fn check(path: &Path) -> Result<Checked, Vec<String>> {
    let (content, quests, fingerprint) =
        factional_content::load_dir_fingerprinted(path).map_err(|error| {
            error
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })?;
    let counts = [
        content.characters.len(),
        content.factions.len(),
        content.actions.len(),
        content.relations.len(),
        content.outcomes.len(),
    ];
    let warnings = factional_content::warnings(&content)
        .iter()
        .chain(&factional_content::quest_warnings(&content, &quests))
        .map(ToString::to_string)
        .collect();
    // The loader reports every problem a world would refuse, so an error here means the two
    // disagree: show it rather than hide it.
    let world = World::new(content)
        .map_err(|problems| problems.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    Ok(Checked {
        world,
        quests,
        counts,
        warnings,
        fingerprint,
    })
}

/// What `factional validate <dir>` prints: every problem or warning, as `load` gives them,
/// then a one-line summary; and whether the world would load. `dir` is read from `base`.
pub fn validate(base: &Path, dir: &str) -> (String, bool) {
    match check(&base.join(dir)) {
        Ok(checked) => {
            let mut report: String = checked
                .warnings
                .iter()
                .map(|warning| format!("warning: {warning}\n"))
                .collect();
            let with = match checked.warnings.len() {
                0 => String::new(),
                warnings => format!(", with {}", count(warnings, "warning")),
            };
            let [characters, factions, actions, relations, outcomes] = checked.counts;
            let mut counted = vec![
                count(characters, "character"),
                count(factions, "faction"),
                count(actions, "action"),
                count(relations, "relation"),
                count(outcomes, "outcome"),
            ];
            // Quests are counted only where there are any, as most worlds have none.
            if !checked.quests.is_empty() {
                counted.push(count(checked.quests.quests.len(), "quest"));
                counted.push(count(checked.quests.questlines.len(), "questline"));
            }
            let last = counted.pop().expect("there are counts");
            report.push_str(&format!(
                "{dir} loads{with}: {} and {last}\n",
                counted.join(", ")
            ));
            (report, true)
        }
        Err(problems) => {
            let mut report: String = problems
                .iter()
                .map(|problem| format!("error: {problem}\n"))
                .collect();
            report.push_str(&format!(
                "{dir} doesn't load: {}\n",
                count(problems.len(), "problem")
            ));
            (report, false)
        }
    }
}

/// `n` of `noun`, such as `1 character` or `6 characters`.
pub(crate) fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

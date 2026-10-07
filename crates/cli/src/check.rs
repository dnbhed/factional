//! Reading and checking a content directory, as `load` and `factional validate` both do, so
//! the two always report the same problems and warnings.

use std::path::Path;

use factional_content::Fingerprint;
use factional_reputation::World;

/// A content directory that would load: its world, how much is in it, and its warnings.
pub(crate) struct Checked {
    pub(crate) world: World,
    /// Characters, factions, actions, relations and outcomes.
    pub(crate) counts: [usize; 5],
    pub(crate) warnings: Vec<String>,
    /// Exactly what was read, for saves.
    pub(crate) fingerprint: Fingerprint,
}

/// Reads and checks the content in `path`; every problem, one per line, if it wouldn't load.
pub(crate) fn check(path: &Path) -> Result<Checked, Vec<String>> {
    let (content, fingerprint) =
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
        .map(ToString::to_string)
        .collect();
    // The loader reports every problem a world would refuse, so an error here means the two
    // disagree: show it rather than hide it.
    let world = World::new(content)
        .map_err(|problems| problems.iter().map(ToString::to_string).collect::<Vec<_>>())?;
    Ok(Checked {
        world,
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
            report.push_str(&format!(
                "{dir} loads{with}: {}, {}, {}, {} and {}\n",
                count(characters, "character"),
                count(factions, "faction"),
                count(actions, "action"),
                count(relations, "relation"),
                count(outcomes, "outcome"),
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

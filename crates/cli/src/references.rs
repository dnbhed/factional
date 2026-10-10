//! `references <dir> <id>` (U6a): every place in a directory's content that names an id,
//! found from the schema as the editor finds them, read without loading.

use std::path::Path;

use factional_content::{ContentTexts, defined_in, outline_texts, read_texts, referenced_by};

/// The files whose entries can be named, for "did you mean".
const DEFINING: [&str; 4] = [
    "factions.toml",
    "characters.toml",
    "quests.toml",
    "outcomes.toml",
];

use crate::session::{Outcome, hint};

/// `references <dir> <id>`, with `dir` read from `base`.
pub(crate) fn references(base: &Path, args: &str) -> Outcome {
    let [dir, id] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("references needs the form: references <dir> <id>".to_owned());
    };
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    let Some(file) = defined_in(&texts, id) else {
        let outline = outline_texts(&texts);
        let ids: Vec<&str> = outline
            .files
            .iter()
            .filter(|file| DEFINING.contains(&file.name))
            .flat_map(|file| file.entries.iter().map(|entry| entry.key.as_str()))
            .collect();
        return Outcome::Error(format!("unknown id '{id}'{}", hint(id, ids)));
    };
    let what = if file == "balance.toml" {
        "an inertia profile in balance.toml".to_owned()
    } else {
        format!("in {file}")
    };
    let found = referenced_by(&texts, file, id);
    let mut shown = vec![match found.len() {
        0 => format!("{id}, {what}, is named nowhere"),
        1 => format!("{id}, {what}, is named in 1 place:"),
        count => format!("{id}, {what}, is named in {count} places:"),
    }];
    shown.extend(
        found
            .iter()
            .map(|reference| format!("  {}: {}", reference.file, reference.key)),
    );
    Outcome::Output(shown.join("\n"))
}

#[cfg(test)]
mod tests {
    use crate::session::{Outcome, ScriptError, Session};

    fn run(line: &str) -> Result<Outcome, ScriptError> {
        Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).execute(line)
    }

    fn output(text: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(text.into()))
    }

    fn command_error(message: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Error(message.into()))
    }

    #[test]
    fn references_lists_every_place_that_names_a_character() {
        assert_eq!(
            run("references content/sample captain_hale"),
            output(
                "captain_hale, in characters.toml, is named in 4 places:\n\
                 \x20 characters.toml: merchant_ava.contacts[0]\n\
                 \x20 outcomes.toml: fined_by_watch.standing.characters.captain_hale\n\
                 \x20 quests.toml: lost_dog.giver\n\
                 \x20 quests.toml: lost_dog.stages[0].choices[0].effects.standing.characters.captain_hale"
            )
        );
    }

    #[test]
    fn references_finds_a_profile_and_says_when_nothing_names_an_id() {
        assert_eq!(
            run("references content/sample hardening"),
            output(
                "hardening, an inertia profile in balance.toml, is named in 2 places:\n\
                 \x20 characters.toml: captain_hale.inertia\n\
                 \x20 characters.toml: sister_mira.inertia"
            )
        );
        assert_eq!(
            run("references content/sample player"),
            output("player, in characters.toml, is named nowhere")
        );
        assert_eq!(
            run("references content/sample turned_in_vex"),
            output(
                "turned_in_vex, in outcomes.toml, is named in 1 place:\n\
                 \x20 quests.toml: watch_oath.stages[0].choices[0].outcome"
            )
        );
    }

    #[test]
    fn references_names_an_unknown_id_with_a_suggestion() {
        assert_eq!(
            run("references content/sample captain_hail"),
            command_error("unknown id 'captain_hail' (did you mean 'captain_hale'?)")
        );
        assert_eq!(
            run("references content/sample"),
            command_error("references needs the form: references <dir> <id>")
        );
    }
}

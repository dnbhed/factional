//! `words <dir> <file> <key>` (U10a): where a place in a content file is, in words, as the
//! editor's form and problems say it, read without loading.

use std::path::Path;

use factional_content::{CONTENT_FILES, ContentTexts, read_texts, words_at};

use crate::session::{Outcome, hint};

/// `words <dir> <file> <key>`, with `dir` read from `base`.
pub(crate) fn words(base: &Path, args: &str) -> Outcome {
    let [dir, file, key] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("words needs: words <dir> <file> <key>".to_owned());
    };
    if !CONTENT_FILES.contains(&file) {
        let hint = hint(file, CONTENT_FILES.to_vec());
        return Outcome::Error(format!("unknown file '{file}'{hint}"));
    }
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    Outcome::Output(words_at(&texts, file, key))
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

    #[test]
    fn words_says_a_place_as_the_editor_does() {
        assert_eq!(
            run("words content/sample quests.toml watch_oath.stages[0].choices[1].next"),
            output("The Watch's Oath › Stage \"patrol\" › Choice \"look_away\" › Next")
        );
        assert_eq!(
            run("words content/sample characters.toml vex.memberships[0].faction"),
            output("Vex › Membership \"The Lantern Guild\" › Faction")
        );
    }

    #[test]
    fn words_names_an_unknown_file_and_says_what_it_needs() {
        assert_eq!(
            run("words content/sample faction.toml city_watch.name"),
            Ok(Outcome::Error(
                "unknown file 'faction.toml' (did you mean 'factions.toml'?)".into()
            ))
        );
        assert_eq!(
            run("words content/sample factions.toml"),
            Ok(Outcome::Error(
                "words needs: words <dir> <file> <key>".into()
            ))
        );
    }
}

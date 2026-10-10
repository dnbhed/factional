//! `fixes <dir>` (U6c): each problem in a directory's content that the loader's "did you
//! mean" fixes, and the change that fixes it, as the editor's problems panel offers it.

use std::path::Path;

use factional_content::{ContentTexts, fix_for, load_texts, read_texts};

use crate::session::Outcome;

/// `fixes <dir>`, with `dir` read from `base`.
pub(crate) fn fixes(base: &Path, args: &str) -> Outcome {
    let [dir] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("fixes needs: fixes <dir>".to_owned());
    };
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    let Err(error) = load_texts(&texts) else {
        return Outcome::Output(format!("{dir} loads: nothing to fix"));
    };
    let problems = match error.diagnostics.len() {
        1 => "1 problem".to_owned(),
        count => format!("{count} problems"),
    };
    let found: Vec<String> = error
        .diagnostics
        .iter()
        .filter_map(|diagnostic| fix_for(&texts, diagnostic))
        .map(|fix| fix.to_string())
        .collect();
    let with = match found.len() {
        0 => "none".to_owned(),
        count => count.to_string(),
    };
    let mut shown = vec![format!("{dir} doesn't load: {problems}, {with} with a fix")];
    shown.extend(found);
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

    const WORLDS: &str = "crates/cli/tests/fixtures/worlds";

    #[test]
    fn fixes_lists_the_change_each_suggestion_asks_for() {
        assert_eq!(
            run(&format!("fixes {WORLDS}/typos")),
            output(&format!(
                "{WORLDS}/typos doesn't load: 3 problems, 3 with a fix\n\
                 factions.toml: temple.drift.policy: set it to 'demote'\n\
                 characters.toml: captain_hale.weigths: rename it 'weights'\n\
                 characters.toml: vex.memberships[0].faction: set it to 'lantern_guild'"
            ))
        );
    }

    #[test]
    fn fixes_counts_what_it_can_fix_and_says_when_there_is_nothing() {
        assert_eq!(
            run(&format!("fixes {WORLDS}/broken")),
            output(&format!(
                "{WORLDS}/broken doesn't load: 3 problems, 1 with a fix\n\
                 characters.toml: hale.alignmnet: rename it 'alignment'"
            ))
        );
        assert_eq!(
            run(&format!("fixes {WORLDS}/unparsed_balance")),
            output(&format!(
                "{WORLDS}/unparsed_balance doesn't load: 1 problem, none with a fix"
            ))
        );
        assert_eq!(
            run("fixes content/sample"),
            output("content/sample loads: nothing to fix")
        );
        assert_eq!(
            run("fixes"),
            Ok(Outcome::Error("fixes needs: fixes <dir>".into()))
        );
    }
}

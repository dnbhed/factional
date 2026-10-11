//! `form <dir> <file> <entry>` (U6b): an entry's values as the editor's Content tab shows
//! them, grouped by what they mean, with their comments, defaults and choices, read without
//! loading.

use std::path::Path;

use factional_content::{
    CONTENT_FILES, ContentTexts, FormRow, entry_form, outline_texts, read_texts,
};

use crate::session::{Outcome, hint};

/// `form <dir> <file> <entry>`, with `dir` read from `base`.
pub(crate) fn form(base: &Path, args: &str) -> Outcome {
    let [dir, file, key] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("form needs: form <dir> <file> <entry>".to_owned());
    };
    if !CONTENT_FILES.contains(&file) {
        let hint = hint(file, CONTENT_FILES.to_vec());
        return Outcome::Error(format!("unknown file '{file}'{hint}"));
    }
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    let outline = outline_texts(&texts);
    let entries = outline
        .files
        .iter()
        .find(|outlined| outlined.name == file)
        .map(|outlined| outlined.entries.as_slice())
        .unwrap_or_default();
    let Some(entry) = entries.iter().find(|entry| entry.key == key) else {
        let keys = entries.iter().map(|entry| entry.key.as_str()).collect();
        return Outcome::Error(format!("no entry '{key}' in {file}{}", hint(key, keys)));
    };
    let mut shown = vec![match &entry.name {
        Some(name) => format!("{file} › {key}: {name}"),
        None => format!("{file} › {key}"),
    }];
    for group in entry_form(&texts, file, key) {
        shown.extend(group.name);
        for row in &group.rows {
            shown.extend(row.comment.iter().map(|line| format!("  # {line}")));
            shown.push(format!("  {}", describe(row)));
        }
    }
    Outcome::Output(shown.join("\n"))
}

/// `Drift › Then · drift.then = expel (demote or expel)`, `Secret members · secret_members:
/// not set, false (false or true)` or `Standing · standing: empty`: its words, then its key
/// and value.
fn describe(row: &FormRow) -> String {
    let words: Vec<&str> = row.words.iter().map(|word| word.text.as_str()).collect();
    let value = match (&row.written, &row.default) {
        (Some(field), _) => format!("{} = {}", row.key, field.value),
        (None, Some(default)) => format!("{}: not set, {default}", row.key),
        (None, None) => format!("{}: empty", row.key),
    };
    let value = format!("{} · {value}", words.join(" › "));
    match either(&row.choices) {
        Some(choices) => format!("{value} ({choices})"),
        None => value,
    }
}

/// `a`, `a or b`, or `a, b or c`; `None` for nothing to choose from.
fn either(choices: &[String]) -> Option<String> {
    let (last, rest) = choices.split_last()?;
    Some(match rest {
        [] => last.clone(),
        rest => format!("{} or {last}", rest.join(", ")),
    })
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
    fn form_groups_a_factions_values_with_their_comments_defaults_and_choices() {
        assert_eq!(
            run("form content/sample factions.toml city_watch"),
            output(
                "factions.toml › city_watch: The City Watch\n\
                 Identity\n\
                 \x20 Name · name = The City Watch\n\
                 Where it stands\n\
                 \x20 Alignment › Law · alignment.law = 70.0\n\
                 \x20 Alignment › Good · alignment.good = 20.0\n\
                 \x20 # Cares about order far more than kindness.\n\
                 \x20 Weights › Law · weights.law = 1.0\n\
                 \x20 Weights › Good · weights.good = 0.25\n\
                 Membership\n\
                 \x20 Tolerance · tolerance = 40.0\n\
                 \x20 Member tolerance · member_tolerance = 50.0\n\
                 \x20 Standing change on expulsion · expel_standing_change: not set, -20.00\n\
                 \x20 Standing change on leaving · leave_standing_change: not set, 0.00\n\
                 \x20 Secret members · secret_members: not set, false (false or true)\n\
                 Drift\n\
                 \x20 # The Watch gives a straying officer a hundred ticks to mend their ways.\n\
                 \x20 Drift › Policy · drift.policy = probation (ignore, flag, demote, expel or probation)\n\
                 \x20 Drift › Grace (ticks) · drift.grace_ticks = 100\n\
                 \x20 Drift › Then · drift.then = expel (demote or expel)\n\
                 Ranks\n\
                 \x20 Rank \"recruit\" › Id · ranks[0].id = recruit\n\
                 \x20 Rank \"sergeant\" › Id · ranks[1].id = sergeant\n\
                 \x20 Rank \"sergeant\" › Requirements › Standing · ranks[1].requires.standing = 30.0\n\
                 \x20 Rank \"captain\" › Id · ranks[2].id = captain\n\
                 \x20 Rank \"captain\" › Requirements › Standing · ranks[2].requires.standing = 70.0\n\
                 \x20 # Captains are held to a stricter standard than the faction's member tolerance.\n\
                 \x20 Rank \"captain\" › Tolerance · ranks[2].tolerance = 25.0"
            )
        );
    }

    #[test]
    fn form_shows_an_ungrouped_entry_without_headings() {
        assert_eq!(
            run("form content/sample balance.toml knowledge"),
            output(
                "balance.toml › knowledge\n\
                 \x20 # Who learns of an act: \"omniscient\" (everyone, fully and at once, whoever saw it),\n\
                 \x20 # \"witnessed\" (only the witnesses, the parties the act names, and the factions of those who\n\
                 \x20 # learned), or \"ripple\" (those, then onward through contacts and factions). A party that\n\
                 \x20 # hasn't learned of an act keeps its standing (DESIGN.md §10).\n\
                 \x20 Model · model = ripple (omniscient, witnessed or ripple)\n\
                 \x20 # How strongly news arrives at each hop beyond those who saw it; it goes no further than\n\
                 \x20 # the last. Standing changes scale with it.\n\
                 \x20 Ripple › Strength › 1 · ripple.strength[0] = 0.5\n\
                 \x20 Ripple › Strength › 2 · ripple.strength[1] = 0.25\n\
                 \x20 Ripple › Strength › 3 · ripple.strength[2] = 0.1\n\
                 \x20 # How many ticks each hop takes.\n\
                 \x20 Ripple › Ticks a hop · ripple.hop_ticks = 10"
            )
        );
    }

    #[test]
    fn an_empty_table_or_list_is_said_to_be_empty() {
        use factional_content::{FormRow, ValuePath, Word};
        let row = FormRow {
            path: ValuePath::parse("ava.standing").expect("a path"),
            key: "standing".to_owned(),
            words: vec![Word {
                text: "Standing".to_owned(),
                through: 1,
            }],
            written: None,
            default: None,
            description: None,
            comment: Vec::new(),
            choices: Vec::new(),
        };
        assert_eq!(super::describe(&row), "Standing · standing: empty");
    }

    #[test]
    fn choices_are_listed_in_words() {
        let words = |choices: &[&str]| {
            let choices: Vec<String> = choices.iter().map(|c| (*c).to_owned()).collect();
            super::either(&choices)
        };
        assert_eq!(words(&[]), None);
        assert_eq!(words(&["a"]).as_deref(), Some("a"));
        assert_eq!(words(&["a", "b"]).as_deref(), Some("a or b"));
        assert_eq!(words(&["a", "b", "c"]).as_deref(), Some("a, b or c"));
    }

    #[test]
    fn form_names_an_unknown_file_or_entry_with_a_suggestion() {
        assert_eq!(
            run("form content/sample factions.toml city_wach"),
            command_error("no entry 'city_wach' in factions.toml (did you mean 'city_watch'?)")
        );
        assert_eq!(
            run("form content/sample faction.toml city_watch"),
            command_error("unknown file 'faction.toml' (did you mean 'factions.toml'?)")
        );
        assert_eq!(
            run("form content/sample factions.toml"),
            command_error("form needs: form <dir> <file> <entry>")
        );
    }
}

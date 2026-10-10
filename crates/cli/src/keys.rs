//! `keys <dir> <file> [<path>]` (U6d): what can be added at a place in a content file, each
//! key with what it is, its default and its description, as the editor's "Add to" menu
//! offers it.

use std::path::Path;

use factional_content::{
    Addition, CONTENT_FILES, ContentTexts, Step, ValuePath, additions, entry_places, key_info,
    outline_texts, read_texts,
};

use crate::session::{Outcome, hint};

/// `keys <dir> <file> [<path>]`, with `dir` read from `base`.
pub(crate) fn keys(base: &Path, args: &str) -> Outcome {
    let (dir, file, at) = match args.split_whitespace().collect::<Vec<_>>()[..] {
        [dir, file] => (dir, file, None),
        [dir, file, at] => (dir, file, Some(at)),
        _ => return Outcome::Error("keys needs: keys <dir> <file> [<path>]".to_owned()),
    };
    let Some(place) = CONTENT_FILES.iter().position(|name| *name == file) else {
        let hint = hint(file, CONTENT_FILES.to_vec());
        return Outcome::Error(format!("unknown file '{file}'{hint}"));
    };
    let texts: ContentTexts = read_texts(&base.join(dir)).unwrap_or_default();
    let text = texts[place].as_deref().unwrap_or_default();
    let path = match at {
        None => ValuePath::ROOT,
        Some(at) => match ValuePath::parse(at).filter(|path| is_place(&texts, place, path)) {
            Some(path) => path,
            None => return Outcome::Error(format!("nothing at {at} in {file}")),
        },
    };
    let heading = match at {
        Some(at) => format!("{file} › {at}"),
        None => file.to_owned(),
    };
    let offered = additions(file, text, &path);
    if offered.is_empty() {
        return Outcome::Output(format!("{heading} has every key it can"));
    }
    let mut shown = vec![format!("{heading} can have:")];
    for addition in offered {
        match addition {
            Addition::Key(key) => {
                let Some(info) = key_info(file, &path.then(Step::Key(key.clone()))) else {
                    continue;
                };
                let default = info
                    .default
                    .map(|default| format!(", {default} by default"))
                    .unwrap_or_default();
                shown.push(format!("  {key}: {}{default}", info.kind));
                shown.extend(info.description.map(|said| format!("    {said}")));
            }
            Addition::Item => shown.push("  an item at the end".to_owned()),
            Addition::Id => shown.push("  an entry by a new id".to_owned()),
        }
    }
    Outcome::Output(shown.join("\n"))
}

/// Whether `path` is a table or list in an entry of the `place`th file.
fn is_place(texts: &ContentTexts, place: usize, path: &ValuePath) -> bool {
    let outline = outline_texts(texts);
    let text = texts[place].as_deref().unwrap_or_default();
    outline.files[place]
        .entries
        .iter()
        .any(|entry| entry_places(text, &entry.key).contains(path))
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
    fn keys_says_what_each_key_that_can_be_added_is() {
        assert_eq!(
            run("keys content/sample factions.toml city_watch"),
            output(
                "factions.toml › city_watch can have:\n\
                 \x20 defectors: a table\n\
                 \x20   Does the faction joined take a member of its enemies? The last rule must have no conditions.\n\
                 \x20 deserters: a table\n\
                 \x20   Does each enemy faction someone is in let them go? The last rule must have no conditions.\n\
                 \x20 expel_standing_change: a number from -100.00 to 100.00, -20.00 by default\n\
                 \x20   The change in standing with the faction when it expels someone.\n\
                 \x20 exposed: a table\n\
                 \x20   On learning a member is secretly in a faction it's in conflict with: keep, demote or expel them? \"Current\" is the faction that found out, \"target\" the secret one. The last rule must have no conditions.\n\
                 \x20 leave_standing_change: a number from -100.00 to 100.00, 0.00 by default\n\
                 \x20   The change in standing with the faction when someone leaves of their own accord.\n\
                 \x20 secret_members: true or false, false by default\n\
                 \x20   Whether characters may belong to it secretly, unknown to anyone outside it. Needs knowledge.model witnessed or ripple (DESIGN.md §10.4)."
            )
        );
    }

    #[test]
    fn keys_offers_items_and_ids_where_they_fit() {
        assert_eq!(
            run("keys content/sample factions.toml city_watch.ranks"),
            output("factions.toml › city_watch.ranks can have:\n  an item at the end")
        );
        assert_eq!(
            run("keys content/sample characters.toml"),
            output("characters.toml can have:\n  an entry by a new id")
        );
        assert_eq!(
            run("keys content/sample factions.toml city_watch.alignment"),
            output("factions.toml › city_watch.alignment has every key it can")
        );
    }

    #[test]
    fn keys_names_what_it_cant_find() {
        assert_eq!(
            run("keys content/sample faction.toml"),
            command_error("unknown file 'faction.toml' (did you mean 'factions.toml'?)")
        );
        assert_eq!(
            run("keys content/sample factions.toml nobody"),
            command_error("nothing at nobody in factions.toml")
        );
        assert_eq!(
            run("keys content/sample"),
            command_error("keys needs: keys <dir> <file> [<path>]")
        );
    }
}

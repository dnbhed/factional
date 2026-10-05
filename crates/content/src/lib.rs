//! Loads designer content (TOML) from disk, validates it, and turns mistakes into diagnostics
//! that name the file and the key path (DESIGN.md §12.1).

mod reader;

use std::collections::BTreeMap;
use std::path::Path;
use std::{fmt, fs, io};

use factional_core::{Curve, Fixed};
use factional_reputation::{
    Action, ActionId, Alignment, AlignmentDelta, Balance, Character, CharacterId, Content,
};
use reader::{Report, Section};
use serde::Deserialize;
use toml::Value;

/// One problem in the content, such as
/// `characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    /// Where in the file, like `vex.alignment.law`; `None` for the file as a whole.
    pub key: Option<String>,
    pub message: String,
}

/// Every problem found, so a designer can fix them all in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError {
    pub diagnostics: Vec<Diagnostic>,
}

/// The text of each content file; `None` for a file that isn't there. A missing file means
/// the defaults (for `balance.toml`) or nothing of that kind (for the others).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    pub balance: Option<&'a str>,
    pub characters: Option<&'a str>,
    pub actions: Option<&'a str>,
}

const BALANCE_FILE: &str = "balance.toml";
const CHARACTERS_FILE: &str = "characters.toml";
const ACTIONS_FILE: &str = "actions.toml";

/// Reads and validates the content files in `dir`.
pub fn load_dir(dir: &Path) -> Result<Content, ContentError> {
    let unreadable = |error: io::Error| ContentError {
        diagnostics: vec![Diagnostic {
            file: dir.display().to_string(),
            key: None,
            message: format!("cannot read the directory: {error}"),
        }],
    };
    if let Err(error) = fs::read_dir(dir) {
        return Err(unreadable(error));
    }
    let read = |file: &str| match fs::read_to_string(dir.join(file)) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ContentError {
            diagnostics: vec![Diagnostic {
                file: file.to_owned(),
                key: None,
                message: format!("cannot read the file: {error}"),
            }],
        }),
    };
    let balance = read(BALANCE_FILE)?;
    let characters = read(CHARACTERS_FILE)?;
    let actions = read(ACTIONS_FILE)?;
    parse_content(Sources {
        balance: balance.as_deref(),
        characters: characters.as_deref(),
        actions: actions.as_deref(),
    })
}

/// Validates content from the text of its files, reporting every problem at once.
pub fn parse_content(sources: Sources<'_>) -> Result<Content, ContentError> {
    let mut balance_report = Report::new(BALANCE_FILE);
    let balance = sources
        .balance
        .map(|text| read_balance(text, &mut balance_report))
        .unwrap_or_default();
    let mut characters_report = Report::new(CHARACTERS_FILE);
    let characters = sources
        .characters
        .map(|text| read_characters(text, &mut characters_report))
        .unwrap_or_default();

    let mut actions_report = Report::new(ACTIONS_FILE);
    let actions = sources
        .actions
        .map(|text| read_actions(text, &mut actions_report))
        .unwrap_or_default();

    let diagnostics: Vec<Diagnostic> = [balance_report, characters_report, actions_report]
        .into_iter()
        .flat_map(|report| report.diagnostics)
        .collect();
    if diagnostics.is_empty() {
        Ok(Content {
            balance,
            characters,
            actions,
        })
    } else {
        Err(ContentError { diagnostics })
    }
}

/// `balance.toml`: world-wide rules and defaults; anything left out keeps its default.
fn read_balance(text: &str, report: &mut Report) -> Balance {
    let mut balance = Balance::default();
    let Some(table) = report.parse(text) else {
        return balance;
    };
    let mut file = Section::new(&table, String::new());
    if let Some(mut alignment) = file.optional_table("alignment", "[alignment]", report) {
        if let Some(threshold) = alignment.optional_fixed("label_threshold", report) {
            let (low, high) = (Fixed::from_hundredths(1), factional_reputation::AXIS_LIMIT);
            if (low..=high).contains(&threshold) {
                balance.label_threshold = threshold;
            } else {
                report.error(
                    &alignment.path_to("label_threshold"),
                    format!("{threshold} must be between {low} and {high}"),
                );
            }
        }
        alignment.finish(report);
    }
    file.finish(report);
    balance
}

/// `characters.toml`: one table per character, keyed by id.
fn read_characters(text: &str, report: &mut Report) -> BTreeMap<CharacterId, Character> {
    let mut characters = BTreeMap::new();
    let Some(table) = report.parse(text) else {
        return characters;
    };
    for (key, value) in &table {
        let id = match CharacterId::new(key) {
            Ok(id) => id,
            Err(invalid) => {
                report.error(key, invalid.to_string());
                continue;
            }
        };
        let Value::Table(fields) = value else {
            report.error(
                key,
                format!("expected a table of character fields, like [{key}]"),
            );
            continue;
        };
        if let Some(character) = read_character(id, fields, report) {
            characters.insert(character.id.clone(), character);
        }
    }
    characters
}

fn read_character(id: CharacterId, fields: &toml::Table, report: &mut Report) -> Option<Character> {
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let alignment = section
        .table("alignment", "{ law = 0.0, good = 0.0 }", report)
        .and_then(|mut axes| {
            let law = axes.fixed("law", report);
            let good = axes.fixed("good", report);
            let alignment = match (law, good) {
                (Some(law), Some(good)) => match Alignment::new(law, good) {
                    Ok(alignment) => Some(alignment),
                    Err(out_of_range) => {
                        for problem in out_of_range {
                            report.error(&axes.path_to(problem.axis.key()), problem.to_string());
                        }
                        None
                    }
                },
                _ => None,
            };
            axes.finish(report);
            alignment
        });
    section.finish(report);
    Some(Character {
        id,
        name: name?,
        alignment: alignment?,
    })
}

/// `actions.toml`: the action catalogue, one table per action, keyed by id.
fn read_actions(text: &str, report: &mut Report) -> BTreeMap<ActionId, Action> {
    let mut actions = BTreeMap::new();
    let Some(table) = report.parse(text) else {
        return actions;
    };
    for (key, value) in &table {
        let id = match ActionId::new(key) {
            Ok(id) => id,
            Err(invalid) => {
                report.error(key, invalid.to_string());
                continue;
            }
        };
        let Value::Table(fields) = value else {
            report.error(
                key,
                format!("expected a table of action fields, like [{key}]"),
            );
            continue;
        };
        actions.insert(id.clone(), read_action(id, fields, report));
    }
    actions
}

/// One action. Its `alignment` and each axis in it may be left out: an act needn't touch
/// both axes, or alignment at all. Any problem is reported, and fails the whole load.
fn read_action(id: ActionId, fields: &toml::Table, report: &mut Report) -> Action {
    let mut section = Section::new(fields, id.to_string());
    let mut alignment = AlignmentDelta::default();
    if let Some(mut axes) = section.optional_table("alignment", "{ law = 0.0, good = 0.0 }", report)
    {
        alignment.law = axes.optional_fixed("law", report).unwrap_or_default();
        alignment.good = axes.optional_fixed("good", report).unwrap_or_default();
        axes.finish(report);
    }
    section.finish(report);
    Action { id, alignment }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.key {
            Some(key) => write!(f, "{}: {key}: {}", self.file, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self.diagnostics.iter().map(Diagnostic::to_string).collect();
        f.write_str(&lines.join("\n"))
    }
}

impl std::error::Error for ContentError {}

/// Reads a curve written the way it would appear in a content file: a number, or a list of
/// `[x, y]` points such as `[[0, 50], [60, 0], [200, -50]]`.
pub fn parse_curve(text: &str) -> Result<Curve, String> {
    #[derive(Deserialize)]
    struct Wrapper {
        curve: Curve,
    }
    toml::from_str::<Wrapper>(&format!("curve = {text}"))
        .map(|wrapper| wrapper.curve)
        .map_err(|error| error.message().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use factional_core::Fixed;
    use factional_reputation::CharacterId;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn characters(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            characters: Some(text),
            ..Sources::default()
        })
    }

    fn actions(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            actions: Some(text),
            ..Sources::default()
        })
    }

    fn balance(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            balance: Some(text),
            ..Sources::default()
        })
    }

    /// The problems reported, one string per diagnostic.
    fn problems(result: Result<Content, ContentError>) -> Vec<String> {
        result
            .expect_err("the content has problems")
            .diagnostics
            .iter()
            .map(Diagnostic::to_string)
            .collect()
    }

    const VEX: &str = r#"
        [vex]
        name = "Vex"
        alignment = { law = -55.0, good = -20.0 }
    "#;

    // Reading

    #[test]
    fn reads_characters() {
        let content = characters(VEX).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(vex.name, "Vex");
        assert_eq!(
            (vex.alignment.law(), vex.alignment.good()),
            (h(-55_00), h(-20_00))
        );
        assert_eq!(content.characters.len(), 1);
    }

    #[test]
    fn missing_files_mean_defaults_and_no_characters_or_actions() {
        let content = parse_content(Sources::default()).expect("valid content");
        assert_eq!(content.balance.label_threshold, h(33_00));
        assert!(content.characters.is_empty());
        assert!(content.actions.is_empty());
    }

    #[test]
    fn reads_the_label_threshold() {
        let content = balance("[alignment]\nlabel_threshold = 40").expect("valid content");
        assert_eq!(content.balance.label_threshold, h(40_00));
    }

    #[test]
    fn an_empty_alignment_section_keeps_the_default_threshold() {
        let content = balance("[alignment]").expect("valid content");
        assert_eq!(content.balance.label_threshold, h(33_00));
    }

    // Mistakes in characters.toml

    #[test]
    fn reports_an_axis_out_of_range_at_its_key() {
        let text = VEX.replace("law = -55.0", "law = 120");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00"]
        );
    }

    #[test]
    fn reports_an_unknown_key_with_a_suggestion() {
        let text = VEX.replace("alignment =", "alignmnet =");
        assert_eq!(
            problems(characters(&text)),
            [
                "characters.toml: vex: missing 'alignment'",
                "characters.toml: vex: unknown key 'alignmnet' (did you mean 'alignment'?)",
            ]
        );
    }

    #[test]
    fn reports_an_unknown_key_without_a_suggestion_when_nothing_is_close() {
        let text = format!("{VEX}colour = \"red\"");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex: unknown key 'colour'"]
        );
    }

    #[test]
    fn reports_missing_and_mistyped_fields() {
        let text = r#"
            [ava]
            alignment = { law = 20.0, good = 10.0 }

            [hale]
            name = 7
            alignment = "lawful"

            [mira]
            name = "Mira"
            alignment = { law = 35.0 }
        "#;
        assert_eq!(
            problems(characters(text)),
            [
                "characters.toml: ava: missing 'name'",
                "characters.toml: hale.name: expected text in quotes",
                "characters.toml: hale.alignment: expected a table, like { law = 0.0, good = 0.0 }",
                "characters.toml: mira.alignment: missing 'good'",
            ]
        );
    }

    #[test]
    fn reports_bad_numbers_in_a_designers_words() {
        let text = VEX.replace("good = -20.0", "good = -20.125");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex.alignment.good: -20.125 has more than 2 decimal places"]
        );
    }

    #[test]
    fn reports_an_invalid_character_id() {
        let text = VEX.replace("[vex]", "[\"Vex Thief\"]");
        assert_eq!(
            problems(characters(&text)),
            [
                "characters.toml: Vex Thief: 'Vex Thief' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            ]
        );
    }

    #[test]
    fn reports_a_character_that_is_not_a_table() {
        assert_eq!(
            problems(characters("vex = 3")),
            ["characters.toml: vex: expected a table of character fields, like [vex]"]
        );
    }

    #[test]
    fn reports_a_toml_syntax_error_with_its_line() {
        let found = problems(characters("[vex]\nname = \"Vex\nalignment = 3"));
        assert_eq!(found.len(), 1);
        assert!(
            found[0].starts_with("characters.toml: line 2: "),
            "{found:?}"
        );
    }

    // actions.toml

    const STEAL: &str = r#"
        [steal]
        alignment = { law = -5.0, good = -3.0 }
    "#;

    fn delta(content: &Content, action: &str) -> AlignmentDelta {
        content.actions[&ActionId::new(action).expect("valid id")].alignment
    }

    #[test]
    fn reads_actions_and_their_alignment_effects() {
        let text = format!("{STEAL}\n[help_stranger]\nalignment = {{ good = 4.0 }}");
        let content = actions(&text).expect("valid content");
        assert_eq!(
            delta(&content, "steal"),
            AlignmentDelta {
                law: h(-5_00),
                good: h(-3_00)
            }
        );
        assert_eq!(
            delta(&content, "help_stranger"),
            AlignmentDelta {
                law: h(0),
                good: h(4_00)
            },
            "an axis left out isn't moved"
        );
        let ids: Vec<&str> = content.actions.keys().map(ActionId::as_str).collect();
        assert_eq!(ids, ["help_stranger", "steal"]);
    }

    #[test]
    fn an_action_without_an_alignment_effect_moves_nothing() {
        let content = actions("[wave]").expect("valid content");
        assert_eq!(delta(&content, "wave"), AlignmentDelta::default());
    }

    #[test]
    fn an_actions_alignment_names_only_law_and_good() {
        assert_eq!(
            problems(actions(&STEAL.replace("good =", "goood ="))),
            ["actions.toml: steal.alignment: unknown key 'goood' (did you mean 'good'?)"]
        );
        assert_eq!(
            problems(actions(&STEAL.replace("good =", "chaos ="))),
            ["actions.toml: steal.alignment: unknown key 'chaos'"]
        );
    }

    #[test]
    fn reports_mistakes_in_an_action() {
        let text = r#"
            bow = 3

            ["Steal"]
            alignment = { law = -5.0 }

            [extort]
            alignment = "evil"

            [murder]
            alignment = { law = -10.005 }
            standing = { target = -100.0 }
        "#;
        assert_eq!(
            problems(actions(text)),
            [
                "actions.toml: Steal: 'Steal' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "actions.toml: bow: expected a table of action fields, like [bow]",
                "actions.toml: extort.alignment: expected a table, like { law = 0.0, good = 0.0 }",
                "actions.toml: murder.alignment.law: -10.005 has more than 2 decimal places",
                "actions.toml: murder: unknown key 'standing'",
            ]
        );
    }

    // Mistakes in balance.toml

    #[test]
    fn reports_an_unknown_section_in_balance() {
        assert_eq!(
            problems(balance("[alignmnt]\nlabel_threshold = 40")),
            ["balance.toml: unknown key 'alignmnt' (did you mean 'alignment'?)"]
        );
    }

    #[test]
    fn reports_an_unknown_key_in_a_balance_section() {
        assert_eq!(
            problems(balance("[alignment]\nlabel_treshold = 40")),
            [
                "balance.toml: alignment: unknown key 'label_treshold' (did you mean 'label_threshold'?)"
            ]
        );
    }

    #[test]
    fn reports_a_label_threshold_out_of_range() {
        for value in ["0", "100.01", "-5"] {
            let shown: Fixed = value.parse().expect("a number");
            assert_eq!(
                problems(balance(&format!("[alignment]\nlabel_threshold = {value}"))),
                [format!(
                    "balance.toml: alignment.label_threshold: {shown} must be between 0.01 and 100.00"
                )]
            );
        }
        assert!(balance("[alignment]\nlabel_threshold = 0.01").is_ok());
        assert!(balance("[alignment]\nlabel_threshold = 100").is_ok());
    }

    #[test]
    fn reports_a_balance_section_that_is_not_a_table() {
        assert_eq!(
            problems(balance("alignment = 3")),
            ["balance.toml: alignment: expected a table, like [alignment]"]
        );
    }

    // All at once

    #[test]
    fn reports_every_problem_in_every_file_in_order() {
        let found = problems(parse_content(Sources {
            balance: Some("[alignment]\nlabel_threshold = 0"),
            characters: Some(
                "[zed]\nalignment = { law = 0.0, good = 0.0 }\n[abe]\nname = 1\nalignment = { law = 0.0, good = 0.0 }",
            ),
            actions: Some("[steal]\nalignment = { evil = 3.0 }"),
        }));
        assert_eq!(
            found,
            [
                "balance.toml: alignment.label_threshold: 0.00 must be between 0.01 and 100.00",
                "characters.toml: abe.name: expected text in quotes",
                "characters.toml: zed: missing 'name'",
                "actions.toml: steal.alignment: unknown key 'evil'",
            ]
        );
    }

    #[test]
    fn a_diagnostic_without_a_key_names_just_the_file() {
        let diagnostic = Diagnostic {
            file: "characters.toml".into(),
            key: None,
            message: "line 2: broken".into(),
        };
        assert_eq!(diagnostic.to_string(), "characters.toml: line 2: broken");
        let error = ContentError {
            diagnostics: vec![diagnostic.clone(), diagnostic],
        };
        assert_eq!(
            error.to_string(),
            "characters.toml: line 2: broken\ncharacters.toml: line 2: broken"
        );
    }

    // Curves

    #[test]
    fn parses_a_curve_list_of_points() {
        let curve = parse_curve("[[0, 1.0], [100, 0.5]]").expect("a valid curve");
        assert_eq!(curve.at(h(5000)), h(75));
    }

    #[test]
    fn parses_a_number_as_a_flat_curve() {
        let curve = parse_curve("1.5").expect("a valid curve");
        assert_eq!(curve.at(h(-99_900)), h(150));
    }

    #[test]
    fn reports_a_curves_own_validation_errors() {
        assert_eq!(
            parse_curve("[[0, 1.0]]"),
            Err("a curve is a single number or at least 2 points".into())
        );
    }

    #[test]
    fn reports_curve_text_that_is_not_toml() {
        let error = parse_curve("[[0, 1.0], [5").unwrap_err();
        assert!(!error.is_empty());
    }
}

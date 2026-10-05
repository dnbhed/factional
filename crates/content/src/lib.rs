//! Loads designer content (TOML) from disk, validates it, and turns mistakes into diagnostics
//! that name the file and the key path (DESIGN.md §12.1).

mod reader;

use std::collections::BTreeMap;
use std::path::Path;
use std::{fmt, fs, io};

use factional_core::{Curve, Fixed, suggest};
use factional_reputation::{
    Action, ActionId, Alignment, AlignmentDelta, Balance, Band, BandProblem, Bands, Character,
    CharacterId, Content, ContentProblem, ContentWarning, Faction, FactionId, InvalidId, Metric,
    ToleranceProblem, Tolerances, WeightProblem, Weights,
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
    pub factions: Option<&'a str>,
    pub characters: Option<&'a str>,
    pub actions: Option<&'a str>,
}

const BALANCE_FILE: &str = "balance.toml";
const FACTIONS_FILE: &str = "factions.toml";
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
    let factions = read(FACTIONS_FILE)?;
    let characters = read(CHARACTERS_FILE)?;
    let actions = read(ACTIONS_FILE)?;
    parse_content(Sources {
        balance: balance.as_deref(),
        factions: factions.as_deref(),
        characters: characters.as_deref(),
        actions: actions.as_deref(),
    })
}

/// Validates content from the text of its files, reporting every problem at once: each
/// file's own, in file order, then problems across files (P-32).
pub fn parse_content(sources: Sources<'_>) -> Result<Content, ContentError> {
    let mut reports = Vec::new();
    let balance = read_file(BALANCE_FILE, sources.balance, &mut reports, read_balance);
    let factions = read_file(
        FACTIONS_FILE,
        sources.factions,
        &mut reports,
        |text, report| read_tables(text, report, "faction", FactionId::new, read_faction),
    );
    let characters = read_file(
        CHARACTERS_FILE,
        sources.characters,
        &mut reports,
        |text, report| read_tables(text, report, "character", CharacterId::new, read_character),
    );
    let actions = read_file(
        ACTIONS_FILE,
        sources.actions,
        &mut reports,
        |text, report| {
            read_tables(
                text,
                report,
                "action",
                ActionId::new,
                |id, fields, report| Some(read_action(id, fields, report)),
            )
        },
    );
    let content = Content {
        balance: balance.unwrap_or_default(),
        factions: factions.unwrap_or_default(),
        characters: characters.unwrap_or_default(),
        actions: actions.unwrap_or_default(),
    };

    let mut diagnostics: Vec<Diagnostic> = reports
        .into_iter()
        .flat_map(|report| report.diagnostics)
        .collect();
    diagnostics.extend(content.problems().iter().map(|problem| {
        let (file, key) = match problem {
            ContentProblem::SharedId(id) => (FACTIONS_FILE, id.to_string()),
            ContentProblem::AffinityOutOfRange(_) => (BALANCE_FILE, "disposition.affinity".into()),
            ContentProblem::UnknownMembershipFaction {
                character, index, ..
            }
            | ContentProblem::DuplicateMembership {
                character, index, ..
            } => (
                CHARACTERS_FILE,
                format!("{character}.memberships[{index}].faction"),
            ),
        };
        Diagnostic {
            file: file.to_owned(),
            key: Some(key),
            message: problem.to_string(),
        }
    }));
    if diagnostics.is_empty() {
        Ok(content)
    } else {
        Err(ContentError { diagnostics })
    }
}

/// Content's warnings: things allowed but probably not meant, each with the file and key
/// it's about. Loading succeeds regardless; show them to the designer.
pub fn warnings(content: &Content) -> Vec<Diagnostic> {
    content
        .warnings()
        .iter()
        .map(|warning| match warning {
            ContentWarning::OutsideMemberTolerance {
                character, index, ..
            } => Diagnostic {
                file: CHARACTERS_FILE.to_owned(),
                key: Some(format!("{character}.memberships[{index}]")),
                message: warning.to_string(),
            },
        })
        .collect()
}

/// Reads one file, if it's there, keeping its report; `None` for a missing file.
fn read_file<T>(
    file: &'static str,
    text: Option<&str>,
    reports: &mut Vec<Report>,
    reader: impl FnOnce(&str, &mut Report) -> T,
) -> Option<T> {
    let mut report = Report::new(file);
    let read = reader(text?, &mut report);
    reports.push(report);
    Some(read)
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
        if let Some(key) = alignment.optional_text("metric", report) {
            match Metric::from_key(&key) {
                Some(metric) => balance.metric = metric,
                None => {
                    let keys = Metric::ALL.map(Metric::key);
                    let message = match suggest(&key, keys) {
                        Some(close) => format!("unknown metric '{key}' (did you mean '{close}'?)"),
                        None => format!(
                            "unknown metric '{key}': use {}, {} or {}",
                            keys[0], keys[1], keys[2]
                        ),
                    };
                    report.error(&alignment.path_to("metric"), message);
                }
            }
        }
        if let Some(weights) = read_weights(&mut alignment, "default_weights", report) {
            balance.default_weights = weights;
        }
        alignment.finish(report);
    }
    if let Some(mut disposition) = file.optional_table("disposition", "[disposition]", report) {
        if let Some(affinity) = disposition.optional_curve("affinity", report) {
            balance.affinity = affinity;
        }
        if let Some(bands) = read_bands(&mut disposition, report) {
            balance.bands = bands;
        }
        disposition.finish(report);
    }
    file.finish(report);
    balance
}

/// `disposition.bands`, lowest first. The checks across bands only run once every band has
/// been read, so each problem's path names the band it's about.
fn read_bands(section: &mut Section<'_>, report: &mut Report) -> Option<Bands> {
    const BAND: &str = "{ name = \"neutral\", up_to = 25.0 }";
    let example = format!("[{BAND}, {{ name = \"friendly\" }}]");
    let items = section.optional_list("bands", &example, report)?;
    let path = section.path_to("bands");
    let at = |index: usize| format!("{path}[{index}]");
    let mut bands = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Value::Table(fields) = item else {
            report.error(&at(index), format!("expected a table, like {BAND}"));
            bands.push(None);
            continue;
        };
        let mut band = Section::new(fields, at(index));
        let errors = report.diagnostics.len();
        let name = band.text("name", report);
        let up_to = band.optional_fixed("up_to", report);
        // A wrong `up_to` reads as none at all, so it would look open-ended: leave it out.
        let read = report.diagnostics.len() == errors;
        band.finish(report);
        bands.push(name.filter(|_| read).map(|name| Band { name, up_to }));
    }
    let bands: Vec<Band> = bands.into_iter().collect::<Option<_>>()?;
    match Bands::new(bands) {
        Ok(bands) => Some(bands),
        Err(problems) => {
            for problem in problems {
                let key = match problem {
                    BandProblem::NoBands => path.clone(),
                    BandProblem::InvalidName { index, .. }
                    | BandProblem::DuplicateName { index, .. } => format!("{}.name", at(index)),
                    BandProblem::MissingUpTo { index } => at(index),
                    BandProblem::LastHasUpTo { index }
                    | BandProblem::NotIncreasing { index, .. } => {
                        format!("{}.up_to", at(index))
                    }
                };
                report.error(&key, problem.to_string());
            }
            None
        }
    }
}

/// A file of tables keyed by id, such as `characters.toml`: each entry's id is checked, and
/// its fields are read by `read`. An entry with problems is reported and left out.
fn read_tables<Id: Ord + Clone, T>(
    text: &str,
    report: &mut Report,
    kind: &str,
    new_id: impl Fn(&str) -> Result<Id, InvalidId>,
    read: impl Fn(Id, &toml::Table, &mut Report) -> Option<T>,
) -> BTreeMap<Id, T> {
    let mut entries = BTreeMap::new();
    let Some(table) = report.parse(text) else {
        return entries;
    };
    for (key, value) in &table {
        let id = match new_id(key) {
            Ok(id) => id,
            Err(invalid) => {
                report.error(key, invalid.to_string());
                continue;
            }
        };
        let Value::Table(fields) = value else {
            report.error(
                key,
                format!("expected a table of {kind} fields, like [{key}]"),
            );
            continue;
        };
        if let Some(entry) = read(id.clone(), fields, report) {
            entries.insert(id, entry);
        }
    }
    entries
}

/// One character in `characters.toml`.
fn read_character(id: CharacterId, fields: &toml::Table, report: &mut Report) -> Option<Character> {
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let alignment = read_alignment(&mut section, report);
    let weights = read_weights(&mut section, "weights", report);
    let memberships = read_memberships(&mut section, report);
    section.finish(report);
    Some(Character {
        id,
        name: name?,
        alignment: alignment?,
        weights,
        memberships: memberships?,
    })
}

/// A character's optional `memberships = [{ faction = "lantern_guild" }]`, as listed. Whether
/// each faction exists is the world's check (P-32), reported at its `faction` key.
fn read_memberships(section: &mut Section<'_>, report: &mut Report) -> Option<Vec<FactionId>> {
    const MEMBERSHIP: &str = "{ faction = \"lantern_guild\" }";
    let Some(items) = section.optional_list("memberships", &format!("[{MEMBERSHIP}]"), report)
    else {
        return Some(Vec::new());
    };
    let path = section.path_to("memberships");
    let mut factions = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = format!("{path}[{index}]");
        let Value::Table(fields) = item else {
            report.error(&at, format!("expected a table, like {MEMBERSHIP}"));
            factions.push(None);
            continue;
        };
        let mut membership = Section::new(fields, at);
        let faction = membership.text("faction", report).and_then(|text| {
            FactionId::new(&text)
                .map_err(|invalid| {
                    report.error(&membership.path_to("faction"), invalid.to_string())
                })
                .ok()
        });
        membership.finish(report);
        factions.push(faction);
    }
    factions.into_iter().collect()
}

/// One faction in `factions.toml`.
fn read_faction(id: FactionId, fields: &toml::Table, report: &mut Report) -> Option<Faction> {
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let alignment = read_alignment(&mut section, report);
    let weights = read_weights(&mut section, "weights", report);
    let tolerance = section.fixed("tolerance", report);
    let member = section.optional_fixed("member_tolerance", report);
    let tolerances = tolerance.and_then(|tolerance| match Tolerances::new(tolerance, member) {
        Ok(tolerances) => Some(tolerances),
        Err(problem) => {
            let key = match problem {
                ToleranceProblem::Negative(_) => "tolerance",
                ToleranceProblem::MemberBelowTolerance { .. } => "member_tolerance",
            };
            report.error(&section.path_to(key), problem.to_string());
            None
        }
    });
    section.finish(report);
    Some(Faction {
        id,
        name: name?,
        alignment: alignment?,
        weights,
        tolerances: tolerances?,
    })
}

/// A required `alignment = { law = …, good = … }`, with both axes in range.
fn read_alignment(section: &mut Section<'_>, report: &mut Report) -> Option<Alignment> {
    let mut axes = section.table("alignment", "{ law = 0.0, good = 0.0 }", report)?;
    let law = axes.fixed("law", report);
    let good = axes.fixed("good", report);
    let alignment = match Alignment::new(law?, good?) {
        Ok(alignment) => Some(alignment),
        Err(out_of_range) => {
            for problem in out_of_range {
                report.error(&axes.path_to(problem.axis.key()), problem.to_string());
            }
            None
        }
    };
    axes.finish(report);
    alignment
}

/// Optional weights under `key`, such as `weights = { law = 1.0, good = 0.25 }`: both axes,
/// each 0.00–1.00, at least one above 0. `None` when they're left out, or wrong; anything
/// wrong is reported, which fails the whole load.
fn read_weights(
    section: &mut Section<'_>,
    key: &'static str,
    report: &mut Report,
) -> Option<Weights> {
    let mut axes = section.optional_table(key, "{ law = 1.0, good = 1.0 }", report)?;
    let law = axes.fixed("law", report);
    let good = axes.fixed("good", report);
    let weights = match Weights::new(law?, good?) {
        Ok(weights) => Some(weights),
        Err(problems) => {
            for problem in problems {
                let path = match problem {
                    WeightProblem::OutOfRange { axis, .. } => axes.path_to(axis.key()),
                    WeightProblem::AllZero => axes.path().to_owned(),
                };
                report.error(&path, problem.to_string());
            }
            None
        }
    };
    axes.finish(report);
    weights
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
    use factional_reputation::{CharacterId, FactionId, Metric, Weights};

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

    fn factions(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(text),
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
        assert!(content.factions.is_empty());
        assert!(content.actions.is_empty());
    }

    #[test]
    fn reads_the_label_threshold() {
        let content = balance("[alignment]\nlabel_threshold = 40").expect("valid content");
        assert_eq!(content.balance.label_threshold, h(40_00));
    }

    #[test]
    fn reads_the_metric_and_default_weights() {
        let content = balance(
            "[alignment]\nmetric = \"manhattan\"\ndefault_weights = { law = 0.5, good = 1.0 }",
        )
        .expect("valid content");
        assert_eq!(content.balance.metric, Metric::Manhattan);
        assert_eq!(
            content.balance.default_weights,
            Weights::new(h(50), h(1_00)).expect("valid")
        );
        for metric in ["euclidean", "chebyshev"] {
            let content =
                balance(&format!("[alignment]\nmetric = \"{metric}\"")).expect("valid content");
            assert_eq!(content.balance.metric.key(), metric);
        }
    }

    #[test]
    fn reads_the_affinity_curve_and_bands() {
        let content = balance(
            r#"
            [disposition]
            affinity = [[0.0, 100.0], [100.0, -100.0]]
            bands = [
              { name = "hostile", up_to = -30.0 },
              { name = "wary", up_to = 10 },
              { name = "warm" },
            ]
            "#,
        )
        .expect("valid content");
        assert_eq!(content.balance.affinity.at(h(25_00)), h(50_00));
        let bands: Vec<(&str, Option<Fixed>)> = content
            .balance
            .bands
            .iter()
            .map(|band| (band.name.as_str(), band.up_to))
            .collect();
        assert_eq!(
            bands,
            [
                ("hostile", Some(h(-30_00))),
                ("wary", Some(h(10_00))),
                ("warm", None)
            ]
        );
    }

    #[test]
    fn an_empty_disposition_section_keeps_the_defaults() {
        let content = balance("[disposition]").expect("valid content");
        assert_eq!(content.balance, Balance::default());
    }

    #[test]
    fn reports_mistakes_in_the_bands_at_their_keys() {
        let text = r#"
            [disposition]
            bands = [
              { name = "unfriendly", up_to = -25.0 },
              { name = "Very Cold", up_to = -30.0 },
              { name = "unfriendly" },
              { name = "friendly", up_to = 90.0, colour = "green" },
            ]
        "#;
        assert_eq!(
            problems(balance(text)),
            [
                "balance.toml: disposition.bands[3]: unknown key 'colour'",
                "balance.toml: disposition.bands[1].name: 'Very Cold' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "balance.toml: disposition.bands[1].up_to: -30.00 must be above the previous band's -25.00",
                "balance.toml: disposition.bands[2].name: another band is already called 'unfriendly'",
                "balance.toml: disposition.bands[2]: missing 'up_to': only the last band takes every score above the band before it",
                "balance.toml: disposition.bands[3].up_to: the last band can't have 'up_to': it takes every score above the band before it",
            ]
        );
    }

    #[test]
    fn reports_a_band_list_that_is_empty_or_not_a_list() {
        assert_eq!(
            problems(balance("[disposition]\nbands = []")),
            ["balance.toml: disposition.bands: there must be at least one band"]
        );
        assert_eq!(
            problems(balance("[disposition]\nbands = \"three\"")),
            [
                "balance.toml: disposition.bands: expected a list, like [{ name = \"neutral\", up_to = 25.0 }, { name = \"friendly\" }]"
            ]
        );
        // An entry that can't be read stops the checks across bands, so their paths stay true.
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [{ up_to = 3.0 }, { name = \"x\" }]"
            )),
            ["balance.toml: disposition.bands[0]: missing 'name'"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [{ name = \"x\", up_to = \"low\" }, { name = \"y\" }]"
            )),
            ["balance.toml: disposition.bands[0].up_to: expected a number, like 25.0"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [7, { name = \"x\", up_to = 1.0 }]"
            )),
            [
                "balance.toml: disposition.bands[0]: expected a table, like { name = \"neutral\", up_to = 25.0 }"
            ]
        );
    }

    #[test]
    fn reports_an_affinity_curve_that_is_invalid_or_out_of_range() {
        assert_eq!(
            problems(balance(
                "[disposition]\naffinity = [[0.0, 150.0], [60.0, 0.0]]"
            )),
            ["balance.toml: disposition.affinity: curve value 150.00 is outside -100.00 to 100.00"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\naffinity = [[60.0, 50.0], [0.0, 0.0]]"
            )),
            [
                "balance.toml: disposition.affinity: curve points must have increasing x: 60.00 then 0.00"
            ]
        );
        assert_eq!(
            problems(balance("[disposition]\naffinity = 20.0\nweights = 1")),
            ["balance.toml: disposition: unknown key 'weights'"]
        );
    }

    #[test]
    fn reports_an_unknown_metric() {
        assert_eq!(
            problems(balance("[alignment]\nmetric = \"euclidian\"")),
            [
                "balance.toml: alignment.metric: unknown metric 'euclidian' (did you mean 'euclidean'?)"
            ]
        );
        assert_eq!(
            problems(balance("[alignment]\nmetric = \"cosine\"")),
            [
                "balance.toml: alignment.metric: unknown metric 'cosine': use euclidean, manhattan or chebyshev"
            ]
        );
        assert_eq!(
            problems(balance("[alignment]\nmetric = 2")),
            ["balance.toml: alignment.metric: expected text in quotes"]
        );
        assert_eq!(
            problems(balance(
                "[alignment]\ndefault_weights = { law = 0.0, good = 0.0 }"
            )),
            ["balance.toml: alignment.default_weights: at least one weight must be above 0.00"]
        );
    }

    #[test]
    fn an_empty_alignment_section_keeps_the_default_threshold() {
        let content = balance("[alignment]").expect("valid content");
        assert_eq!(content.balance, Balance::default());
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
        for wrong in ["\"-20\"", "true", "[1.0]"] {
            let text = VEX.replace("-20.0", wrong);
            assert_eq!(
                problems(characters(&text)),
                ["characters.toml: vex.alignment.good: expected a number, like 25.0"],
                "{wrong}"
            );
        }
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

    // factions.toml

    const WATCH: &str = r#"
        [city_watch]
        name = "The City Watch"
        alignment = { law = 70.0, good = 20.0 }
        weights = { law = 1.0, good = 0.25 }
        tolerance = 40.0
        member_tolerance = 50.0
    "#;

    const GUILD: &str = r#"
        [lantern_guild]
        name = "The Lantern Guild"
        alignment = { law = -60.0, good = -10.0 }
        weights = { law = 1.0, good = 0.5 }
        tolerance = 45.0
        member_tolerance = 60.0
    "#;

    #[test]
    fn reads_factions_with_their_weights() {
        let text = format!(
            "{WATCH}\n[free_company]\nname = \"The Free Company\"\nalignment = {{ law = -10.0, good = 0.0 }}\ntolerance = 60.0"
        );
        let content = factions(&text).expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid id")];
        assert_eq!(watch.name, "The City Watch");
        assert_eq!(
            (watch.alignment.law(), watch.alignment.good()),
            (h(70_00), h(20_00))
        );
        assert_eq!(
            watch.weights,
            Some(Weights::new(h(1_00), h(25)).expect("valid"))
        );
        assert_eq!(
            (watch.tolerances.tolerance(), watch.tolerances.member()),
            (h(40_00), h(50_00))
        );
        let company = &content.factions[&FactionId::new("free_company").expect("valid id")];
        assert_eq!(company.weights, None, "no weights of its own");
        assert_eq!(
            company.tolerances.member(),
            h(60_00),
            "member tolerance defaults to the tolerance"
        );
    }

    #[test]
    fn reports_mistakes_in_a_faction() {
        let text = r#"
            guild = 3

            ["Lantern Guild"]
            name = "The Lantern Guild"

            [temple]
            alignment = { law = 30.0, good = 180.0 }
            tolerance = 35.0

            [watch]
            name = "The Watch"
            alignment = { law = 70.0, good = 20.0 }
            weights = { law = 1.5, good = 0.25 }
            drift = "flag"
        "#;
        assert_eq!(
            problems(factions(text)),
            [
                "factions.toml: Lantern Guild: 'Lantern Guild' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "factions.toml: guild: expected a table of faction fields, like [guild]",
                "factions.toml: temple: missing 'name'",
                "factions.toml: temple.alignment.good: 180.00 is outside -100.00..100.00",
                "factions.toml: watch.weights.law: 1.50 must be between 0.00 and 1.00",
                "factions.toml: watch: missing 'tolerance'",
                "factions.toml: watch: unknown key 'drift'",
            ]
        );
    }

    #[test]
    fn reports_weights_that_count_for_nothing_or_miss_an_axis() {
        assert_eq!(
            problems(factions(
                &WATCH.replace("law = 1.0, good = 0.25", "law = 0, good = 0.0")
            )),
            ["factions.toml: city_watch.weights: at least one weight must be above 0.00"]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("law = 1.0, good = 0.25", "law = 1.0")
            )),
            ["factions.toml: city_watch.weights: missing 'good'"]
        );
        assert_eq!(
            problems(factions(&WATCH.replace("weights = {", "weights = 1 #"))),
            ["factions.toml: city_watch.weights: expected a table, like { law = 1.0, good = 1.0 }"]
        );
    }

    #[test]
    fn reports_tolerances_that_are_negative_or_inverted() {
        assert_eq!(
            problems(factions(
                &WATCH.replace("tolerance = 40.0", "tolerance = -5.0")
            )),
            ["factions.toml: city_watch.tolerance: -5.00 must be at least 0.00"]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("member_tolerance = 50.0", "member_tolerance = 30.0")
            )),
            [
                "factions.toml: city_watch.member_tolerance: 30.00 must be at least the faction's tolerance, 40.00"
            ]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("tolerance = 40.0", "tolerance = \"far\"")
            )),
            ["factions.toml: city_watch.tolerance: expected a number, like 25.0"]
        );
    }

    fn world(factions: &str, characters: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(factions),
            characters: Some(characters),
            ..Sources::default()
        })
    }

    #[test]
    fn reads_starting_memberships() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"lantern_guild\" }}, {{ faction = \"city_watch\" }}]"
        );
        let content = world(&format!("{WATCH}{GUILD}"), &text).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        let factions: Vec<&str> = vex.memberships.iter().map(FactionId::as_str).collect();
        assert_eq!(factions, ["lantern_guild", "city_watch"], "as listed");
        let plain = characters(VEX).expect("valid content");
        assert!(
            plain.characters[&CharacterId::new("vex").expect("valid id")]
                .memberships
                .is_empty()
        );
    }

    #[test]
    fn a_membership_must_name_a_faction_that_exists_once() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"lantern_gild\" }}, {{ faction = \"lantern_guild\" }}, {{ faction = \"lantern_guild\" }}]"
        );
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
                "characters.toml: vex.memberships[2].faction: vex already belongs to lantern_guild",
            ]
        );
    }

    #[test]
    fn reports_mistakes_in_a_membership() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"Lantern Guild\" }}, {{}}, {{ faction = \"lantern_guild\", rank = \"fence\" }}, 3]"
        );
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].faction: 'Lantern Guild' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "characters.toml: vex.memberships[1]: missing 'faction'",
                "characters.toml: vex.memberships[2]: unknown key 'rank'",
                "characters.toml: vex.memberships[3]: expected a table, like { faction = \"lantern_guild\" }",
            ]
        );
        assert_eq!(
            problems(world(
                GUILD,
                &format!("{VEX}memberships = \"lantern_guild\"")
            )),
            [
                "characters.toml: vex.memberships: expected a list, like [{ faction = \"lantern_guild\" }]"
            ]
        );
    }

    #[test]
    fn warns_of_a_starting_member_outside_member_tolerance() {
        let reformed = VEX.replace("law = -55.0, good = -20.0", "law = 35.0, good = 10.0");
        let text = format!("{reformed}memberships = [{{ faction = \"lantern_guild\" }}]");
        let content = world(GUILD, &text).expect("a warning doesn't stop loading");
        let found: Vec<String> = warnings(&content)
            .iter()
            .map(Diagnostic::to_string)
            .collect();
        assert_eq!(
            found,
            [
                "characters.toml: vex.memberships[0]: vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00"
            ]
        );
        let faithful = world(
            GUILD,
            &format!("{VEX}memberships = [{{ faction = \"lantern_guild\" }}]"),
        )
        .expect("valid content");
        assert!(warnings(&faithful).is_empty());
    }

    #[test]
    fn reads_a_characters_own_weights() {
        let text = format!("{VEX}weights = {{ law = 0.5, good = 1.0 }}");
        let content = characters(&text).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(
            vex.weights,
            Some(Weights::new(h(50), h(1_00)).expect("valid"))
        );
        let plain = characters(VEX).expect("valid content");
        assert_eq!(
            plain.characters[&CharacterId::new("vex").expect("valid id")].weights,
            None
        );
        assert_eq!(
            problems(characters(&format!(
                "{VEX}weights = {{ law = -0.5, good = 1.0 }}"
            ))),
            ["characters.toml: vex.weights.law: -0.50 must be between 0.00 and 1.00"]
        );
    }

    #[test]
    fn a_faction_and_a_character_cannot_share_an_id() {
        let found = problems(parse_content(Sources {
            factions: Some(&WATCH.replace("[city_watch]", "[vex]")),
            characters: Some(VEX),
            ..Sources::default()
        }));
        assert_eq!(
            found,
            [
                "factions.toml: vex: 'vex' is also a character's id: factions and characters need different ids"
            ]
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
            factions: Some("[watch]\nname = \"The Watch\""),
        }));
        assert_eq!(
            found,
            [
                "balance.toml: alignment.label_threshold: 0.00 must be between 0.01 and 100.00",
                "factions.toml: watch: missing 'alignment'",
                "factions.toml: watch: missing 'tolerance'",
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

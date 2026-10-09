//! `outline <dir>` (U1): each content file, its entries, and every problem and warning at the
//! entry it's about, as the editor shows them; then the summary `validate` gives.

use std::path::Path;

use factional_content::Diagnostic;

use crate::check::validate;
use crate::session::Outcome;

/// `outline <dir>`, with `dir` read from `base`.
pub(crate) fn outline(base: &Path, args: &str) -> Outcome {
    let [dir] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Outcome::Error("outline needs the form: outline <dir>".to_owned());
    };
    let outline = factional_content::outline(&base.join(dir));
    let mut shown = Vec::new();
    for file in &outline.files {
        shown.push(file.label());
        shown.extend(diagnostics(&file.problems, &file.warnings, "  ", |d| {
            d.key.as_deref()
        }));
        for entry in &file.entries {
            shown.push(format!("  {}", entry.label()));
            shown.extend(diagnostics(&entry.problems, &entry.warnings, "    ", |d| {
                entry.at(d)
            }));
        }
    }
    shown.extend(
        outline
            .problems
            .iter()
            .map(|problem| format!("error: {problem}")),
    );
    let (report, _) = validate(base, dir);
    shown.extend(report.lines().last().map(str::to_owned));
    Outcome::Output(shown.join("\n"))
}

/// Each problem, then each warning, indented, with where it is if `at` says.
fn diagnostics<'d>(
    problems: &'d [Diagnostic],
    warnings: &'d [Diagnostic],
    indent: &'d str,
    at: impl Fn(&'d Diagnostic) -> Option<&'d str> + 'd,
) -> impl Iterator<Item = String> + 'd {
    let problems = problems.iter().map(move |d| ("error", d));
    let warnings = warnings.iter().map(move |d| ("warning", d));
    problems.chain(warnings).map(move |(kind, d)| match at(d) {
        Some(key) => format!("{indent}{kind}: {key}: {}", d.message),
        None => format!("{indent}{kind}: {}", d.message),
    })
}

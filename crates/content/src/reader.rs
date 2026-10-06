//! Reading TOML tables while collecting every problem with the file and key path it's at.

use factional_core::{Curve, Fixed, suggest};
use serde::Deserialize;
use toml::{Table, Value};

use crate::Diagnostic;

/// The problems found in one file.
pub(crate) struct Report {
    file: &'static str,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub(crate) fn new(file: &'static str) -> Report {
        Report {
            file,
            diagnostics: Vec::new(),
        }
    }

    /// Records a problem at `path`, such as `vex.alignment`; an empty path means the file.
    pub(crate) fn error(&mut self, path: &str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            file: self.file.to_owned(),
            key: (!path.is_empty()).then(|| path.to_owned()),
            message: message.into(),
        });
    }

    /// Parses the whole file as TOML; a syntax error is reported with its line.
    pub(crate) fn parse(&mut self, text: &str) -> Option<Table> {
        match text.parse::<Table>() {
            Ok(table) => Some(table),
            Err(error) => {
                let line = error
                    .span()
                    .map(|span| text[..span.start].matches('\n').count() + 1);
                let message = error.message().trim_end().to_owned();
                match line {
                    Some(line) => self.error("", format!("line {line}: {message}")),
                    None => self.error("", message),
                }
                None
            }
        }
    }
}

/// One TOML table being read. It remembers which keys were asked for, so `finish` can report
/// any others as unknown, with a suggestion when one is close.
pub(crate) struct Section<'t> {
    table: &'t Table,
    path: String,
    read: Vec<String>,
}

impl<'t> Section<'t> {
    pub(crate) fn new(table: &'t Table, path: String) -> Section<'t> {
        Section {
            table,
            path,
            read: Vec::new(),
        }
    }

    /// Where this section is, such as `vex.alignment`.
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    /// Every key in the table, for tables whose keys are ids rather than field names.
    pub(crate) fn keys(&self) -> Vec<String> {
        self.table.keys().cloned().collect()
    }

    /// Counts `key` as known, so `finish` won't call it unknown.
    pub(crate) fn mark(&mut self, key: &str) {
        self.read.push(key.to_owned());
    }

    /// The number under a key that isn't a fixed field name, such as an id in
    /// `factions = { city_watch = 10.0 }`. `None` if it's absent or not a number (that's
    /// reported).
    pub(crate) fn fixed_any(&mut self, key: &str, report: &mut Report) -> Option<Fixed> {
        self.mark(key);
        let value = self.table.get(key)?;
        self.to_fixed(key, value, report)
    }

    /// The table under a key that isn't a fixed field name, such as a profile's id in
    /// `[inertia.profiles.hardening]`. `None` if it's absent or not a table (that's reported).
    pub(crate) fn table_any(
        &mut self,
        key: &str,
        example: &str,
        report: &mut Report,
    ) -> Option<Section<'t>> {
        self.mark(key);
        let value = self.table.get(key)?;
        self.to_section(key, value, example, report)
    }

    /// Any value under `key`, as it is; `None` if it's absent.
    pub(crate) fn optional_value(&mut self, key: &'static str) -> Option<&'t Value> {
        self.get(key)
    }

    /// Whether any of `keys` is there. They all count as known keys, so `finish` won't call
    /// them unknown even when they're not read.
    pub(crate) fn has_any(&mut self, keys: &[&'static str]) -> bool {
        self.read.extend(keys.iter().map(|key| (*key).to_owned()));
        keys.iter().any(|key| self.table.contains_key(*key))
    }

    /// The path to `key` inside this section, such as `vex.alignment`.
    pub(crate) fn path_to(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    fn get(&mut self, key: &'static str) -> Option<&'t Value> {
        self.read.push(key.to_owned());
        self.table.get(key)
    }

    fn required(&mut self, key: &'static str, report: &mut Report) -> Option<&'t Value> {
        let value = self.get(key);
        if value.is_none() {
            report.error(&self.path, format!("missing '{key}'"));
        }
        value
    }

    /// A required string.
    pub(crate) fn text(&mut self, key: &'static str, report: &mut Report) -> Option<String> {
        match self.required(key, report)? {
            Value::String(text) => Some(text.clone()),
            _ => {
                report.error(&self.path_to(key), "expected text in quotes");
                None
            }
        }
    }

    /// A string that may be left out; `None` if it's absent or not a string (that's reported).
    pub(crate) fn optional_text(
        &mut self,
        key: &'static str,
        report: &mut Report,
    ) -> Option<String> {
        match self.get(key)? {
            Value::String(text) => Some(text.clone()),
            _ => {
                report.error(&self.path_to(key), "expected text in quotes");
                None
            }
        }
    }

    /// A required number.
    pub(crate) fn fixed(&mut self, key: &'static str, report: &mut Report) -> Option<Fixed> {
        let value = self.required(key, report)?;
        self.to_fixed(key, value, report)
    }

    /// A required whole number of at least 0, such as a count of ticks.
    pub(crate) fn whole(&mut self, key: &'static str, report: &mut Report) -> Option<u64> {
        let whole = match self.required(key, report)? {
            Value::Integer(whole) => u64::try_from(*whole).ok(),
            _ => None,
        };
        if whole.is_none() {
            report.error(&self.path_to(key), "expected a whole number, like 100");
        }
        whole
    }

    /// A number that may be left out; `None` if it's absent or wrong (a wrong one is reported).
    pub(crate) fn optional_fixed(
        &mut self,
        key: &'static str,
        report: &mut Report,
    ) -> Option<Fixed> {
        let value = self.get(key)?;
        self.to_fixed(key, value, report)
    }

    fn to_fixed(&self, key: &str, value: &Value, report: &mut Report) -> Option<Fixed> {
        if !matches!(value, Value::Integer(_) | Value::Float(_)) {
            report.error(&self.path_to(key), "expected a number, like 25.0");
            return None;
        }
        match Fixed::deserialize(value.clone()) {
            Ok(fixed) => Some(fixed),
            Err(error) => {
                report.error(&self.path_to(key), error.message());
                None
            }
        }
    }

    /// A curve that may be left out: a number, or `[x, y]` points (DESIGN.md §4.2). `None`
    /// if it's absent or invalid (that's reported).
    pub(crate) fn optional_curve(
        &mut self,
        key: &'static str,
        report: &mut Report,
    ) -> Option<Curve> {
        let value = self.get(key)?;
        match Curve::deserialize(value.clone()) {
            Ok(curve) => Some(curve),
            Err(error) => {
                report.error(&self.path_to(key), error.message());
                None
            }
        }
    }

    /// A list that may be left out, such as `bands = [...]`; `None` if it's absent or not a
    /// list (that's reported).
    pub(crate) fn optional_list(
        &mut self,
        key: &'static str,
        example: &str,
        report: &mut Report,
    ) -> Option<&'t Vec<Value>> {
        let value = self.get(key)?;
        self.to_list(key, value, example, report)
    }

    /// A required list, such as a rule table's `rules = [...]`.
    pub(crate) fn list(
        &mut self,
        key: &'static str,
        example: &str,
        report: &mut Report,
    ) -> Option<&'t Vec<Value>> {
        let value = self.required(key, report)?;
        self.to_list(key, value, example, report)
    }

    fn to_list(
        &self,
        key: &str,
        value: &'t Value,
        example: &str,
        report: &mut Report,
    ) -> Option<&'t Vec<Value>> {
        match value {
            Value::Array(items) => Some(items),
            _ => {
                report.error(
                    &self.path_to(key),
                    format!("expected a list, like {example}"),
                );
                None
            }
        }
    }

    /// A required table, such as `alignment = { law = 0.0, good = 0.0 }`; `example` shows the
    /// expected shape when something else is there.
    pub(crate) fn table(
        &mut self,
        key: &'static str,
        example: &str,
        report: &mut Report,
    ) -> Option<Section<'t>> {
        let value = self.required(key, report)?;
        self.to_section(key, value, example, report)
    }

    /// A table that may be left out, such as `[alignment]` in balance.toml.
    pub(crate) fn optional_table(
        &mut self,
        key: &'static str,
        example: &str,
        report: &mut Report,
    ) -> Option<Section<'t>> {
        let value = self.get(key)?;
        self.to_section(key, value, example, report)
    }

    fn to_section(
        &self,
        key: &str,
        value: &'t Value,
        example: &str,
        report: &mut Report,
    ) -> Option<Section<'t>> {
        match value {
            Value::Table(table) => Some(Section::new(table, self.path_to(key))),
            _ => {
                report.error(
                    &self.path_to(key),
                    format!("expected a table, like {example}"),
                );
                None
            }
        }
    }

    /// Reports every key in the table that was never asked for.
    pub(crate) fn finish(self, report: &mut Report) {
        for key in self.table.keys() {
            if self.read.contains(key) {
                continue;
            }
            let hint = suggest(key, self.read.iter().map(String::as_str))
                .map(|known| format!(" (did you mean '{known}'?)"))
                .unwrap_or_default();
            report.error(&self.path, format!("unknown key '{key}'{hint}"));
        }
    }
}

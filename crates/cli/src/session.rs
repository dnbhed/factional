use std::fmt;
use std::path::PathBuf;

use crate::charts;
use crate::check::{check, validate};
use crate::compare::report;

use factional_core::{Fixed, ParseFixedError, Ratio, Tick, article, suggest};
use factional_reputation::{
    ActionId, Alignment, AlignmentDelta, Axis, Change, Character, CharacterId, Command,
    ComponentKind, Condition, ConditionCheck, Distance, DriftPolicy, Event, Faction, FactionId,
    LeaveReason, ModifierId, ModifierObserver, Observed, Observer, OutcomeId, Party, RankCheck,
    RankRef, Shift, StandingEffects, TableDecision, TableSource, Toward, Verdict, WeightsFrom,
    Witnesses, World,
};

/// Every command as `(usage, description)`, in the order `help` lists them.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "list these commands"),
    ("quit", "leave the REPL, or end a script early"),
    (
        "load <dir>",
        "load the content files in <dir>, such as content/sample, as a new world",
    ),
    (
        "reload",
        "re-read the loaded content, replay this session's commands on it, and show what changed",
    ),
    (
        "validate <dir>",
        "check the content files in <dir> as load would, without loading them",
    ),
    ("characters", "list the loaded characters"),
    (
        "show character <id>",
        "a character's alignment and its label",
    ),
    (
        "factions",
        "list the factions, with their tolerances and members",
    ),
    (
        "show faction <id>",
        "a faction's alignment, label, tolerances and members",
    ),
    (
        "can-join <character> <faction> [--explain]",
        "whether <character> may join <faction> now, and why not",
    ),
    (
        "join <character> <faction>",
        "<character> joins <faction>, if they may",
    ),
    (
        "leave <character> <faction>",
        "<character> leaves <faction>",
    ),
    ("ranks <faction>", "a faction's rank ladder, lowest first"),
    (
        "promote <character> <faction> [--explain]",
        "move <character> up a rank, if the next rank's requirements hold",
    ),
    (
        "demote <character> <faction>",
        "move <character> down a rank",
    ),
    (
        "standing <subject> [<party>]",
        "how factions and characters regard <subject> from what has passed between them",
    ),
    ("outcomes", "list the outcomes, such as quest results"),
    (
        "outcome <outcome> <character>",
        "apply an outcome's effects to <character>",
    ),
    (
        "relations [<faction>]",
        "how factions regard each other, or just <faction>, with bands and conflicts",
    ),
    (
        "relate <from> <to> <value> [--one-way]",
        "set how <from> and <to> regard each other; --one-way sets only <from> → <to>",
    ),
    (
        "relate <from> <to> --by <n> [--one-way]",
        "shift a relation by <n>, stopping at -100 and 100",
    ),
    (
        "disposition <observer> <subject> [--explain]",
        "how <observer>, a faction or character, regards <subject>: a score and its band",
    ),
    (
        "distance <observer> <subject> [--explain]",
        "how far <subject> is from <observer>, a faction or character, as the observer sees it",
    ),
    (
        "modify <observer|everyone> <subject> <id> <amount> [--until <tick>]",
        "put a modifier on how <observer> regards <subject>, as another module would",
    ),
    ("unmodify <subject> <id>", "take a modifier off <subject>"),
    (
        "modifiers <subject>",
        "the modifiers on how <subject> is seen",
    ),
    (
        "resolve <character> <faction-to-keep>",
        "settle a war between two of <character>'s factions by leaving the other",
    ),
    (
        "faction-align <faction> <law> <good>",
        "set a faction's alignment, then review its members",
    ),
    (
        "faction-shift <faction> [--law <n>] [--good <n>]",
        "move a faction's alignment, stopping at -100 and 100, then review its members",
    ),
    (
        "watch <character>",
        "report whenever anyone's disposition toward <character> changes band",
    ),
    (
        "unwatch <character>",
        "stop reporting <character>'s band changes",
    ),
    (
        "watching",
        "the watched characters, and the band each observer puts them in",
    ),
    (
        "actions",
        "list the action catalogue and how each act moves alignment",
    ),
    (
        "act <actor> <action> [--target <id>] [--scale <n>] [--explain]",
        "<actor> does <action>; --scale says how big this instance was (default 1.00)",
    ),
    ("advance <ticks>", "move time forward"),
    ("time", "the current tick"),
    (
        "events [--since <seq>]",
        "what has happened, oldest first; --since shows only later events",
    ),
    (
        "journal",
        "every command issued, and whether it was accepted",
    ),
    (
        "calc <a> <op> <b>",
        "the engine's fixed-point arithmetic; <op> is + - * or /",
    ),
    (
        "curve <curve> [at <x>]",
        "a curve's value at <x>, or without at, the whole curve; <curve> is a knob, such as disposition.affinity, a number or [[x, y], ...]",
    ),
    (
        "map <faction>",
        "the alignment plane: who stands where, and the cells within <faction>'s tolerance",
    ),
    (
        "matrix [<subject>...] [--csv]",
        "every observer's disposition toward each subject (default: every character)",
    ),
    ("echo <text>", "print <text>"),
    ("fail <message>", "fail with <message>; for testing scripts"),
    (
        "assert <command> == <expected>",
        "check that <command>'s result is exactly <expected>",
    ),
];

/// What running one command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command ran; this is what it printed (possibly nothing).
    Output(String),
    /// The command ran and failed with this message.
    Error(String),
    /// The session should end.
    Quit,
}

impl Outcome {
    /// The text a person sees for this outcome; `assert` compares against exactly this.
    pub fn render(&self) -> String {
        match self {
            Outcome::Output(text) => text.clone(),
            Outcome::Error(message) => format!("error: {message}"),
            Outcome::Quit => String::new(),
        }
    }
}

/// A mistake in the input itself, rather than a command that ran and failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    UnknownCommand(String),
    MalformedAssert,
    AssertionFailed { expected: String, got: String },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::UnknownCommand(name) => write!(f, "unknown command '{name}'"),
            ScriptError::MalformedAssert => {
                f.write_str("assert needs the form: assert <command> == <expected>")
            }
            ScriptError::AssertionFailed { expected, got } => {
                write!(f, "expected '{expected}', got '{got}'")
            }
        }
    }
}

impl std::error::Error for ScriptError {}

/// One CLI session: the state that commands act on, shared by the REPL and the script runner.
#[derive(Debug)]
pub struct Session {
    /// Relative paths, as in `load content/sample`, are resolved against this.
    base_dir: PathBuf,
    world: Option<World>,
    /// The directory the world was loaded from, as typed, for `reload`.
    loaded: Option<String>,
}

impl Default for Session {
    /// A session whose relative paths start from the current directory.
    fn default() -> Session {
        Session::new(".")
    }
}

impl Session {
    pub fn new(base_dir: impl Into<PathBuf>) -> Session {
        Session {
            base_dir: base_dir.into(),
            world: None,
            loaded: None,
        }
    }

    /// The loaded world, if there is one.
    pub(crate) fn world(&self) -> Option<&World> {
        self.world.as_ref()
    }

    /// Runs one command line.
    pub fn execute(&mut self, line: &str) -> Result<Outcome, ScriptError> {
        let (name, rest) = split_command(line);
        match name {
            "help" => Ok(Outcome::Output(help_text())),
            "quit" => Ok(Outcome::Quit),
            "load" => Ok(self.load(rest)),
            "validate" => Ok(self.validate(rest)),
            "reload" => Ok(self.reload()),
            "characters" => Ok(self.characters()),
            "show" => Ok(self.show(rest)),
            "factions" => Ok(self.factions()),
            "distance" => Ok(self.distance(rest)),
            "disposition" => Ok(self.disposition(rest)),
            "watch" => Ok(self.watch(rest, true)),
            "unwatch" => Ok(self.watch(rest, false)),
            "watching" => Ok(self.watching()),
            "can-join" => Ok(self.can_join(rest)),
            "relations" => Ok(self.relations(rest)),
            "ranks" => Ok(self.ranks(rest)),
            "promote" => Ok(self.promote(rest)),
            "demote" => Ok(self.demote(rest)),
            "standing" => Ok(self.standing(rest)),
            "outcomes" => Ok(self.outcomes()),
            "outcome" => Ok(self.outcome(rest)),
            "relate" => Ok(self.relate(rest)),
            "faction-align" => Ok(self.faction_alignment(rest, false)),
            "resolve" => Ok(self.resolve(rest)),
            "modify" => Ok(self.modify(rest)),
            "unmodify" => Ok(self.unmodify(rest)),
            "modifiers" => Ok(self.modifiers(rest)),
            "faction-shift" => Ok(self.faction_alignment(rest, true)),
            "join" => Ok(self.membership(rest, true)),
            "leave" => Ok(self.membership(rest, false)),
            "actions" => Ok(self.actions()),
            "act" => Ok(self.act(rest)),
            "advance" => Ok(self.advance(rest)),
            "time" => Ok(self.time()),
            "events" => Ok(self.events(rest)),
            "journal" => Ok(self.journal()),
            "calc" => Ok(calc(rest)),
            "curve" => Ok(charts::curve(self.world.as_ref(), rest)),
            "map" => Ok(self
                .world
                .as_ref()
                .map_or_else(no_world, |w| charts::map(w, rest))),
            "matrix" => Ok(self
                .world
                .as_ref()
                .map_or_else(no_world, |w| charts::matrix(w, rest))),
            "echo" => Ok(Outcome::Output(rest.to_owned())),
            "fail" => Ok(Outcome::Error(rest.to_owned())),
            "assert" => self.assert(rest),
            _ => Err(ScriptError::UnknownCommand(name.to_owned())),
        }
    }

    /// `assert <command> == <expected>`: runs `<command>` and passes if its rendered result is
    /// exactly `<expected>`. A command that fails renders as `error: …`, so failures can be
    /// asserted on too; a mistake in the command itself is still a script error.
    fn assert(&mut self, spec: &str) -> Result<Outcome, ScriptError> {
        let (command, expected) = split_assert(spec).ok_or(ScriptError::MalformedAssert)?;
        let got = self.execute(command)?.render();
        if got == expected {
            Ok(Outcome::Output(String::new()))
        } else {
            Err(ScriptError::AssertionFailed {
                expected: expected.to_owned(),
                got,
            })
        }
    }

    /// `load <dir>`: reads and validates a content directory, replacing the loaded world only
    /// if every file is valid.
    fn load(&mut self, dir: &str) -> Outcome {
        if dir.is_empty() {
            return Outcome::Error("load needs the form: load <dir>".to_owned());
        }
        match check(&self.base_dir.join(dir)) {
            Ok(checked) => {
                let count = checked.counts[0];
                let noun = if count == 1 {
                    "character"
                } else {
                    "characters"
                };
                self.world = Some(checked.world);
                self.loaded = Some(dir.to_owned());
                let summary = format!("loaded {count} {noun} from {dir}");
                let warnings = checked
                    .warnings
                    .iter()
                    .map(|warning| format!("warning: {warning}"));
                Outcome::Output(lines(std::iter::once(summary).chain(warnings)))
            }
            Err(problems) => Outcome::Error(problems.join("\n")),
        }
    }

    /// `reload`: re-reads the directory last loaded, replays every command in the journal on
    /// the new world, and reports what changed. If the content no longer loads, or any
    /// command's acceptance differs from before, nothing changes (P-52).
    fn reload(&mut self) -> Outcome {
        let (Some(world), Some(dir)) = (&self.world, &self.loaded) else {
            return no_world();
        };
        let checked = match check(&self.base_dir.join(dir)) {
            Ok(checked) => checked,
            Err(problems) => return Outcome::Error(problems.join("\n")),
        };
        let mut replayed = checked.world;
        for (index, entry) in world.journal().iter().enumerate() {
            let now = replayed.execute(entry.command.clone());
            let changed = match (&entry.result, &now) {
                (Ok(()), Err(refusal)) => {
                    Some(format!("it was accepted, but now it's refused: {refusal}"))
                }
                (Err(refusal), Ok(_)) => {
                    Some(format!("it was refused ({refusal}), but now it's accepted"))
                }
                _ => None,
            };
            if let Some(changed) = changed {
                return Outcome::Error(format!(
                    "reload stopped at command {}, {}: {changed}. Nothing has changed.",
                    index + 1,
                    describe_command(&entry.command)
                ));
            }
        }
        let commands = match world.journal().len() {
            1 => "1 command".to_owned(),
            count => format!("{count} commands"),
        };
        let summary = format!("reloaded {dir} and replayed {commands}");
        let warnings = checked
            .warnings
            .iter()
            .map(|warning| format!("warning: {warning}"));
        let changes = report(("before", world), ("after", &replayed));
        let output = lines(std::iter::once(summary).chain(warnings).chain(changes));
        self.world = Some(replayed);
        Outcome::Output(output)
    }

    /// `validate <dir>`: what `load <dir>` would report, and a summary, without loading it.
    fn validate(&self, dir: &str) -> Outcome {
        if dir.is_empty() {
            return Outcome::Error("validate needs the form: validate <dir>".to_owned());
        }
        let (report, _) = validate(&self.base_dir, dir);
        Outcome::Output(report.trim_end().to_owned())
    }

    /// `characters`: every loaded character, one per line, in id order.
    fn characters(&self) -> Outcome {
        match &self.world {
            Some(world) => {
                let lines: Vec<String> = world
                    .characters()
                    .map(|character| describe(world, character))
                    .collect();
                Outcome::Output(lines.join("\n"))
            }
            None => no_world(),
        }
    }

    /// `show character <id>`.
    fn show(&self, args: &str) -> Outcome {
        let (kind, id) = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [kind @ ("character" | "faction"), id] => (kind, id),
            _ => {
                return Outcome::Error(
                    "show needs the form: show character <id>, or show faction <id>".to_owned(),
                );
            }
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let found = if kind == "character" {
            world
                .characters()
                .find(|character| character.id.as_str() == id)
                .map(|character| describe(world, character))
        } else {
            world
                .factions()
                .find(|faction| faction.id.as_str() == id)
                .map(|faction| describe_faction(world, faction))
        };
        match found {
            Some(line) => Outcome::Output(line),
            None => {
                let ids: Vec<&str> = if kind == "character" {
                    world.characters().map(|c| c.id.as_str()).collect()
                } else {
                    world.factions().map(|f| f.id.as_str()).collect()
                };
                Outcome::Error(format!("unknown {kind} '{id}'{}", hint(id, ids)))
            }
        }
    }
}

impl Session {
    /// `factions`: every faction, one per line, in id order.
    fn factions(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        Outcome::Output(lines(
            world
                .factions()
                .map(|faction| describe_faction(world, faction)),
        ))
    }

    /// `ranks <faction>`.
    fn ranks(&self, args: &str) -> Outcome {
        let [faction] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("ranks needs the form: ranks <faction>".to_owned());
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let Some(faction) = world.factions().find(|f| f.id.as_str() == faction) else {
            let ids: Vec<&str> = world.factions().map(|f| f.id.as_str()).collect();
            return Outcome::Error(format!("unknown faction '{faction}'{}", hint(faction, ids)));
        };
        Outcome::Output(lines(faction.ranks.iter().enumerate().map(
            |(index, rank)| {
                let needs: Vec<String> = [
                    rank.requires_standing
                        .map(|standing| format!("standing {standing}")),
                    rank.tolerance.map(|limit| format!("to be within {limit}")),
                ]
                .into_iter()
                .flatten()
                .collect();
                let mut line = format!("{}. {}", index + 1, rank.id);
                if !needs.is_empty() {
                    line += &format!(" — needs {}", needs.join(", and "));
                }
                line
            },
        )))
    }

    /// `promote <character> <faction> [--explain]`: the engine decides, and the event or its
    /// refusal is shown; with `--explain`, every requirement of the next rank.
    fn promote(&mut self, args: &str) -> Outcome {
        const USAGE: &str = "promote needs the form: promote <character> <faction> [--explain]";
        let (character, faction, explain) = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [character, faction] => (character, faction, false),
            [character, faction, "--explain"] => (character, faction, true),
            _ => return Outcome::Error(USAGE.to_owned()),
        };
        let (character, faction) = match member_ids(character, faction) {
            Ok(ids) => ids,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        if !explain {
            return match world.execute(Command::Promote { character, faction }) {
                Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
                Err(refusal) => Outcome::Error(refusal.to_string()),
            };
        }
        let Some(assessment) = world.assess_promotion(&character, &faction) else {
            // Not a member, or unknown: the engine's refusal says which.
            let refusal = world
                .clone()
                .execute(Command::Promote { character, faction })
                .expect_err("promoting a non-member is refused");
            return Outcome::Error(refusal.to_string());
        };
        let Some(next) = &assessment.next else {
            return Outcome::Output(format!(
                "{character}: {} in {faction} is the highest rank",
                assessment.current
            ));
        };
        let verdict = if assessment.allowed() { "yes" } else { "no" };
        let mut explained = vec![format!(
            "{character}: {} → {next} in {faction}: {verdict}",
            assessment.current
        )];
        explained.extend(assessment.checks.iter().map(|check| {
            let met = if check.met() { "met" } else { "not met" };
            match check {
                RankCheck::Standing { required, has } => {
                    format!("standing: needs {required}, has {has} — {met}")
                }
                RankCheck::Tolerance { limit, distance } => {
                    format!("tolerance: within {limit}, is {distance} — {met}")
                }
            }
        }));
        Outcome::Output(explained.join("\n"))
    }

    /// `demote <character> <faction>`.
    fn demote(&mut self, args: &str) -> Outcome {
        let [character, faction] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error(
                "demote needs the form: demote <character> <faction>".to_owned(),
            );
        };
        let (character, faction) = match member_ids(character, faction) {
            Ok(ids) => ids,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(Command::Demote { character, faction }) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `standing <subject> [<party>]`.
    fn standing(&self, args: &str) -> Outcome {
        let (subject, party) = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [subject] => (subject, None),
            [subject, party] => (subject, Some(party)),
            _ => {
                return Outcome::Error(
                    "standing needs the form: standing <subject> [<party>]".to_owned(),
                );
            }
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let Some(subject) = world.characters().find(|c| c.id.as_str() == subject) else {
            let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
            return Outcome::Error(format!(
                "unknown character '{subject}'{}",
                hint(subject, ids)
            ));
        };
        let Some(party) = party else {
            let standings = world
                .standings(&subject.id)
                .expect("a character from the world");
            if standings.is_empty() {
                return Outcome::Output("no standing with anyone yet".to_owned());
            }
            return Outcome::Output(lines(
                standings
                    .into_iter()
                    .map(|(party, value)| format!("{party}: {value}")),
            ));
        };
        let found = if let Some(faction) = world.factions().find(|f| f.id.as_str() == party) {
            Party::Faction(faction.id.clone())
        } else if let Some(character) = world.characters().find(|c| c.id.as_str() == party) {
            Party::Character(character.id.clone())
        } else {
            let factions = world.factions().map(|f| f.id.as_str());
            let ids: Vec<&str> = factions
                .chain(world.characters().map(|c| c.id.as_str()))
                .collect();
            return Outcome::Error(format!(
                "unknown faction or character '{party}'{}",
                hint(party, ids)
            ));
        };
        let value = world
            .standing(&subject.id, &found)
            .expect("both were found");
        Outcome::Output(value.to_string())
    }

    /// `outcomes`: the outcomes in content, with their effects.
    fn outcomes(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        Outcome::Output(lines(world.outcomes().map(|outcome| {
            let effects = &outcome.effects;
            let mut line = outcome.id.to_string();
            if effects.alignment != AlignmentDelta::default() {
                line += &format!(
                    " — alignment: law {}, good {}",
                    effects.alignment.law, effects.alignment.good
                );
            }
            let standing = named_effects(&effects.standing);
            if !standing.is_empty() {
                line += &format!(" — standing: {}", standing.join(", "));
            }
            line
        })))
    }

    /// `outcome <outcome> <character>`: applies an outcome, as a quest module would.
    fn outcome(&mut self, args: &str) -> Outcome {
        let [outcome, character] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error(
                "outcome needs the form: outcome <outcome> <character>".to_owned(),
            );
        };
        let outcome = match OutcomeId::new(outcome) {
            Ok(id) => id,
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        };
        let character = match CharacterId::new(character) {
            Ok(id) => id,
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(Command::ApplyOutcome { outcome, character }) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `relations [<faction>]`: every relation set, or those involving one faction.
    fn relations(&self, args: &str) -> Outcome {
        let only = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [] => None,
            [faction] => Some(faction),
            _ => {
                return Outcome::Error(
                    "relations needs the form: relations [<faction>]".to_owned(),
                );
            }
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        if let Some(faction) = only.filter(|id| !world.factions().any(|f| f.id.as_str() == *id)) {
            let ids: Vec<&str> = world.factions().map(|f| f.id.as_str()).collect();
            return Outcome::Error(format!("unknown faction '{faction}'{}", hint(faction, ids)));
        }
        let listed: Vec<String> = world
            .relations()
            .into_iter()
            .filter(|regard| {
                only.is_none_or(|id| regard.from.as_str() == id || regard.to.as_str() == id)
            })
            .map(|regard| {
                let conflict = world
                    .in_conflict(&regard.from, &regard.to)
                    .expect("factions from the world");
                let marker = if conflict { " — in conflict" } else { "" };
                format!(
                    "{} → {}: {} ({}){marker}",
                    regard.from, regard.to, regard.value, regard.band
                )
            })
            .collect();
        Outcome::Output(match (listed.is_empty(), only) {
            (false, _) => listed.join("\n"),
            (true, Some(faction)) => format!("no relations set for {faction}"),
            (true, None) => "no relations set".to_owned(),
        })
    }

    /// `relate <from> <to> <value> [--one-way]` or `relate <from> <to> --by <n> [--one-way]`:
    /// the engine decides, and the events or its refusal are shown.
    fn relate(&mut self, args: &str) -> Outcome {
        let command = match parse_relate(args) {
            Ok(command) => command,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `can-join <character> <faction> [--explain]`: whether the character may join now, from
    /// the engine's assessment, and with `--explain`, its working.
    fn can_join(&self, args: &str) -> Outcome {
        const USAGE: &str = "can-join needs the form: can-join <character> <faction> [--explain]";
        let (character, faction, explain) = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [character, faction] => (character, faction, false),
            [character, faction, "--explain"] => (character, faction, true),
            _ => return Outcome::Error(USAGE.to_owned()),
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let Some(character) = world.characters().find(|c| c.id.as_str() == character) else {
            let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
            return Outcome::Error(format!(
                "unknown character '{character}'{}",
                hint(character, ids)
            ));
        };
        let Some(faction) = world.factions().find(|f| f.id.as_str() == faction) else {
            let ids: Vec<&str> = world.factions().map(|f| f.id.as_str()).collect();
            return Outcome::Error(format!("unknown faction '{faction}'{}", hint(faction, ids)));
        };
        let assessment = world
            .assess_join(&character.id, &faction.id)
            .expect("both were found");
        let verdict = if assessment.allowed() { "yes" } else { "no" };
        if !explain {
            let why = if assessment.allowed() {
                format!(
                    "{} from {}, tolerance is {}",
                    assessment.distance.value, assessment.faction_name, assessment.tolerance
                )
            } else {
                assessment.reasons().join("; ")
            };
            return Outcome::Output(format!("{verdict}: {why}"));
        }
        let distance = &assessment.distance;
        let mut explained = vec![
            format!("{} → {}: {verdict}", character.id, faction.id),
            format!(
                "distance {} ({}), tolerance {}",
                distance.value,
                distance.metric.key(),
                assessment.tolerance
            ),
        ];
        explained.extend(working(distance, faction.id.as_str()));
        for defection in &assessment.defections {
            explained.push(format!(
                "leaving {}, in conflict with {} ({})",
                defection.from, faction.id, defection.relation
            ));
            for table in [&defection.deserters, &defection.defectors] {
                explained.extend(describe_table(table, &defection.from, &faction.id));
            }
        }
        explained.extend(
            assessment
                .reasons()
                .into_iter()
                .map(|reason| format!("refused: {reason}")),
        );
        Outcome::Output(explained.join("\n"))
    }

    /// `join <character> <faction>` or `leave <character> <faction>`: the engine decides, and
    /// the events or its refusal are shown.
    fn membership(&mut self, args: &str, joining: bool) -> Outcome {
        let name = if joining { "join" } else { "leave" };
        let [character, faction] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error(format!(
                "{name} needs the form: {name} <character> <faction>"
            ));
        };
        let character = match CharacterId::new(character) {
            Ok(id) => id,
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        };
        let faction = match FactionId::new(faction) {
            Ok(id) => id,
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        let command = if joining {
            Command::JoinFaction { character, faction }
        } else {
            Command::LeaveFaction { character, faction }
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `disposition <observer> <subject> [--explain]`: how the observer regards the subject,
    /// and with `--explain`, the working the engine returned.
    fn disposition(&self, args: &str) -> Outcome {
        let (world, query) = match self.judgement("disposition", args) {
            Ok(found) => found,
            Err(failed) => return failed,
        };
        let regard = world
            .disposition(&query.observer, &query.subject)
            .expect("both were found");
        let summary = format!("{} ({})", regard.score, regard.band);
        if !query.explain {
            return Outcome::Output(summary);
        }
        let distance = &regard.distance;
        let mut explained = vec![format!(
            "{} → {}: {summary}",
            query.observer_id, query.subject
        )];
        // Whose lack of factions explains an empty kinship or faction opinion.
        let observer_unaffiliated = match &query.observer {
            Observer::Character(id) => world
                .memberships(id)
                .expect("the observer was found")
                .next()
                .is_none(),
            Observer::Faction(_) => false,
        };
        let unaffiliated = if observer_unaffiliated {
            query.observer_id.to_owned()
        } else {
            query.subject.to_string()
        };
        for component in &regard.components {
            let mut line = format!(
                "{}: {} × {} = {}",
                component.kind.key().replace('_', " "),
                component.value,
                component.weight,
                component.weighted
            );
            match component.kind {
                ComponentKind::Affinity => {
                    line += &format!(
                        ", at distance {} ({})",
                        distance.value,
                        distance.metric.key()
                    );
                    explained.push(line);
                    explained.extend(
                        working(distance, query.observer_id)
                            .into_iter()
                            .map(|working| format!("  {working}")),
                    );
                    continue;
                }
                ComponentKind::Kinship | ComponentKind::FactionOpinion => {
                    let parts: Vec<String> = component
                        .parts
                        .iter()
                        .map(|part| match &part.to {
                            Some(to) if *to == part.from => {
                                format!("both in {} {}", part.from, part.value)
                            }
                            Some(to) => format!("{} → {to} {}", part.from, part.value),
                            None => format!("{} {}", part.from, part.value),
                        })
                        .collect();
                    let note = if !parts.is_empty() {
                        parts.join(", ")
                    } else if component.kind == ComponentKind::FactionOpinion
                        && matches!(query.observer, Observer::Faction(_))
                    {
                        "only characters have one".to_owned()
                    } else {
                        format!("{unaffiliated} belongs to no factions")
                    };
                    line += &format!(" ({note})");
                }
                ComponentKind::Modifiers if !component.modifiers.is_empty() => {
                    let modifiers: Vec<String> = component
                        .modifiers
                        .iter()
                        .map(|modifier| {
                            format!(
                                "{} {} from {}",
                                modifier.id, modifier.amount, modifier.observer
                            )
                        })
                        .collect();
                    line += &format!(" ({})", modifiers.join("; "));
                }
                ComponentKind::Standing | ComponentKind::Modifiers => {}
            }
            explained.push(line);
        }
        explained.push(format!("bands: {}", describe_bands(world)));
        Outcome::Output(explained.join("\n"))
    }

    /// `distance <observer> <subject> [--explain]`: how far the subject is from the observer,
    /// and with `--explain`, the working the engine returned.
    fn distance(&self, args: &str) -> Outcome {
        let (world, query) = match self.judgement("distance", args) {
            Ok(found) => found,
            Err(failed) => return failed,
        };
        let measured = world
            .distance(&query.observer, &query.subject)
            .expect("both were found");
        if !query.explain {
            return Outcome::Output(measured.value.to_string());
        }
        let mut explained = vec![format!(
            "{} → {}: {} ({})",
            query.observer_id,
            query.subject,
            measured.value,
            measured.metric.key()
        )];
        explained.extend(working(&measured, query.observer_id));
        Outcome::Output(explained.join("\n"))
    }

    /// Reads `<observer> <subject> [--explain]` for `command` and finds both in the world:
    /// the observer is a faction or a character, the subject a character.
    fn judgement<'a>(
        &self,
        command: &str,
        args: &'a str,
    ) -> Result<(&World, Judgement<'a>), Outcome> {
        let (observer_id, subject_id, explain) = match args.split_whitespace().collect::<Vec<_>>()[..]
        {
            [observer, subject] => (observer, subject, false),
            [observer, subject, "--explain"] => (observer, subject, true),
            _ => {
                return Err(Outcome::Error(format!(
                    "{command} needs the form: {command} <observer> <subject> [--explain]"
                )));
            }
        };
        let Some(world) = &self.world else {
            return Err(no_world());
        };
        let faction_ids = || world.factions().map(|faction| faction.id.as_str());
        let character_ids = || world.characters().map(|character| character.id.as_str());
        let character = |id: &str| world.characters().find(|c| c.id.as_str() == id);
        let observer =
            if let Some(faction) = world.factions().find(|f| f.id.as_str() == observer_id) {
                Observer::Faction(faction.id.clone())
            } else if let Some(found) = character(observer_id) {
                Observer::Character(found.id.clone())
            } else {
                let ids: Vec<&str> = faction_ids().chain(character_ids()).collect();
                return Err(Outcome::Error(format!(
                    "unknown observer '{observer_id}'{}",
                    hint(observer_id, ids)
                )));
            };
        let Some(subject) = character(subject_id) else {
            if faction_ids().any(|id| id == subject_id) {
                return Err(Outcome::Error(format!(
                    "a {command}'s subject must be a character"
                )));
            }
            let ids: Vec<&str> = character_ids().collect();
            return Err(Outcome::Error(format!(
                "unknown subject '{subject_id}'{}",
                hint(subject_id, ids)
            )));
        };
        Ok((
            world,
            Judgement {
                observer,
                observer_id,
                subject: subject.id.clone(),
                explain,
            },
        ))
    }

    /// `actions`: the action catalogue, in id order.
    fn actions(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        Outcome::Output(lines(world.actions().map(|action| {
            let delta = action.alignment;
            let standing = &action.standing;
            let mut effects: Vec<String> = [
                ("target", standing.target),
                ("target_factions", standing.target_factions),
            ]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| format!("{key} {value}")))
            .collect();
            effects.extend(named_effects(&standing.named));
            let mut line = format!("{} — law {}, good {}", action.id, delta.law, delta.good);
            if !effects.is_empty() {
                line += &format!(" — standing: {}", effects.join(", "));
            }
            line
        })))
    }

    /// `act <actor> <action> [--target <id>] [--scale <n>]`: performs an action and shows the
    /// events it caused.
    fn act(&mut self, args: &str) -> Outcome {
        let (command, explain) = match parse_act(args) {
            Ok(parsed) => parsed,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        // The working comes from where the actor stands before the act.
        let working = match &command {
            Command::PerformAction {
                actor,
                action,
                target,
                scale,
                ..
            } if explain => world
                .action_shift(actor, action, target.as_ref(), *scale)
                .map(|shift| describe_shift(&shift, actor, target.as_ref())),
            _ => None,
        };
        match world.execute(command) {
            Ok(events) => {
                let mut output: Vec<String> = events.iter().map(describe_event).collect();
                output.extend(working.into_iter().flatten());
                Outcome::Output(output.join("\n"))
            }
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `advance <ticks>`: moves time forward and shows the events it caused.
    fn advance(&mut self, args: &str) -> Outcome {
        let [ticks] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("advance needs the form: advance <ticks>".to_owned());
        };
        let Ok(ticks) = ticks.parse::<u64>() else {
            return Outcome::Error(format!("'{ticks}' is not a whole number of ticks"));
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(Command::AdvanceTime { ticks }) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `faction-align <faction> <law> <good>` or `faction-shift <faction> [--law <n>]
    /// [--good <n>]`: the engine decides, and the events or its refusal are shown.
    fn faction_alignment(&mut self, args: &str, shifting: bool) -> Outcome {
        let command = match parse_faction_alignment(args, shifting) {
            Ok(command) => command,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `modify <observer|everyone> <subject> <id> <amount> [--until <tick>]`: the engine
    /// decides, and the events or its refusal are shown. The observer is a faction if one
    /// has that id, otherwise a character.
    fn modify(&mut self, args: &str) -> Outcome {
        const USAGE: &str = "modify needs the form: modify <observer|everyone> <subject> <id> <amount> [--until <tick>]";
        let words: Vec<&str> = args.split_whitespace().collect();
        let (observer, subject, id, amount, until) = match words[..] {
            [observer, subject, id, amount] => (observer, subject, id, amount, None),
            [observer, subject, id, amount, "--until", tick] if !is_flag(tick) => {
                (observer, subject, id, amount, Some(tick))
            }
            _ => return Outcome::Error(USAGE.to_owned()),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        let parsed = (|| -> Result<Command, String> {
            let observer = if observer == "everyone" {
                ModifierObserver::Everyone
            } else if let Some(faction) = world.factions().find(|f| f.id.as_str() == observer) {
                ModifierObserver::Faction(faction.id.clone())
            } else {
                ModifierObserver::Character(
                    CharacterId::new(observer).map_err(|invalid| invalid.to_string())?,
                )
            };
            let expires_at = match until {
                Some(tick) => Some(Tick(
                    tick.parse()
                        .map_err(|_| format!("'{tick}' is not a whole tick"))?,
                )),
                None => None,
            };
            Ok(Command::AddModifier {
                id: ModifierId::new(id).map_err(|invalid| invalid.to_string())?,
                observer,
                subject: CharacterId::new(subject).map_err(|invalid| invalid.to_string())?,
                amount: amount
                    .parse()
                    .map_err(|error: ParseFixedError| error.to_string())?,
                expires_at,
            })
        })();
        let command = match parsed {
            Ok(command) => command,
            Err(message) => return Outcome::Error(message),
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `unmodify <subject> <id>`: the engine decides, and the events or its refusal are
    /// shown.
    fn unmodify(&mut self, args: &str) -> Outcome {
        let [subject, id] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("unmodify needs the form: unmodify <subject> <id>".to_owned());
        };
        let command = match (CharacterId::new(subject), ModifierId::new(id)) {
            (Ok(subject), Ok(id)) => Command::RemoveModifier { subject, id },
            (Err(invalid), _) | (_, Err(invalid)) => return Outcome::Error(invalid.to_string()),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `modifiers <subject>`: the modifiers on how the subject is seen, in id order.
    fn modifiers(&self, args: &str) -> Outcome {
        let [subject] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error("modifiers needs the form: modifiers <subject>".to_owned());
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let Some(character) = world.characters().find(|c| c.id.as_str() == subject) else {
            let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
            return Outcome::Error(format!(
                "unknown character '{subject}'{}",
                hint(subject, ids)
            ));
        };
        let modifiers = world
            .modifiers(&character.id)
            .expect("a character from the world");
        if modifiers.is_empty() {
            return Outcome::Output(format!("{} has no modifiers", character.id));
        }
        Outcome::Output(lines(modifiers.iter().map(|(modifier, until)| {
            let until = until
                .map(|tick| format!(", until tick {tick}"))
                .unwrap_or_default();
            format!(
                "{}: {} from {}{until}",
                modifier.id, modifier.amount, modifier.observer
            )
        })))
    }

    /// `resolve <character> <faction-to-keep>`: the engine decides, and the events or its
    /// refusal are shown.
    fn resolve(&mut self, args: &str) -> Outcome {
        let [character, keep] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error(
                "resolve needs the form: resolve <character> <faction-to-keep>".to_owned(),
            );
        };
        let (character, keep) = match member_ids(character, keep) {
            Ok(ids) => ids,
            Err(message) => return Outcome::Error(message),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        match world.execute(Command::ResolveConflict { character, keep }) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `watch <character>` or `unwatch <character>`: the engine decides, and the events or
    /// its refusal are shown.
    fn watch(&mut self, args: &str, watching: bool) -> Outcome {
        let name = if watching { "watch" } else { "unwatch" };
        let [subject] = args.split_whitespace().collect::<Vec<_>>()[..] else {
            return Outcome::Error(format!("{name} needs the form: {name} <character>"));
        };
        let subject = match CharacterId::new(subject) {
            Ok(id) => id,
            Err(invalid) => return Outcome::Error(invalid.to_string()),
        };
        let Some(world) = self.world.as_mut() else {
            return no_world();
        };
        let command = if watching {
            Command::Watch { subject }
        } else {
            Command::Unwatch { subject }
        };
        match world.execute(command) {
            Ok(events) => Outcome::Output(lines(events.iter().map(describe_event))),
            Err(refusal) => Outcome::Error(refusal.to_string()),
        }
    }

    /// `watching`: each watched character, with the observers in each band, lowest band
    /// first.
    fn watching(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        let watched: Vec<String> = world
            .watched()
            .map(|(subject, remembered)| {
                let groups: Vec<String> = world
                    .balance()
                    .bands
                    .iter()
                    .filter_map(|band| {
                        let observers: Vec<String> = remembered
                            .iter()
                            .filter(|(_, name)| **name == band.name)
                            .map(|(observer, _)| observer.to_string())
                            .collect();
                        (!observers.is_empty())
                            .then(|| format!("{}: {}", band.name, observers.join(", ")))
                    })
                    .collect();
                format!("{subject} — {}", groups.join("; "))
            })
            .collect();
        if watched.is_empty() {
            Outcome::Output("no one is watched yet".to_owned())
        } else {
            Outcome::Output(watched.join("\n"))
        }
    }

    /// `time`: the current tick.
    fn time(&self) -> Outcome {
        match &self.world {
            Some(world) => Outcome::Output(format!("tick {}", world.now())),
            None => no_world(),
        }
    }

    /// `events [--since <seq>]`: what has happened, oldest first.
    fn events(&self, args: &str) -> Outcome {
        let since = match args.split_whitespace().collect::<Vec<_>>()[..] {
            [] => None,
            ["--since", seq] => match seq.parse::<u64>() {
                Ok(seq) => Some(seq),
                Err(_) => return Outcome::Error(format!("'{seq}' is not an event number")),
            },
            _ => {
                return Outcome::Error("events needs the form: events [--since <seq>]".to_owned());
            }
        };
        let Some(world) = &self.world else {
            return no_world();
        };
        let events = world.events_since(since.unwrap_or(0));
        Outcome::Output(match (events.is_empty(), since) {
            (false, _) => lines(events.iter().map(describe_event)),
            (true, Some(seq)) => format!("no events after #{seq}"),
            (true, None) => "no events yet".to_owned(),
        })
    }

    /// `journal`: every command issued, and whether it was accepted.
    fn journal(&self) -> Outcome {
        let Some(world) = &self.world else {
            return no_world();
        };
        if world.journal().is_empty() {
            return Outcome::Output("no commands yet".to_owned());
        }
        Outcome::Output(lines(world.journal().iter().enumerate().map(
            |(index, entry)| {
                let result = match &entry.result {
                    Ok(()) => "accepted".to_owned(),
                    Err(refusal) => format!("refused: {refusal}"),
                };
                format!(
                    "{}. {} — {result}",
                    index + 1,
                    describe_command(&entry.command)
                )
            },
        )))
    }
}

/// How an act moved `actor`, axis by axis, from the engine's working (DESIGN.md §5.2–5.4):
/// each multiplier, and where it came from. Target multipliers appear only for an act with a
/// `target` whose action has the curves.
fn describe_shift(shift: &Shift, actor: &CharacterId, target: Option<&CharacterId>) -> Vec<String> {
    let mut axes = Vec::new();
    let mut targeted = false;
    for axis in &shift.axes {
        let mut multipliers = Vec::new();
        if let (Some((at, multiplier)), Some(target)) = (axis.by_target, target) {
            multipliers.push(format!(
                "{multiplier} (by_target.{} at {target}'s {at})",
                axis.axis.key()
            ));
        }
        if let Some((relation, multiplier)) = &axis.by_relation {
            let at = match &relation.between {
                Some((from, to)) => format!("{from} → {to} {}", relation.value),
                None => format!("{}: no relation between their factions", relation.value),
            };
            multipliers.push(format!("{multiplier} (by_target.relation at {at})"));
        }
        if !multipliers.is_empty() {
            targeted = true;
        }
        let toward = Toward::of(axis.axis, axis.base).expect("only moved axes have working");
        let curve = format!("{}.{}", axis.axis.key(), toward.key());
        multipliers.push(match axis.inertia {
            Some((_, multiplier)) => format!("{multiplier} ({curve} at {})", axis.from),
            None => format!("{} ({} has no {curve} curve)", Ratio::ONE, shift.profile),
        });
        axes.push(format!(
            "{}: {} × {} × {} = {}, from {} to {}",
            axis.axis.key(),
            axis.base,
            axis.scale,
            multipliers.join(" × "),
            axis.shift,
            axis.from,
            axis.to
        ));
    }
    let formula = if targeted {
        "base × scale × target × inertia"
    } else {
        "base × scale × inertia"
    };
    let mut described = vec![format!(
        "shift = {formula}, rounded once; {actor}'s inertia profile is {}",
        shift.profile
    )];
    described.extend(axes);
    described
}

/// How a `defectors` or `deserters` table decided: where it came from, the rule that fired
/// and what it decided, then each rule tried. `current` is the faction being left and
/// `target` the one being joined.
fn describe_table(table: &TableDecision, current: &FactionId, target: &FactionId) -> Vec<String> {
    let source = match table.source {
        TableSource::World => "balance.toml",
        TableSource::Faction => "factions.toml",
        TableSource::BuiltIn => "built in",
    };
    let verdict = match &table.verdict {
        Verdict::Allow { standing_change } if *standing_change == Fixed::ZERO => {
            table.kind.allow_key().to_owned()
        }
        Verdict::Allow { standing_change } => {
            format!("{}, standing {standing_change}", table.kind.allow_key())
        }
        Verdict::Refuse {
            reason: Some(reason),
        } => format!("refuse, \"{reason}\""),
        Verdict::Refuse { reason: None } => "refuse".to_owned(),
    };
    let mut lines = vec![format!(
        "{}'s {} ({source}): rule {}, {verdict}",
        table.faction,
        table.kind,
        table.fired() + 1
    )];
    for rule in &table.tried {
        let checks = if rule.checks.is_empty() {
            "always".to_owned()
        } else {
            let checks: Vec<String> = rule
                .checks
                .iter()
                .map(|check| describe_check(check, current, target))
                .collect();
            checks.join("; ")
        };
        lines.push(format!("  rule {}: {checks}", rule.index + 1));
    }
    lines
}

/// One condition as content writes it, whether it held, and what it was checked against:
/// `rank_at_least = 3? no, cutpurse is rung 1`.
fn describe_check(check: &ConditionCheck, current: &FactionId, target: &FactionId) -> String {
    let named = match &check.condition {
        Condition::RankAtLeast(named) | Condition::RankBelow(named) => Some(named),
        _ => None,
    };
    let value = match &check.condition {
        Condition::RankAtLeast(RankRef::Rung(rung)) | Condition::RankBelow(RankRef::Rung(rung)) => {
            rung.to_string()
        }
        Condition::RankAtLeast(RankRef::Id(id)) | Condition::RankBelow(RankRef::Id(id)) => {
            format!("\"{id}\"")
        }
        Condition::StandingWithCurrentAtLeast(value)
        | Condition::StandingWithCurrentBelow(value)
        | Condition::StandingWithTargetAtLeast(value)
        | Condition::StandingWithTargetBelow(value) => value.to_string(),
        Condition::CloserToTarget(flag) | Condition::OutsideMemberTolerance(flag) => {
            flag.to_string()
        }
    };
    let observed = match &check.observed {
        Observed::Rank {
            rank,
            rung,
            named: at,
        } => match named {
            Some(RankRef::Id(id)) => format!("{rank} is rung {rung}, {id} is rung {at}"),
            _ => format!("{rank} is rung {rung}"),
        },
        Observed::Standing(standing) => format!("standing {standing}"),
        Observed::Distances {
            target: to_target,
            current: to_current,
        } => format!("{to_target} from {target}, {to_current} from {current}"),
        Observed::Tolerance {
            distance,
            tolerance,
        } => format!("{distance} from {current}, tolerance {tolerance}"),
    };
    let held = if check.held { "yes" } else { "no" };
    format!("{} = {value}? {held}, {observed}", check.condition.key())
}

/// `#1 at tick 0: time advanced from 0 to 5`.
pub(crate) fn describe_event(event: &Event) -> String {
    let what = match &event.payload {
        Change::TimeAdvanced { from, to } => format!("time advanced from {from} to {to}"),
        Change::ActionPerformed {
            actor,
            action,
            target,
            scale,
            witnesses,
        } => {
            let mut what = format!("{actor} did {action}");
            if let Some(target) = target {
                what += &format!(", targeting {target}");
            }
            if *scale != Fixed::ONE {
                what += &format!(", at scale {scale}");
            }
            match witnesses {
                Witnesses::Everyone => {}
                Witnesses::Nobody => what += ", witnessed by nobody",
                Witnesses::These(ids) => {
                    let ids: Vec<&str> = ids.iter().map(CharacterId::as_str).collect();
                    what += &format!(", witnessed by {}", ids.join(", "));
                }
            }
            what
        }
        Change::JoinedFaction {
            character,
            faction,
            rank,
        } => format!(
            "{character} joined {faction} as {} {rank}",
            article(rank.as_str())
        ),
        Change::RankChanged {
            character,
            faction,
            from,
            to,
        } => format!("{character}'s rank in {faction} changed from {from} to {to}"),
        Change::LeftFaction {
            character,
            faction,
            reason,
        } => {
            let reason = match reason {
                LeaveReason::Voluntary => "voluntary",
                LeaveReason::Defected => "defected",
                LeaveReason::Expelled => "expelled",
                LeaveReason::ConflictResolved => "the war between their factions was settled",
            };
            format!("{character} left {faction} ({reason})")
        }
        Change::RelationChanged {
            from,
            to,
            before,
            after,
        } => format!("{from} → {to} changed from {before} to {after}"),
        Change::StandingChanged {
            subject,
            party,
            before,
            after,
            spilled,
        } => {
            let mut moved =
                format!("{subject}'s standing with {party} moved from {before} to {after}");
            let spills: Vec<String> = spilled
                .iter()
                .map(|spill| {
                    format!(
                        "{} spilled from {} ({} × {}; {party} regards it at {})",
                        spill.amount, spill.from, spill.change, spill.multiplier, spill.relation
                    )
                })
                .collect();
            if !spills.is_empty() {
                moved += &format!(", with {}", spills.join(", and "));
            }
            moved
        }
        Change::OutcomeApplied { outcome, character } => {
            format!("outcome {outcome} applied to {character}")
        }
        Change::EffectsApplied { source, character } => {
            format!("effects from {source} applied to {character}")
        }
        Change::Watched { subject, bands } => {
            format!("now watching {subject} ({} observers)", bands.len())
        }
        Change::Unwatched { subject } => format!("no longer watching {subject}"),
        Change::ProbationStarted {
            character,
            faction,
            until,
        } => format!("{character} is on probation with {faction} until tick {until}"),
        Change::ProbationCleared { character, faction } => {
            format!("{character}'s probation with {faction} is cleared")
        }
        Change::ProbationExpired { character, faction } => {
            format!("{character}'s probation with {faction} ran out")
        }
        Change::MembershipConflict {
            character,
            factions: (a, b),
        } => format!("{a} and {b} are now in conflict, and {character} belongs to both"),
        Change::MembershipConflictEnded {
            character,
            factions: (a, b),
        } => format!("{a} and {b} are no longer in conflict, so {character} keeps both"),
        Change::ModifierAdded {
            subject,
            id,
            observer,
            amount,
            expires_at,
        } => {
            let until = expires_at
                .map(|tick| format!(", until tick {tick}"))
                .unwrap_or_default();
            format!("modifier {id} on {subject}: {amount} from {observer}{until}")
        }
        Change::ModifierRemoved { subject, id } => format!("modifier {id} on {subject} removed"),
        Change::ModifierExpired { subject, id } => format!("modifier {id} on {subject} expired"),
        Change::FactionAlignmentChanged { faction, from, to } => format!(
            "{faction}'s alignment moved from {} to {}",
            axes(*from),
            axes(*to)
        ),
        Change::MemberOutOfTolerance {
            character,
            faction,
            distance,
            tolerance,
        } => format!(
            "{character} has drifted out of {faction}'s tolerance: {distance} from it, tolerance {tolerance}"
        ),
        Change::MemberBackInTolerance {
            character,
            faction,
            distance,
            tolerance,
        } => format!(
            "{character} is back within {faction}'s tolerance: {distance} from it, tolerance {tolerance}"
        ),
        Change::DispositionBandChanged {
            observer,
            subject,
            from,
            to,
            score,
        } => format!("{observer} now regards {subject} as {to} (was {from}), at {score}"),
        Change::AlignmentChanged {
            character,
            from,
            to,
        } => format!(
            "{character}'s alignment moved from {} to {}",
            axes(*from),
            axes(*to)
        ),
    };
    format!("#{} at tick {}: {what}", event.seq, event.tick)
}

/// A command written the way it's typed in the CLI.
fn describe_command(command: &Command) -> String {
    match command {
        Command::AdvanceTime { ticks } => format!("advance {ticks}"),
        Command::JoinFaction { character, faction } => format!("join {character} {faction}"),
        Command::Promote { character, faction } => format!("promote {character} {faction}"),
        Command::Demote { character, faction } => format!("demote {character} {faction}"),
        Command::LeaveFaction { character, faction } => format!("leave {character} {faction}"),
        Command::ApplyOutcome { outcome, character } => format!("outcome {outcome} {character}"),
        Command::ApplyEffects {
            source, character, ..
        } => format!("effects from {source} on {character}"),
        Command::Watch { subject } => format!("watch {subject}"),
        Command::AddModifier {
            id,
            observer,
            subject,
            amount,
            expires_at,
        } => {
            let until = expires_at
                .map(|tick| format!(" --until {tick}"))
                .unwrap_or_default();
            format!("modify {observer} {subject} {id} {amount}{until}")
        }
        Command::RemoveModifier { subject, id } => format!("unmodify {subject} {id}"),
        Command::ResolveConflict { character, keep } => format!("resolve {character} {keep}"),
        Command::SetFactionAlignment { faction, alignment } => format!(
            "faction-align {faction} {} {}",
            alignment.law(),
            alignment.good()
        ),
        Command::ShiftFactionAlignment { faction, by } => {
            format!(
                "faction-shift {faction} --law {} --good {}",
                by.law, by.good
            )
        }
        Command::Unwatch { subject } => format!("unwatch {subject}"),
        Command::SetRelation {
            from,
            to,
            value,
            mutual,
        } => format!("relate {from} {to} {value}{}", one_way(*mutual)),
        Command::ShiftRelation {
            from,
            to,
            by,
            mutual,
        } => format!("relate {from} {to} --by {by}{}", one_way(*mutual)),
        Command::PerformAction {
            actor,
            action,
            target,
            scale,
            ..
        } => {
            let mut typed = format!("act {actor} {action}");
            if let Some(target) = target {
                typed += &format!(" --target {target}");
            }
            if *scale != Fixed::ONE {
                typed += &format!(" --scale {scale}");
            }
            typed
        }
    }
}

/// `act`'s arguments as a command. Everyone witnesses an act done from the CLI.
/// `act`'s arguments as a command, and whether `--explain` was given.
fn parse_act(args: &str) -> Result<(Command, bool), String> {
    const USAGE: &str =
        "act needs the form: act <actor> <action> [--target <id>] [--scale <n>] [--explain]";
    let words: Vec<&str> = args.split_whitespace().collect();
    let [actor, action, options @ ..] = &words[..] else {
        return Err(USAGE.to_owned());
    };
    let (mut target, mut scale, mut explain) = (None, None, false);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        let slot = match *option {
            "--explain" if !explain => {
                explain = true;
                continue;
            }
            "--target" => &mut target,
            "--scale" => &mut scale,
            _ => return Err(USAGE.to_owned()),
        };
        let Some(value) = options.next().filter(|value| !is_flag(value)) else {
            return Err(USAGE.to_owned());
        };
        if slot.replace(*value).is_some() {
            return Err(USAGE.to_owned());
        }
    }
    let character = |id: &str| CharacterId::new(id).map_err(|invalid| invalid.to_string());
    let command = Command::PerformAction {
        actor: character(actor)?,
        action: ActionId::new(action).map_err(|invalid| invalid.to_string())?,
        target: target.map(character).transpose()?,
        scale: match scale {
            Some(n) => n
                .parse()
                .map_err(|error: ParseFixedError| error.to_string())?,
            None => Fixed::ONE,
        },
        witnesses: Witnesses::Everyone,
    };
    Ok((command, explain))
}

/// `law -5.00, good -3.00`.
fn axes(alignment: Alignment) -> String {
    format!("law {}, good {}", alignment.law(), alignment.good())
}

/// A character and faction id from the command line.
fn member_ids(character: &str, faction: &str) -> Result<(CharacterId, FactionId), String> {
    Ok((
        CharacterId::new(character).map_err(|invalid| invalid.to_string())?,
        FactionId::new(faction).map_err(|invalid| invalid.to_string())?,
    ))
}

/// Named standing effects as `city_watch -20.00, captain_hale -10.00`: factions first.
fn named_effects(effects: &StandingEffects) -> Vec<String> {
    effects
        .parties()
        .into_iter()
        .map(|(party, value)| format!("{party} {value}"))
        .collect()
}

/// Whether a word is an option such as `--one-way`, rather than a value. Negative numbers
/// have one dash.
fn is_flag(word: &str) -> bool {
    word.starts_with("--")
}

fn one_way(mutual: bool) -> &'static str {
    if mutual { "" } else { " --one-way" }
}

/// `faction-align`'s or `faction-shift`'s arguments as a command.
fn parse_faction_alignment(args: &str, shifting: bool) -> Result<Command, String> {
    let number = |text: &str| -> Result<Fixed, String> {
        text.parse()
            .map_err(|error: ParseFixedError| error.to_string())
    };
    let words: Vec<&str> = args.split_whitespace().collect();
    if !shifting {
        let [faction, law, good] = words[..] else {
            return Err(
                "faction-align needs the form: faction-align <faction> <law> <good>".to_owned(),
            );
        };
        let faction = FactionId::new(faction).map_err(|invalid| invalid.to_string())?;
        let alignment = Alignment::new(number(law)?, number(good)?).map_err(|problems| {
            let problem = &problems[0];
            format!("{}: {problem}", problem.axis.key())
        })?;
        return Ok(Command::SetFactionAlignment { faction, alignment });
    }
    const USAGE: &str =
        "faction-shift needs the form: faction-shift <faction> [--law <n>] [--good <n>]";
    let [faction, options @ ..] = &words[..] else {
        return Err(USAGE.to_owned());
    };
    let (mut law, mut good) = (None, None);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        let slot = match *option {
            "--law" => &mut law,
            "--good" => &mut good,
            _ => return Err(USAGE.to_owned()),
        };
        let Some(value) = options.next().filter(|value| !is_flag(value)) else {
            return Err(USAGE.to_owned());
        };
        if slot.replace(number(value)?).is_some() {
            return Err(USAGE.to_owned());
        }
    }
    if law.is_none() && good.is_none() {
        return Err(USAGE.to_owned());
    }
    Ok(Command::ShiftFactionAlignment {
        faction: FactionId::new(faction).map_err(|invalid| invalid.to_string())?,
        by: AlignmentDelta {
            law: law.unwrap_or_default(),
            good: good.unwrap_or_default(),
        },
    })
}

/// `relate`'s arguments as a command.
fn parse_relate(args: &str) -> Result<Command, String> {
    const USAGE: &str = "relate needs the form: relate <from> <to> <value> [--one-way], or relate <from> <to> --by <n> [--one-way]";
    let words: Vec<&str> = args.split_whitespace().collect();
    let (from, to, rest) = match &words[..] {
        [from, to, rest @ ..] => (*from, *to, rest),
        _ => return Err(USAGE.to_owned()),
    };
    let (shifting, number, mutual) = match rest {
        ["--by", n] if !is_flag(n) => (true, *n, true),
        ["--by", n, "--one-way"] if !is_flag(n) => (true, *n, false),
        [value] if !is_flag(value) => (false, *value, true),
        [value, "--one-way"] if !is_flag(value) => (false, *value, false),
        _ => return Err(USAGE.to_owned()),
    };
    let faction = |id: &str| FactionId::new(id).map_err(|invalid| invalid.to_string());
    let (from, to) = (faction(from)?, faction(to)?);
    let number: Fixed = number
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    Ok(if shifting {
        Command::ShiftRelation {
            from,
            to,
            by: number,
            mutual,
        }
    } else {
        Command::SetRelation {
            from,
            to,
            value: number,
            mutual,
        }
    })
}

pub(crate) fn lines(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join("\n")
}

pub(crate) fn no_world() -> Outcome {
    Outcome::Error("no world is loaded yet: use load <dir> first".to_owned())
}

/// A drift policy as content writes it, with probation's settings in words.
fn describe_drift(policy: DriftPolicy) -> String {
    match policy {
        DriftPolicy::Probation { grace_ticks, then } => {
            format!("probation for {grace_ticks} ticks, then {}", then.key())
        }
        simple => simple.key().to_owned(),
    }
}

/// One line about a faction: `temple — Temple of the Dawn — law 30.00, good 80.00 — Neutral Good`.
fn describe_faction(world: &World, faction: &Faction) -> String {
    let members: Vec<String> = world
        .members(&faction.id)
        .expect("a faction from the world")
        .into_iter()
        .map(|member| {
            let (_, membership) = world
                .memberships(member)
                .expect("a member of the world")
                .find(|(member_of, _)| **member_of == faction.id)
                .expect("a member of this faction");
            format!("{member} ({})", membership.rank)
        })
        .collect();
    let members = if members.is_empty() {
        "no members".to_owned()
    } else {
        format!("members: {}", members.join(", "))
    };
    let policy = world
        .drift_policy(&faction.id)
        .expect("a faction from the world");
    let alignment = world
        .faction_alignment(&faction.id)
        .expect("a faction from the world");
    let by_default = if faction.drift.is_none() {
        ", by default"
    } else {
        ""
    };
    format!(
        "{} — {} — {} — {} — tolerance {}, member tolerance {}, drift {}{by_default} — {members}",
        faction.id,
        faction.name,
        axes(alignment),
        alignment.label(world.balance().label_threshold),
        faction.tolerances.tolerance(),
        faction.tolerances.member(),
        describe_drift(policy),
    )
}

/// The observer and subject of a `distance` or `disposition`, found in the world.
struct Judgement<'a> {
    observer: Observer,
    /// The observer's id as typed.
    observer_id: &'a str,
    subject: CharacterId,
    explain: bool,
}

/// The lines that explain a distance: each axis, then whose weights were used.
fn working(distance: &Distance, observer: &str) -> [String; 3] {
    let axis = |axis: Axis| {
        format!(
            "{}: {} vs {}, gap {}, weight {}",
            axis.key(),
            distance.observer.on(axis),
            distance.subject.on(axis),
            distance.gap(axis),
            distance.weights.on(axis)
        )
    };
    let weights = match distance.weights_from {
        WeightsFrom::Own => format!("{observer}'s own"),
        WeightsFrom::Default => "the default (alignment.default_weights)".to_owned(),
    };
    [
        axis(Axis::Law),
        axis(Axis::Good),
        format!("weights: {weights}"),
    ]
}

/// The world's bands in a line: `unfriendly ≤ -25.00 < neutral ≤ 25.00 < friendly`.
fn describe_bands(world: &World) -> String {
    let bands: Vec<String> = world
        .balance()
        .bands
        .iter()
        .map(|band| match band.up_to {
            Some(up_to) => format!("{} ≤ {up_to} <", band.name),
            None => band.name.clone(),
        })
        .collect();
    bands.join(" ")
}

/// ` (did you mean 'x'?)` when one of `ids` is close to `word`; otherwise nothing.
pub(crate) fn hint(word: &str, ids: Vec<&str>) -> String {
    suggest(word, ids)
        .map(|close| format!(" (did you mean '{close}'?)"))
        .unwrap_or_default()
}

/// One line about a character as they are now:
/// `vex — Vex — law -55.00, good -20.00 — Chaotic Neutral`.
fn describe(world: &World, character: &Character) -> String {
    let alignment = world
        .alignment(&character.id)
        .expect("every character in the world has an alignment");
    let memberships: Vec<String> = world
        .memberships(&character.id)
        .expect("a character from the world")
        .map(|(faction, membership)| {
            format!(
                "{faction} ({}) since tick {}",
                membership.rank, membership.since
            )
        })
        .collect();
    let mut line = format!(
        "{} — {} — {} — {}",
        character.id,
        character.name,
        axes(alignment),
        alignment.label(world.balance().label_threshold)
    );
    if !memberships.is_empty() {
        line += &format!(" — member of {}", memberships.join(", "));
    }
    let wars: Vec<String> = world
        .conflicts(&character.id)
        .expect("a character from the world")
        .into_iter()
        .map(|(a, b)| format!("{a} and {b}"))
        .collect();
    if !wars.is_empty() {
        line += &format!(" — at war: {}", wars.join("; "));
    }
    line
}

/// `calc <a> <op> <b>`: the engine's fixed-point arithmetic, so a designer can check how a
/// number rounds (DESIGN.md §4.1).
fn calc(args: &str) -> Outcome {
    match calculate(args) {
        Ok(value) => Outcome::Output(value.to_string()),
        Err(message) => Outcome::Error(message),
    }
}

fn calculate(args: &str) -> Result<Fixed, String> {
    const USAGE: &str = "calc needs the form: calc <a> <op> <b>, where <op> is + - * or /";
    let [a, op, b] = args.split_whitespace().collect::<Vec<_>>()[..] else {
        return Err(USAGE.to_owned());
    };
    let a: Fixed = a
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    let b: Fixed = b
        .parse()
        .map_err(|error: ParseFixedError| error.to_string())?;
    let result = match op {
        "+" => a.checked_add(b),
        "-" => a.checked_sub(b),
        "*" => a.checked_mul(b),
        "/" if b == Fixed::ZERO => return Err(format!("cannot divide by {b}")),
        "/" => a.checked_div(b),
        _ => return Err(USAGE.to_owned()),
    };
    result.ok_or_else(|| "the result is out of range".to_owned())
}

/// Whether a (trimmed) line has nothing to run: it's blank, or a `#` comment.
pub(crate) fn is_blank_or_comment(line: &str) -> bool {
    line.is_empty() || line.starts_with('#')
}

/// Splits a line into its command name and the rest of the line.
pub(crate) fn split_command(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (line, ""),
    }
}

/// Splits `<command> == <expected>` at the first ` == `. A trailing ` ==` expects an empty result.
pub(crate) fn split_assert(spec: &str) -> Option<(&str, &str)> {
    spec.split_once(" == ")
        .or_else(|| spec.strip_suffix(" ==").map(|command| (command, "")))
}

/// The list of commands that `help` prints, with their descriptions aligned.
pub fn help_text() -> String {
    let width = COMMANDS
        .iter()
        .map(|(usage, _)| usage.len())
        .max()
        .unwrap_or(0);
    COMMANDS
        .iter()
        .map(|(usage, description)| format!("{usage:<width$}  {description}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use factional_core::Tick;
    use factional_reputation::{ActionId, CharacterId, Effects, Witnesses};

    fn run(line: &str) -> Result<Outcome, ScriptError> {
        Session::default().execute(line)
    }

    fn passes() -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(String::new()))
    }

    #[test]
    fn echo_prints_the_rest_of_the_line_verbatim() {
        assert_eq!(
            run("echo hello  there"),
            Ok(Outcome::Output("hello  there".into()))
        );
    }

    #[test]
    fn echo_on_its_own_prints_nothing() {
        assert_eq!(run("echo"), Ok(Outcome::Output(String::new())));
    }

    #[test]
    fn fail_reports_its_message_as_a_command_error() {
        assert_eq!(run("fail boom"), Ok(Outcome::Error("boom".into())));
    }

    #[test]
    fn quit_ends_the_session() {
        assert_eq!(run("quit"), Ok(Outcome::Quit));
    }

    #[test]
    fn help_prints_the_command_list() {
        assert_eq!(run("help"), Ok(Outcome::Output(help_text())));
    }

    fn output(text: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(text.into()))
    }

    fn command_error(message: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Error(message.into()))
    }

    #[test]
    fn calc_does_the_engines_fixed_point_arithmetic() {
        assert_eq!(run("calc 4.00 * 0.41"), output("1.64"));
        assert_eq!(run("calc 0.05 * 0.50"), output("0.03"));
        assert_eq!(run("calc 2 / 3"), output("0.67"));
        assert_eq!(run("calc 12.5 + -0.05"), output("12.45"));
        assert_eq!(run("calc 1 - 3.25"), output("-2.25"));
    }

    #[test]
    fn calc_reports_bad_numbers_and_impossible_results_as_command_errors() {
        assert_eq!(run("calc 1 / 0"), command_error("cannot divide by 0.00"));
        assert_eq!(
            run("calc 12.345 + 1"),
            command_error("12.345 has more than 2 decimal places")
        );
        assert_eq!(run("calc 1 + abc"), command_error("'abc' is not a number"));
        assert_eq!(
            run("calc 92233720368547758.07 + 1"),
            command_error("the result is out of range")
        );
        assert_eq!(
            run("calc -92233720368547758.08 - 1"),
            command_error("the result is out of range")
        );
        assert_eq!(
            run("calc 92233720368547758.07 * 2"),
            command_error("the result is out of range")
        );
    }

    /// The repository's root, so tests can load `content/sample`.
    fn repo() -> Session {
        Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    fn riverhold() -> Session {
        let mut session = repo();
        assert_eq!(
            session.execute("load content/sample"),
            output("loaded 6 characters from content/sample")
        );
        session
    }

    #[test]
    fn show_character_gives_the_alignment_and_its_label() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("show character vex"),
            output(
                "vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of lantern_guild (fence) since tick 0"
            )
        );
        assert_eq!(
            session.execute("show character merchant_ava"),
            output("merchant_ava — Merchant Ava — law 20.00, good 10.00 — True Neutral")
        );
    }

    #[test]
    fn characters_lists_everyone_in_id_order() {
        assert_eq!(
            riverhold().execute("characters"),
            output(
                "brother_ash — Brother Ash — law 25.00, good -70.00 — Neutral Evil — member of ashen_circle (initiate) since tick 0\n\
                 captain_hale — Captain Hale — law 75.00, good 30.00 — Lawful Neutral — member of city_watch (captain) since tick 0\n\
                 merchant_ava — Merchant Ava — law 20.00, good 10.00 — True Neutral\n\
                 player — The Player — law 0.00, good 0.00 — True Neutral\n\
                 sister_mira — Sister Mira — law 35.00, good 85.00 — Lawful Good — member of temple (ordained) since tick 0\n\
                 vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of lantern_guild (fence) since tick 0"
            )
        );
    }

    #[test]
    fn labels_use_the_loaded_threshold() {
        let mut session = repo();
        assert_eq!(
            session.execute("load crates/cli/tests/fixtures/worlds/stern"),
            output("loaded 1 character from crates/cli/tests/fixtures/worlds/stern")
        );
        assert_eq!(
            session.execute("show character vex"),
            output("vex — Vex — law -55.00, good -20.00 — True Neutral")
        );
    }

    #[test]
    fn load_reports_every_problem_and_keeps_the_world_already_loaded() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("load crates/cli/tests/fixtures/worlds/broken"),
            command_error(
                "characters.toml: hale: missing 'alignment'\n\
                 characters.toml: hale: unknown key 'alignmnet' (did you mean 'alignment'?)\n\
                 characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00"
            )
        );
        assert_eq!(
            session.execute("show character vex"),
            output(
                "vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of lantern_guild (fence) since tick 0"
            )
        );
    }

    #[test]
    fn load_reports_a_directory_it_cannot_read() {
        let Ok(Outcome::Error(message)) = repo().execute("load no/such/world") else {
            panic!("expected a command error");
        };
        assert!(
            message.contains("no/such/world: cannot read the directory: "),
            "{message}"
        );
    }

    #[test]
    fn validate_reports_on_a_world_without_loading_it() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("validate crates/cli/tests/fixtures/worlds/broken"),
            output(
                "error: characters.toml: hale: missing 'alignment'\n\
                 error: characters.toml: hale: unknown key 'alignmnet' (did you mean 'alignment'?)\n\
                 error: characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00\n\
                 crates/cli/tests/fixtures/worlds/broken doesn't load: 3 problems"
            )
        );
        assert_eq!(
            session.execute("validate crates/cli/tests/fixtures/worlds/stern"),
            output(
                "crates/cli/tests/fixtures/worlds/stern loads: 1 character, 0 factions, 0 actions, 0 relations and 0 outcomes"
            )
        );
        assert_eq!(
            session.execute("show character vex"),
            output(
                "vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of lantern_guild (fence) since tick 0"
            ),
            "the world already loaded stays"
        );
        assert_eq!(
            run("validate"),
            command_error("validate needs the form: validate <dir>")
        );
    }

    #[test]
    fn load_needs_a_directory() {
        assert_eq!(
            run("load"),
            command_error("load needs the form: load <dir>")
        );
    }

    #[test]
    fn show_suggests_a_close_id_for_an_unknown_character() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("show character vx"),
            command_error("unknown character 'vx' (did you mean 'vex'?)")
        );
        assert_eq!(
            session.execute("show character nobody"),
            command_error("unknown character 'nobody'")
        );
    }

    #[test]
    fn show_needs_a_kind_and_an_id() {
        let mut session = riverhold();
        let usage = command_error("show needs the form: show character <id>, or show faction <id>");
        for line in [
            "show",
            "show character",
            "show faction",
            "show rank vex",
            "show character vex hale",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
    }

    #[test]
    fn world_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        assert_eq!(run("characters"), none);
        assert_eq!(run("show character vex"), none);
    }

    #[test]
    fn advance_moves_time_and_shows_the_events() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("advance 5"),
            output("#1 at tick 0: time advanced from 0 to 5")
        );
        assert_eq!(session.execute("time"), output("tick 5"));
        assert_eq!(
            session.execute("advance 3"),
            output("#2 at tick 5: time advanced from 5 to 8")
        );
    }

    #[test]
    fn advance_reports_a_refusal_and_bad_input() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("advance 0"),
            command_error("ticks must be at least 1")
        );
        for bad in ["x", "-1", "1.5"] {
            assert_eq!(
                session.execute(&format!("advance {bad}")),
                command_error(&format!("'{bad}' is not a whole number of ticks"))
            );
        }
        let usage = command_error("advance needs the form: advance <ticks>");
        assert_eq!(session.execute("advance"), usage);
        assert_eq!(session.execute("advance 1 2"), usage);
        assert_eq!(session.execute("time"), output("tick 0"));
    }

    #[test]
    fn events_lists_what_happened_with_an_optional_starting_point() {
        let mut session = riverhold();
        assert_eq!(session.execute("events"), output("no events yet"));
        for ticks in [1, 2, 3] {
            session.execute(&format!("advance {ticks}")).expect("valid");
        }
        assert_eq!(
            session.execute("events"),
            output(
                "#1 at tick 0: time advanced from 0 to 1\n\
                 #2 at tick 1: time advanced from 1 to 3\n\
                 #3 at tick 3: time advanced from 3 to 6"
            )
        );
        assert_eq!(
            session.execute("events --since 2"),
            output("#3 at tick 3: time advanced from 3 to 6")
        );
        assert_eq!(
            session.execute("events --since 3"),
            output("no events after #3")
        );
        assert_eq!(
            session.execute("events --since x"),
            command_error("'x' is not an event number")
        );
        let usage = command_error("events needs the form: events [--since <seq>]");
        assert_eq!(session.execute("events 3"), usage);
        assert_eq!(session.execute("events --since"), usage);
    }

    #[test]
    fn journal_lists_every_command_and_whether_it_was_accepted() {
        let mut session = riverhold();
        assert_eq!(session.execute("journal"), output("no commands yet"));
        for line in ["advance 5", "advance 0", "advance 2"] {
            session.execute(line).expect("valid");
        }
        assert_eq!(
            session.execute("journal"),
            output(
                "1. advance 5 — accepted\n\
                 2. advance 0 — refused: ticks must be at least 1\n\
                 3. advance 2 — accepted"
            )
        );
    }

    #[test]
    fn loading_starts_a_new_world() {
        let mut session = riverhold();
        session.execute("advance 5").expect("valid");
        session.execute("load content/sample").expect("valid");
        assert_eq!(session.execute("time"), output("tick 0"));
        assert_eq!(session.execute("journal"), output("no commands yet"));
    }

    #[test]
    fn time_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in ["advance 1", "time", "events", "journal"] {
            assert_eq!(run(line), none, "{line}");
        }
    }

    // Actions

    #[test]
    fn actions_lists_the_catalogue_with_each_acts_alignment_effect() {
        assert_eq!(
            riverhold().execute("actions"),
            output(
                "donate_to_temple — law 0.00, good 3.00 — standing: temple 10.00\n\
                 extort — law -2.00, good -6.00 — standing: target -30.00, target_factions -15.00\n\
                 help_stranger — law 0.00, good 4.00 — standing: target 10.00\n\
                 murder — law -10.00, good -15.00 — standing: target -100.00, target_factions -40.00\n\
                 report_crime — law 4.00, good 1.00 — standing: city_watch 5.00\n\
                 steal — law -5.00, good -3.00 — standing: target -20.00, target_factions -10.00"
            )
        );
    }

    #[test]
    fn act_moves_the_actors_alignment_and_shows_the_events() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("act player steal --target merchant_ava"),
            output(
                "#1 at tick 0: player did steal, targeting merchant_ava\n\
                 #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -5.00, good -3.00\n\
                 #3 at tick 0: player's standing with merchant_ava moved from 0.00 to -20.00"
            )
        );
        assert_eq!(
            session.execute("show character player"),
            output("player — The Player — law -5.00, good -3.00 — True Neutral")
        );
    }

    #[test]
    fn act_takes_a_scale_and_needs_no_target() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("act player steal --scale 2"),
            output(
                "#1 at tick 0: player did steal, at scale 2.00\n\
                 #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -10.00, good -6.00"
            )
        );
        assert_eq!(
            session.execute("act player steal --scale 0.5 --target vex"),
            output(
                "#3 at tick 0: player did steal, targeting vex, at scale 0.50\n\
                 #4 at tick 0: player's alignment moved from law -10.00, good -6.00 to law -12.50, good -7.50\n\
                 #5 at tick 0: player's standing with city_watch moved from 0.00 to 1.80, with 1.80 spilled from lantern_guild (-10.00 × -0.18; city_watch regards it at -80.00)\n\
                 #6 at tick 0: player's standing with lantern_guild moved from 0.00 to -10.00\n\
                 #7 at tick 0: player's standing with vex moved from 0.00 to -20.00"
            )
        );
    }

    #[test]
    fn act_reports_the_worlds_refusals() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "act player stael",
                "unknown action 'stael' (did you mean 'steal'?)",
            ),
            (
                "act plyer steal",
                "unknown actor 'plyer' (did you mean 'player'?)",
            ),
            (
                "act player steal --target nobody",
                "unknown target 'nobody'",
            ),
            (
                "act player steal --target player",
                "an action's target must be another character",
            ),
            (
                "act player steal --scale 0",
                "scale must be greater than 0.00",
            ),
            (
                "act player steal --scale -1",
                "scale must be greater than 0.00",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        assert_eq!(
            session.execute("show character player"),
            output("player — The Player — law 0.00, good 0.00 — True Neutral")
        );
        assert_eq!(session.execute("events"), output("no events yet"));
    }

    #[test]
    fn act_reports_bad_input() {
        let mut session = riverhold();
        let usage = command_error(
            "act needs the form: act <actor> <action> [--target <id>] [--scale <n>] [--explain]",
        );
        for line in [
            "act",
            "act player",
            "act player steal vex",
            "act player steal --target",
            "act player steal --scale",
            "act player steal --target vex --target ava",
            "act player steal --scale 1 --scale 2",
            "act player steal --witnesses nobody",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
        assert_eq!(
            session.execute("act player steal --scale 1.234"),
            command_error("1.234 has more than 2 decimal places")
        );
        assert_eq!(
            session.execute("act Player steal"),
            command_error(
                "'Player' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            )
        );
        assert_eq!(
            session.execute("act player Steal"),
            command_error(
                "'Steal' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            )
        );
        assert_eq!(
            session.execute("act player steal --target Vex"),
            command_error(
                "'Vex' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            )
        );
    }

    #[test]
    fn the_journal_shows_acts_as_they_were_typed() {
        let mut session = riverhold();
        for line in [
            "act player steal --target merchant_ava",
            "act player steal --scale 2.5",
            "act player stael",
        ] {
            session.execute(line).expect("valid");
        }
        assert_eq!(
            session.execute("journal"),
            output(
                "1. act player steal --target merchant_ava — accepted\n\
                 2. act player steal --scale 2.50 — accepted\n\
                 3. act player stael — refused: unknown action 'stael' (did you mean 'steal'?)"
            )
        );
    }

    #[test]
    fn describes_who_witnessed_an_act_unless_everyone_did() {
        let id = |text| CharacterId::new(text).expect("valid id");
        let event = |witnesses| Event {
            seq: 4,
            tick: Tick(2),
            payload: Change::ActionPerformed {
                actor: id("vex"),
                action: ActionId::new("steal").expect("valid id"),
                target: None,
                scale: Fixed::ONE,
                witnesses,
            },
        };
        assert_eq!(
            describe_event(&event(Witnesses::Nobody)),
            "#4 at tick 2: vex did steal, witnessed by nobody"
        );
        assert_eq!(
            describe_event(&event(Witnesses::These(
                [id("player"), id("captain_hale")].into()
            ))),
            "#4 at tick 2: vex did steal, witnessed by captain_hale, player"
        );
    }

    #[test]
    fn action_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        assert_eq!(run("actions"), none);
        assert_eq!(run("act player steal"), none);
    }

    // Factions and distance

    #[test]
    fn factions_lists_every_faction_in_id_order() {
        assert_eq!(
            riverhold().execute("factions"),
            output(
                "ashen_circle — The Ashen Circle — law 20.00, good -80.00 — Neutral Evil — tolerance 30.00, member tolerance 40.00, drift expel — members: brother_ash (initiate)\n\
                 city_watch — The City Watch — law 70.00, good 20.00 — Lawful Neutral — tolerance 40.00, member tolerance 50.00, drift probation for 100 ticks, then expel — members: captain_hale (captain)\n\
                 free_company — The Free Company — law -10.00, good 0.00 — True Neutral — tolerance 60.00, member tolerance 80.00, drift ignore — no members\n\
                 lantern_guild — The Lantern Guild — law -60.00, good -10.00 — Chaotic Neutral — tolerance 45.00, member tolerance 60.00, drift flag — members: vex (fence)\n\
                 temple — Temple of the Dawn — law 30.00, good 80.00 — Neutral Good — tolerance 35.00, member tolerance 45.00, drift demote — members: sister_mira (ordained)"
            )
        );
    }

    #[test]
    fn show_faction_gives_its_alignment_and_label() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("show faction temple"),
            output(
                "temple — Temple of the Dawn — law 30.00, good 80.00 — Neutral Good — tolerance 35.00, member tolerance 45.00, drift demote — members: sister_mira (ordained)"
            )
        );
        assert_eq!(
            session.execute("show faction city_wach"),
            command_error("unknown faction 'city_wach' (did you mean 'city_watch'?)")
        );
    }

    #[test]
    fn distance_is_measured_with_the_observers_weights() {
        let mut session = riverhold();
        for (line, expected) in [
            ("distance city_watch player", "70.18"),
            ("distance temple sister_mira", "5.59"),
            ("distance captain_hale player", "75.37"),
            ("distance merchant_ava player", "22.36"),
        ] {
            assert_eq!(session.execute(line), output(expected), "{line}");
        }
    }

    #[test]
    fn distance_follows_the_subject_as_they_move() {
        let mut session = riverhold();
        session
            .execute("act player steal --scale 2")
            .expect("valid");
        assert_eq!(
            session.execute("distance lantern_guild player"),
            output("50.04")
        );
        session
            .execute("act player steal --scale 2")
            .expect("valid");
        assert_eq!(
            session.execute("distance lantern_guild player"),
            output("40.01")
        );
    }

    #[test]
    fn distance_explains_its_working() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("distance city_watch player --explain"),
            output(
                "city_watch → player: 70.18 (euclidean)\n\
                 law: 70.00 vs 0.00, gap 70.00, weight 1.00\n\
                 good: 20.00 vs 0.00, gap 20.00, weight 0.25\n\
                 weights: city_watch's own"
            )
        );
        assert_eq!(
            session.execute("distance merchant_ava vex --explain"),
            output(
                "merchant_ava → vex: 80.78 (euclidean)\n\
                 law: 20.00 vs -55.00, gap 75.00, weight 1.00\n\
                 good: 10.00 vs -20.00, gap 30.00, weight 1.00\n\
                 weights: the default (alignment.default_weights)"
            )
        );
    }

    #[test]
    fn distance_reports_unknown_or_wrong_ids_and_bad_input() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "distance city_wach player",
                "unknown observer 'city_wach' (did you mean 'city_watch'?)",
            ),
            (
                "distance captian_hale player",
                "unknown observer 'captian_hale' (did you mean 'captain_hale'?)",
            ),
            (
                "distance city_watch plyer",
                "unknown subject 'plyer' (did you mean 'player'?)",
            ),
            ("distance city_watch nobody", "unknown subject 'nobody'"),
            (
                "distance player city_watch",
                "a distance's subject must be a character",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        let usage =
            command_error("distance needs the form: distance <observer> <subject> [--explain]");
        for line in [
            "distance",
            "distance city_watch",
            "distance city_watch player vex",
            "distance city_watch player --explian",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
    }

    #[test]
    fn faction_commands_need_a_loaded_world() {
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in [
            "factions",
            "show faction temple",
            "distance city_watch player",
        ] {
            assert_eq!(run(line), none, "{line}");
        }
    }

    // Joining and leaving

    #[test]
    fn can_join_says_whether_and_why_not() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("can-join player lantern_guild"),
            output("no: 60.21 from The Lantern Guild, tolerance is 45.00")
        );
        session
            .execute("act player steal --scale 4")
            .expect("valid");
        assert_eq!(
            session.execute("can-join player lantern_guild"),
            output("yes: 40.01 from The Lantern Guild, tolerance is 45.00")
        );
        assert_eq!(
            session.execute("can-join vex lantern_guild"),
            output("no: vex is already a member of lantern_guild")
        );
    }

    #[test]
    fn can_join_explains_its_working() {
        assert_eq!(
            riverhold().execute("can-join player lantern_guild --explain"),
            output(
                "player → lantern_guild: no\n\
                 distance 60.21 (euclidean), tolerance 45.00\n\
                 law: -60.00 vs 0.00, gap 60.00, weight 1.00\n\
                 good: -10.00 vs 0.00, gap 10.00, weight 0.50\n\
                 weights: lantern_guild's own\n\
                 refused: 60.21 from The Lantern Guild, tolerance is 45.00"
            )
        );
    }

    #[test]
    fn joining_and_leaving_change_membership_and_show_the_events() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("join player lantern_guild"),
            command_error(
                "player can't join lantern_guild: 60.21 from The Lantern Guild, tolerance is 45.00"
            )
        );
        session
            .execute("act player steal --scale 4")
            .expect("valid");
        session.execute("advance 2").expect("valid");
        assert_eq!(
            session.execute("join player lantern_guild"),
            output("#4 at tick 2: player joined lantern_guild as a cutpurse")
        );
        assert_eq!(
            session.execute("show character player"),
            output(
                "player — The Player — law -20.00, good -12.00 — True Neutral — member of lantern_guild (cutpurse) since tick 2"
            )
        );
        assert_eq!(
            session.execute("show faction lantern_guild"),
            output(
                "lantern_guild — The Lantern Guild — law -60.00, good -10.00 — Chaotic Neutral — tolerance 45.00, member tolerance 60.00, drift flag — members: player (cutpurse), vex (fence)"
            )
        );
        assert_eq!(
            session.execute("leave player lantern_guild"),
            output("#5 at tick 2: player left lantern_guild (voluntary)")
        );
        assert_eq!(
            session.execute("leave player lantern_guild"),
            command_error("player isn't a member of lantern_guild")
        );
        assert_eq!(
            session.execute("journal"),
            output(
                "1. join player lantern_guild — refused: player can't join lantern_guild: 60.21 from The Lantern Guild, tolerance is 45.00\n\
                 2. act player steal --scale 4.00 — accepted\n\
                 3. advance 2 — accepted\n\
                 4. join player lantern_guild — accepted\n\
                 5. leave player lantern_guild — accepted\n\
                 6. leave player lantern_guild — refused: player isn't a member of lantern_guild"
            )
        );
    }

    #[test]
    fn a_character_in_several_factions_lists_them_all() {
        let mut session = riverhold();
        session.execute("advance 1").expect("valid");
        // Vex is 12.31 from the Free Company, well within its 60.00.
        session.execute("join vex free_company").expect("valid");
        assert_eq!(
            session.execute("show character vex"),
            output(
                "vex — Vex — law -55.00, good -20.00 — Chaotic Neutral — member of free_company (sellsword) since tick 1, lantern_guild (fence) since tick 0"
            )
        );
    }

    #[test]
    fn membership_commands_report_unknown_ids_and_bad_input() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "join plyer temple",
                "unknown character 'plyer' (did you mean 'player'?)",
            ),
            (
                "join player tempel",
                "unknown faction 'tempel' (did you mean 'temple'?)",
            ),
            (
                "leave player tempel",
                "unknown faction 'tempel' (did you mean 'temple'?)",
            ),
            (
                "can-join plyer temple",
                "unknown character 'plyer' (did you mean 'player'?)",
            ),
            (
                "can-join player tempel",
                "unknown faction 'tempel' (did you mean 'temple'?)",
            ),
            (
                "join Player temple",
                "'Player' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            ),
            (
                "leave player Temple",
                "'Temple' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        for (line, usage) in [
            (
                "join player",
                "join needs the form: join <character> <faction>",
            ),
            (
                "leave a b c",
                "leave needs the form: leave <character> <faction>",
            ),
            (
                "can-join player",
                "can-join needs the form: can-join <character> <faction> [--explain]",
            ),
            (
                "can-join player temple --why",
                "can-join needs the form: can-join <character> <faction> [--explain]",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(usage), "{line}");
        }
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in [
            "join player temple",
            "leave player temple",
            "can-join player temple",
        ] {
            assert_eq!(run(line), none, "{line}");
        }
    }

    #[test]
    fn load_prints_warnings_after_its_summary() {
        let mut session = repo();
        assert_eq!(
            session.execute("load crates/cli/tests/fixtures/worlds/reformed"),
            output(
                "loaded 1 character from crates/cli/tests/fixtures/worlds/reformed\n\
                 warning: characters.toml: vex.memberships[0]: vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00\n\
                 warning: factions.toml: lantern_guild.tolerance: no one starts within The Lantern Guild's tolerance of 45.00: the nearest is vex, 95.52 away"
            )
        );
        assert_eq!(
            session.execute("show character vex"),
            output(
                "vex — Vex — law 35.00, good 10.00 — Lawful Neutral — member of lantern_guild (cutpurse) since tick 0"
            )
        );
    }

    // Ranks

    #[test]
    fn ranks_lists_a_ladder_with_its_requirements() {
        assert_eq!(
            riverhold().execute("ranks city_watch"),
            output(
                "1. recruit\n\
                 2. sergeant — needs standing 30.00\n\
                 3. captain — needs standing 70.00, and to be within 25.00"
            )
        );
        assert_eq!(
            riverhold().execute("ranks city_wach"),
            command_error("unknown faction 'city_wach' (did you mean 'city_watch'?)")
        );
    }

    #[test]
    fn promote_moves_a_member_up_when_the_next_rank_allows() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("promote vex lantern_guild"),
            command_error(
                "vex can't be promoted in lantern_guild: shadow needs standing 60.00, vex has 30.00"
            )
        );
        session
            .execute("outcome fenced_the_crown_jewels vex")
            .expect("valid");
        assert_eq!(
            session.execute("promote vex lantern_guild"),
            output("#4 at tick 0: vex's rank in lantern_guild changed from fence to shadow")
        );
        assert_eq!(
            session.execute("promote vex lantern_guild"),
            command_error("vex is already a shadow, the highest rank of lantern_guild")
        );
        assert_eq!(
            session.execute("demote vex lantern_guild"),
            output("#5 at tick 0: vex's rank in lantern_guild changed from shadow to fence")
        );
        assert_eq!(
            session.execute("journal"),
            output(
                "1. promote vex lantern_guild — refused: vex can't be promoted in lantern_guild: shadow needs standing 60.00, vex has 30.00\n\
                 2. outcome fenced_the_crown_jewels vex — accepted\n\
                 3. promote vex lantern_guild — accepted\n\
                 4. promote vex lantern_guild — refused: vex is already a shadow, the highest rank of lantern_guild\n\
                 5. demote vex lantern_guild — accepted"
            )
        );
    }

    #[test]
    fn promote_explains_each_requirement() {
        assert_eq!(
            riverhold().execute("promote sister_mira temple --explain"),
            output(
                "sister_mira: ordained → high_priest in temple: no\n\
                 standing: needs 80.00, has 40.00 — not met\n\
                 tolerance: within 20.00, is 5.59 — met"
            )
        );
        assert_eq!(
            riverhold().execute("promote captain_hale city_watch --explain"),
            output("captain_hale: captain in city_watch is the highest rank")
        );
    }

    #[test]
    fn rank_commands_report_mistakes() {
        let mut session = riverhold();
        for (line, message) in [
            ("promote player temple", "player isn't a member of temple"),
            ("demote player temple", "player isn't a member of temple"),
            (
                "promote plyer temple",
                "unknown character 'plyer' (did you mean 'player'?)",
            ),
            (
                "demote vex tempel",
                "unknown faction 'tempel' (did you mean 'temple'?)",
            ),
            (
                "promote player temple --explain",
                "player isn't a member of temple",
            ),
            (
                "demote brother_ash ashen_circle",
                "brother_ash is already an initiate, the lowest rank of ashen_circle",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        for (line, usage) in [
            ("ranks", "ranks needs the form: ranks <faction>"),
            (
                "promote vex",
                "promote needs the form: promote <character> <faction> [--explain]",
            ),
            (
                "promote vex lantern_guild --why",
                "promote needs the form: promote <character> <faction> [--explain]",
            ),
            (
                "demote vex",
                "demote needs the form: demote <character> <faction>",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(usage), "{line}");
        }
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in ["ranks temple", "promote vex temple", "demote vex temple"] {
            assert_eq!(run(line), none, "{line}");
        }
    }

    // Standing and outcomes

    /// The lines of `act … --explain` after the events: the shift's working.
    fn working_of(session: &mut Session, line: &str) -> Vec<String> {
        let Ok(Outcome::Output(output)) = session.execute(line) else {
            panic!("{line} should succeed");
        };
        output
            .lines()
            .skip_while(|line| line.starts_with('#'))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn act_explains_how_inertia_scaled_each_axis() {
        let mut session = riverhold();
        // Mira hardens: toward_good at 85 is 0.405, and 4.00 × 0.405 is 1.62, rounded once.
        assert_eq!(
            working_of(
                &mut session,
                "act sister_mira help_stranger --target merchant_ava --explain"
            ),
            [
                "shift = base × scale × inertia, rounded once; sister_mira's inertia profile is hardening",
                "good: 4.00 × 1.00 × 0.405 (good.toward_good at 85.00) = 1.62, from 85.00 to 86.62",
            ]
        );
        // Hale's profile has no law curves; toward_evil at 30 is 0.85.
        assert_eq!(
            working_of(&mut session, "act captain_hale steal --explain"),
            [
                "shift = base × scale × inertia, rounded once; captain_hale's inertia profile is hardening",
                "law: -5.00 × 1.00 × 1.00 (hardening has no law.toward_chaotic curve) = -5.00, from 75.00 to 70.00",
                "good: -3.00 × 1.00 × 0.85 (good.toward_evil at 30.00) = -2.55, from 30.00 to 27.45",
            ]
        );
        assert_eq!(
            working_of(&mut session, "act player steal --scale 2 --explain"),
            [
                "shift = base × scale × inertia, rounded once; player's inertia profile is steady",
                "law: -5.00 × 2.00 × 1.00 (steady has no law.toward_chaotic curve) = -10.00, from 0.00 to -10.00",
                "good: -3.00 × 2.00 × 1.00 (steady has no good.toward_evil curve) = -6.00, from 0.00 to -6.00",
            ]
        );
    }

    #[test]
    fn act_explains_how_the_target_scaled_each_axis() {
        // Hale (hardening, the Watch) murders Vex (the Guild, good −20): DESIGN.md §5.4.
        assert_eq!(
            working_of(
                &mut riverhold(),
                "act captain_hale murder --target vex --explain"
            ),
            [
                "shift = base × scale × target × inertia, rounded once; captain_hale's inertia profile is hardening",
                "law: -10.00 × 1.00 × 0.62 (by_target.relation at city_watch → lantern_guild -80.00) × 1.00 (hardening has no law.toward_chaotic curve) = -6.20, from 75.00 to 68.80",
                "good: -15.00 × 1.00 × 0.84 (by_target.good at vex's -20.00) × 0.62 (by_target.relation at city_watch → lantern_guild -80.00) × 0.85 (good.toward_evil at 30.00) = -6.64, from 30.00 to 23.36",
            ]
        );
        // The player is in no faction, so the relation curve is read at 0.
        assert_eq!(
            working_of(
                &mut riverhold(),
                "act player murder --target brother_ash --explain"
            ),
            [
                "shift = base × scale × target × inertia, rounded once; player's inertia profile is steady",
                "law: -10.00 × 1.00 × 1.00 (by_target.relation at 0.00: no relation between their factions) × 1.00 (steady has no law.toward_chaotic curve) = -10.00, from 0.00 to -10.00",
                "good: -15.00 × 1.00 × 0.44 (by_target.good at brother_ash's -70.00) × 1.00 (by_target.relation at 0.00: no relation between their factions) × 1.00 (steady has no good.toward_evil curve) = -6.60, from 0.00 to -6.60",
            ]
        );
    }

    #[test]
    fn shifting_a_faction_reviews_its_members() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("faction-shift city_watch --law -30"),
            output(
                "#1 at tick 0: city_watch's alignment moved from law 70.00, good 20.00 to law 40.00, good 20.00\n\
                 #2 at tick 0: captain_hale is on probation with city_watch until tick 100"
            )
        );
        assert_eq!(
            session.execute("faction-shift city_watch --good 5 --law 20"),
            output(
                "#3 at tick 0: city_watch's alignment moved from law 40.00, good 20.00 to law 60.00, good 25.00\n\
                 #4 at tick 0: captain_hale's probation with city_watch is cleared"
            )
        );
        assert_eq!(
            session.execute("faction-align temple 10 90"),
            output(
                "#5 at tick 0: temple's alignment moved from law 30.00, good 80.00 to law 10.00, good 90.00"
            )
        );
        assert_eq!(
            session.execute("show faction temple"),
            output(
                "temple — Temple of the Dawn — law 10.00, good 90.00 — Neutral Good — tolerance 35.00, member tolerance 45.00, drift demote — members: sister_mira (ordained)"
            )
        );
        assert_eq!(
            session.execute("journal"),
            output(
                "1. faction-shift city_watch --law -30.00 --good 0.00 — accepted\n\
                 2. faction-shift city_watch --law 20.00 --good 5.00 — accepted\n\
                 3. faction-align temple 10.00 90.00 — accepted"
            )
        );
    }

    #[test]
    fn faction_alignment_commands_report_mistakes() {
        let mut session = riverhold();
        let shift = command_error(
            "faction-shift needs the form: faction-shift <faction> [--law <n>] [--good <n>]",
        );
        for line in [
            "faction-shift city_watch",
            "faction-shift city_watch --law",
            "faction-shift city_watch --law 5 --law 5",
            "faction-shift city_watch --charm 5",
        ] {
            assert_eq!(session.execute(line), shift, "{line}");
        }
        assert_eq!(
            session.execute("faction-align temple 10"),
            command_error("faction-align needs the form: faction-align <faction> <law> <good>")
        );
        assert_eq!(
            session.execute("faction-align temple 120 0"),
            command_error("law: 120.00 is outside -100.00..100.00")
        );
        assert_eq!(
            session.execute("faction-shift tempel --law 5"),
            command_error("unknown faction 'tempel' (did you mean 'temple'?)")
        );
        assert_eq!(
            session.execute("faction-align temple ten 0"),
            command_error("'ten' is not a number")
        );
    }

    #[test]
    fn modifiers_change_dispositions_and_explain_where_they_came_from() {
        let mut session = riverhold();
        for line in [
            "act player steal --target merchant_ava",
            "act player steal --target merchant_ava",
            "outcome fined_by_watch player",
        ] {
            session.execute(line).expect("valid");
        }
        // The thefts made 6 events, and the fine 5 (two of them spills).
        assert_eq!(
            session.execute("modify captain_hale player bribed 20"),
            output("#12 at tick 0: modifier bribed on player: 20.00 from captain_hale")
        );
        assert_eq!(
            session.execute("modify everyone player hero_of_riverhold 10 --until 50"),
            output(
                "#13 at tick 0: modifier hero_of_riverhold on player: 10.00 from everyone, until tick 50"
            )
        );
        assert_eq!(
            session.execute("modifiers player"),
            output(
                "bribed: 20.00 from captain_hale\nhero_of_riverhold: 10.00 from everyone, until tick 50"
            )
        );
        let Ok(Outcome::Output(explained)) =
            session.execute("disposition captain_hale player --explain")
        else {
            panic!("expected the explanation");
        };
        // DESIGN.md §8.2's −29.10, plus 30.00.
        assert!(
            explained.starts_with("captain_hale → player: 0.90 (neutral)\n"),
            "{explained}"
        );
        assert!(
            explained.contains(
                "\nmodifiers: 30.00 × 1.00 = 30.00 (bribed 20.00 from captain_hale; hero_of_riverhold 10.00 from everyone)\n"
            ),
            "{explained}"
        );
        assert_eq!(
            session.execute("unmodify player bribed"),
            output("#14 at tick 0: modifier bribed on player removed")
        );
        assert_eq!(
            session.execute("modify temple player blessed 5"),
            output("#15 at tick 0: modifier blessed on player: 5.00 from temple")
        );
        assert_eq!(
            session.execute("modifiers vex"),
            output("vex has no modifiers")
        );
    }

    #[test]
    fn modifier_commands_report_mistakes() {
        let mut session = riverhold();
        let usage = command_error(
            "modify needs the form: modify <observer|everyone> <subject> <id> <amount> [--until <tick>]",
        );
        for line in [
            "modify everyone player",
            "modify everyone player x 5 --until",
            "modify everyone player x 5 --for 3",
            "modify everyone player x 5 --until --soon",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
        assert_eq!(
            session.execute("modify everyone player x five"),
            command_error("'five' is not a number")
        );
        assert_eq!(
            session.execute("modify everyone player x 5 --until soon"),
            command_error("'soon' is not a whole tick")
        );
        assert_eq!(
            session.execute("modify ghost player x 5"),
            command_error("unknown character 'ghost'")
        );
        assert_eq!(
            session.execute("unmodify player x"),
            command_error("player has no modifier 'x'")
        );
        assert_eq!(
            session.execute("unmodify player"),
            command_error("unmodify needs the form: unmodify <subject> <id>")
        );
        assert_eq!(
            session.execute("modifiers"),
            command_error("modifiers needs the form: modifiers <subject>")
        );
    }

    #[test]
    fn watching_lists_each_observers_band_lowest_first() {
        let mut session = riverhold();
        assert_eq!(session.execute("watching"), output("no one is watched yet"));
        assert_eq!(
            session.execute("watch player"),
            output("#1 at tick 0: now watching player (10 observers)")
        );
        // The Free Company (2.50 away) and Ava (22.36 away) are friendly; everyone else is
        // neutral toward a neutral player.
        assert_eq!(
            session.execute("watching"),
            output(
                "player — neutral: ashen_circle, city_watch, lantern_guild, temple, brother_ash, \
                 captain_hale, sister_mira, vex; friendly: free_company, merchant_ava"
            )
        );
    }

    #[test]
    fn a_watched_thief_hears_when_the_watch_turns_unfriendly() {
        let mut session = riverhold();
        for line in [
            "act player steal --target merchant_ava",
            "act player steal --target merchant_ava",
            "watch player",
        ] {
            session.execute(line).expect("valid");
        }
        assert_eq!(
            session.execute("outcome fined_by_watch player"),
            output(
                "#8 at tick 0: outcome fined_by_watch applied to player\n\
                 #9 at tick 0: player's standing with city_watch moved from 0.00 to -20.00\n\
                 #10 at tick 0: player's standing with lantern_guild moved from 0.00 to 3.60, with 3.60 spilled from city_watch (-20.00 × -0.18; lantern_guild regards it at -80.00)\n\
                 #11 at tick 0: player's standing with temple moved from 0.00 to -2.00, with -2.00 spilled from city_watch (-20.00 × 0.10; temple regards it at 60.00)\n\
                 #12 at tick 0: player's standing with captain_hale moved from 0.00 to -10.00\n\
                 #13 at tick 0: city_watch now regards player as unfriendly (was neutral), at -27.24\n\
                 #14 at tick 0: captain_hale now regards player as unfriendly (was neutral), at -29.10"
            )
        );
        assert_eq!(
            session.execute("unwatch player"),
            output("#15 at tick 0: no longer watching player")
        );
        assert_eq!(
            session.execute("journal"),
            output(
                "1. act player steal --target merchant_ava — accepted\n\
                 2. act player steal --target merchant_ava — accepted\n\
                 3. watch player — accepted\n\
                 4. outcome fined_by_watch player — accepted\n\
                 5. unwatch player — accepted"
            )
        );
    }

    #[test]
    fn watch_and_unwatch_report_mistakes() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("watch"),
            command_error("watch needs the form: watch <character>")
        );
        assert_eq!(
            session.execute("unwatch player vex"),
            command_error("unwatch needs the form: unwatch <character>")
        );
        assert_eq!(
            session.execute("watch plyer"),
            command_error("unknown character 'plyer' (did you mean 'player'?)")
        );
        assert_eq!(
            session.execute("unwatch player"),
            command_error("player isn't watched")
        );
        assert_eq!(
            session.execute("watch Player"),
            command_error(
                "'Player' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            )
        );
        let none = command_error("no world is loaded yet: use load <dir> first");
        assert_eq!(run("watch player"), none);
        assert_eq!(run("watching"), none);
    }

    #[test]
    fn act_explain_is_a_flag_and_a_refused_act_has_no_working() {
        let mut session = riverhold();
        let usage = command_error(
            "act needs the form: act <actor> <action> [--target <id>] [--scale <n>] [--explain]",
        );
        assert_eq!(
            session.execute("act player steal --explain --explain"),
            usage
        );
        assert_eq!(session.execute("act player steal --scale --explain"), usage);
        assert_eq!(
            session.execute("act player stael --explain"),
            command_error("unknown action 'stael' (did you mean 'steal'?)")
        );
        assert_eq!(
            session.execute("act player --explain steal"),
            usage,
            "options come after the action"
        );
    }

    #[test]
    fn acts_change_standing_and_standing_shows_it() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("act player steal --target vex"),
            output(
                "#1 at tick 0: player did steal, targeting vex\n\
                 #2 at tick 0: player's alignment moved from law 0.00, good 0.00 to law -5.00, good -3.00\n\
                 #3 at tick 0: player's standing with city_watch moved from 0.00 to 1.80, with 1.80 spilled from lantern_guild (-10.00 × -0.18; city_watch regards it at -80.00)\n\
                 #4 at tick 0: player's standing with lantern_guild moved from 0.00 to -10.00\n\
                 #5 at tick 0: player's standing with vex moved from 0.00 to -20.00"
            )
        );
        assert_eq!(
            session.execute("standing player"),
            output("city_watch: 1.80\nlantern_guild: -10.00\nvex: -20.00")
        );
        assert_eq!(session.execute("standing player vex"), output("-20.00"));
        assert_eq!(session.execute("standing player temple"), output("0.00"));
        assert_eq!(
            session.execute("standing merchant_ava"),
            output("no standing with anyone yet")
        );
        assert_eq!(
            session.execute("standing captain_hale"),
            output("city_watch: 75.00")
        );
    }

    #[test]
    fn outcomes_lists_them_and_outcome_applies_one() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("outcomes"),
            output(
                "fenced_the_crown_jewels — standing: lantern_guild 30.00\n\
                 fined_by_watch — standing: city_watch -20.00, captain_hale -10.00\n\
                 rescued_merchant — alignment: law 0.00, good 6.00 — standing: city_watch 10.00, merchant_ava 30.00"
            )
        );
        assert_eq!(
            session.execute("outcome fined_by_watch player"),
            output(
                "#1 at tick 0: outcome fined_by_watch applied to player\n\
                 #2 at tick 0: player's standing with city_watch moved from 0.00 to -20.00\n\
                 #3 at tick 0: player's standing with lantern_guild moved from 0.00 to 3.60, with 3.60 spilled from city_watch (-20.00 × -0.18; lantern_guild regards it at -80.00)\n\
                 #4 at tick 0: player's standing with temple moved from 0.00 to -2.00, with -2.00 spilled from city_watch (-20.00 × 0.10; temple regards it at 60.00)\n\
                 #5 at tick 0: player's standing with captain_hale moved from 0.00 to -10.00"
            )
        );
        assert_eq!(
            session.execute("journal"),
            output("1. outcome fined_by_watch player — accepted")
        );
    }

    #[test]
    fn standing_and_outcome_report_mistakes() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "outcome fined_by_wach player",
                "unknown outcome 'fined_by_wach' (did you mean 'fined_by_watch'?)",
            ),
            (
                "outcome fined_by_watch plyer",
                "unknown character 'plyer' (did you mean 'player'?)",
            ),
            (
                "standing plyer",
                "unknown character 'plyer' (did you mean 'player'?)",
            ),
            (
                "standing player tempel",
                "unknown faction or character 'tempel' (did you mean 'temple'?)",
            ),
            (
                "outcome Fined player",
                "'Fined' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        for (line, usage) in [
            (
                "standing",
                "standing needs the form: standing <subject> [<party>]",
            ),
            (
                "standing a b c",
                "standing needs the form: standing <subject> [<party>]",
            ),
            (
                "outcome fined_by_watch",
                "outcome needs the form: outcome <outcome> <character>",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(usage), "{line}");
        }
        let none = command_error("no world is loaded yet: use load <dir> first");
        for line in [
            "standing player",
            "outcomes",
            "outcome fined_by_watch player",
        ] {
            assert_eq!(run(line), none, "{line}");
        }
    }

    #[test]
    fn leaving_the_free_company_costs_standing_with_it() {
        let mut session = riverhold();
        session.execute("join player free_company").expect("valid");
        assert_eq!(
            session.execute("leave player free_company"),
            output(
                "#2 at tick 0: player left free_company (voluntary)\n\
                 #3 at tick 0: player's standing with free_company moved from 0.00 to -10.00"
            )
        );
    }

    #[test]
    fn describes_effects_from_other_modules() {
        let event = Event {
            seq: 2,
            tick: Tick(5),
            payload: Change::EffectsApplied {
                source: "quest:lost_relic".to_owned(),
                character: CharacterId::new("player").expect("valid id"),
            },
        };
        assert_eq!(
            describe_event(&event),
            "#2 at tick 5: effects from quest:lost_relic applied to player"
        );
        let command = Command::ApplyEffects {
            source: "quest:lost_relic".to_owned(),
            character: CharacterId::new("player").expect("valid id"),
            effects: Effects::default(),
        };
        assert_eq!(
            describe_command(&command),
            "effects from quest:lost_relic on player"
        );
    }

    // Relations

    #[test]
    fn relations_lists_each_direction_with_its_band_and_conflicts() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("relations city_watch"),
            output(
                "ashen_circle → city_watch: -40.00 (rival)\n\
                 city_watch → ashen_circle: -40.00 (rival)\n\
                 city_watch → free_company: -30.00 (rival)\n\
                 city_watch → lantern_guild: -80.00 (enemy) — in conflict\n\
                 city_watch → temple: 60.00 (allied)\n\
                 free_company → city_watch: -10.00 (neutral)\n\
                 lantern_guild → city_watch: -80.00 (enemy) — in conflict\n\
                 temple → city_watch: 60.00 (allied)"
            )
        );
        let Ok(Outcome::Output(all)) = session.execute("relations") else {
            panic!("expected the list");
        };
        // Five `between` entries set two directions each, and two set one.
        assert_eq!(all.lines().count(), 12);
        assert!(
            all.contains("temple → ashen_circle: -90.00 (enemy) — in conflict"),
            "{all}"
        );
        assert_eq!(
            session.execute("relations tempel"),
            command_error("unknown faction 'tempel' (did you mean 'temple'?)")
        );
        assert_eq!(
            session.execute("relations a b"),
            command_error("relations needs the form: relations [<faction>]")
        );
    }

    #[test]
    fn relations_says_when_there_are_none() {
        let mut session = repo();
        session
            .execute("load crates/cli/tests/fixtures/worlds/diamonds")
            .expect("valid");
        assert_eq!(session.execute("relations"), output("no relations set"));
        assert_eq!(
            session.execute("relations city_watch"),
            output("no relations set for city_watch")
        );
    }

    #[test]
    fn relate_sets_or_shifts_a_relation_and_shows_the_events() {
        let mut session = riverhold();
        assert_eq!(
            session.execute("relate city_watch free_company -60 --one-way"),
            output("#1 at tick 0: city_watch → free_company changed from -30.00 to -60.00")
        );
        assert_eq!(
            session.execute("relate temple ashen_circle --by 50"),
            output(
                "#2 at tick 0: temple → ashen_circle changed from -90.00 to -40.00\n\
                 #3 at tick 0: ashen_circle → temple changed from -90.00 to -40.00"
            )
        );
        assert_eq!(
            session.execute("relate city_watch temple --by -15 --one-way"),
            output("#4 at tick 0: city_watch → temple changed from 60.00 to 45.00")
        );
        assert_eq!(
            session.execute("relate city_watch temple 45 --one-way"),
            output("")
        );
        assert_eq!(
            session.execute("journal"),
            output(
                "1. relate city_watch free_company -60.00 --one-way — accepted\n\
                 2. relate temple ashen_circle --by 50.00 — accepted\n\
                 3. relate city_watch temple --by -15.00 --one-way — accepted\n\
                 4. relate city_watch temple 45.00 --one-way — accepted"
            )
        );
    }

    #[test]
    fn relate_reports_refusals_and_bad_input() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "relate city_wach temple 10",
                "unknown faction 'city_wach' (did you mean 'city_watch'?)",
            ),
            (
                "relate temple temple 10",
                "a faction can't have a relation with itself",
            ),
            (
                "relate temple city_watch 150",
                "150.00 is outside -100.00..100.00",
            ),
            ("relate temple city_watch x", "'x' is not a number"),
            (
                "relate temple city_watch --by 1.234",
                "1.234 has more than 2 decimal places",
            ),
            (
                "relate Temple city_watch 10",
                "'Temple' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        let usage = command_error(
            "relate needs the form: relate <from> <to> <value> [--one-way], or relate <from> <to> --by <n> [--one-way]",
        );
        for line in [
            "relate",
            "relate temple city_watch",
            "relate temple city_watch --by",
            "relate temple city_watch 10 --both",
            "relate temple city_watch --by --one-way",
            "relate temple city_watch --by --by",
            "relate temple city_watch --by --by --one-way",
            "relate temple city_watch --both --one-way",
            "relate temple city_watch 10 --one-way extra",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
        let none = command_error("no world is loaded yet: use load <dir> first");
        assert_eq!(run("relations"), none);
        assert_eq!(run("relate temple city_watch 10"), none);
    }

    #[test]
    fn enemy_membership_bars_joining_until_the_war_ends() {
        let mut session = riverhold();
        session
            .execute("act player steal --scale 4")
            .expect("valid");
        session.execute("join player lantern_guild").expect("valid");
        assert_eq!(
            session.execute("can-join player city_watch"),
            output(
                "no: 90.35 from The City Watch, tolerance is 40.00; \
                 player belongs to The Lantern Guild, in conflict with The City Watch (-80.00): \
                 refused by The City Watch's defectors rule 3, \"You serve our enemies.\""
            )
        );
        session
            .execute("relate city_watch lantern_guild 0")
            .expect("valid");
        assert_eq!(
            session.execute("can-join player city_watch"),
            output("no: 90.35 from The City Watch, tolerance is 40.00")
        );
    }

    #[test]
    fn can_join_explains_how_each_rule_table_decided() {
        let mut session = riverhold();
        session
            .execute("act player steal --scale 4")
            .expect("valid");
        session.execute("join player lantern_guild").expect("valid");
        assert_eq!(
            session.execute("can-join player city_watch --explain"),
            output(
                "player → city_watch: no\n\
                 distance 90.35 (euclidean), tolerance 40.00\n\
                 law: 70.00 vs -20.00, gap 90.00, weight 1.00\n\
                 good: 20.00 vs -12.00, gap 32.00, weight 0.25\n\
                 weights: city_watch's own\n\
                 leaving lantern_guild, in conflict with city_watch (-80.00)\n\
                 lantern_guild's deserters (balance.toml): rule 3, release, standing -40.00\n\
                 \x20 rule 1: rank_at_least = 3? no, cutpurse is rung 1\n\
                 \x20 rule 2: outside_member_tolerance = true? no, 40.01 from lantern_guild, tolerance 60.00\n\
                 \x20 rule 3: always\n\
                 city_watch's defectors (balance.toml): rule 3, refuse, \"You serve our enemies.\"\n\
                 \x20 rule 1: standing_with_target_at_least = 50.00? no, standing 0.00\n\
                 \x20 rule 2: closer_to_target = true? no, 90.35 from city_watch, 40.01 from lantern_guild\n\
                 \x20 rule 3: always\n\
                 refused: 90.35 from The City Watch, tolerance is 40.00\n\
                 refused: player belongs to The Lantern Guild, in conflict with The City Watch (-80.00): \
                 refused by The City Watch's defectors rule 3, \"You serve our enemies.\""
            )
        );
    }

    fn defectors() -> Session {
        let mut session = repo();
        let Ok(Outcome::Output(loaded)) =
            session.execute("load crates/cli/tests/fixtures/worlds/defectors")
        else {
            panic!("the defectors world loads");
        };
        assert!(loaded.starts_with("loaded 4 characters"), "{loaded}");
        session
    }

    #[test]
    fn a_reformed_fence_defects_and_the_events_say_so() {
        let mut session = defectors();
        assert_eq!(
            session.execute("join vex city_watch"),
            output(
                "#1 at tick 0: vex left lantern_guild (defected)\n\
                 #2 at tick 0: vex joined city_watch as a recruit\n\
                 #3 at tick 0: vex's standing with city_watch moved from 0.00 to -10.00\n\
                 #4 at tick 0: vex's standing with lantern_guild moved from 0.00 to 1.80, with 1.80 spilled from city_watch (-10.00 × -0.18; lantern_guild regards it at -80.00)"
            )
        );
    }

    #[test]
    fn can_join_explains_rank_ids_and_a_factions_own_table() {
        let mut session = defectors();
        let Ok(Outcome::Output(explained)) =
            session.execute("can-join kestrel city_watch --explain")
        else {
            panic!("expected the explanation");
        };
        assert!(
            explained.contains(
                "\nlantern_guild's deserters (balance.toml): rule 1, refuse, \"Officers don't walk away.\"\n\
                 \x20 rule 1: rank_at_least = 3? yes, shadow is rung 3\n\
                 city_watch's defectors (balance.toml): rule 2, accept, standing -10.00\n"
            ),
            "{explained}"
        );
        let Ok(Outcome::Output(explained)) =
            session.execute("can-join brother_ash temple --explain")
        else {
            panic!("expected the explanation");
        };
        assert!(
            explained.contains(
                "\nashen_circle's deserters (factions.toml): rule 1, refuse, \"No one leaves the Circle.\"\n\
                 \x20 rule 1: always\n\
                 temple's defectors (factions.toml): rule 1, accept, standing -5.00\n\
                 \x20 rule 1: rank_below = \"ordained\"? yes, initiate is rung 1, ordained is rung 2\n"
            ),
            "{explained}"
        );
    }

    #[test]
    fn a_war_between_two_of_a_characters_factions_is_theirs_to_settle() {
        let mut session = riverhold();
        session
            .execute("act player steal --scale 4")
            .expect("valid");
        session.execute("join player lantern_guild").expect("valid");
        session.execute("join player free_company").expect("valid");
        assert_eq!(
            session.execute("relate lantern_guild free_company -60"),
            output(
                "#5 at tick 0: lantern_guild → free_company changed from 20.00 to -60.00\n\
                 #6 at tick 0: free_company → lantern_guild changed from 20.00 to -60.00\n\
                 #7 at tick 0: free_company and lantern_guild are now in conflict, and player belongs to both"
            )
        );
        assert_eq!(
            session.execute("show character player"),
            output(
                "player — The Player — law -20.00, good -12.00 — True Neutral — member of free_company (sellsword) since tick 0, lantern_guild (cutpurse) since tick 0 — at war: free_company and lantern_guild"
            )
        );
        // Leaving the Company costs 10.00 there; the Guild, now at −60 with it, gets +0.60.
        assert_eq!(
            session.execute("resolve player lantern_guild"),
            output(
                "#8 at tick 0: player left free_company (the war between their factions was settled)\n\
                 #9 at tick 0: player's standing with free_company moved from 0.00 to -10.00\n\
                 #10 at tick 0: player's standing with lantern_guild moved from 0.00 to 0.60, with 0.60 spilled from free_company (-10.00 × -0.06; lantern_guild regards it at -60.00)"
            )
        );
        assert_eq!(
            session.execute("resolve player lantern_guild"),
            command_error("player has no open conflict involving lantern_guild")
        );
        assert_eq!(
            session.execute("resolve player"),
            command_error("resolve needs the form: resolve <character> <faction-to-keep>")
        );
    }

    // Disposition

    #[test]
    fn disposition_gives_the_score_and_its_band() {
        let mut session = riverhold();
        for (line, expected) in [
            ("disposition city_watch player", "-3.64 (neutral)"),
            ("disposition temple sister_mira", "100.00 (friendly)"),
            ("disposition temple brother_ash", "-77.15 (unfriendly)"),
            ("disposition city_watch vex", "-63.36 (unfriendly)"),
            ("disposition captain_hale sister_mira", "44.75 (friendly)"),
        ] {
            assert_eq!(session.execute(line), output(expected), "{line}");
        }
    }

    #[test]
    fn disposition_explains_its_working() {
        assert_eq!(
            riverhold().execute("disposition city_watch player --explain"),
            output(
                "city_watch → player: -3.64 (neutral)\n\
                 affinity: -3.64 × 1.00 = -3.64, at distance 70.18 (euclidean)\n\
                 \x20 law: 70.00 vs 0.00, gap 70.00, weight 1.00\n\
                 \x20 good: 20.00 vs 0.00, gap 20.00, weight 0.25\n\
                 \x20 weights: city_watch's own\n\
                 standing: 0.00 × 1.00 = 0.00\n\
                 kinship: 0.00 × 0.50 = 0.00 (player belongs to no factions)\n\
                 faction opinion: 0.00 × 0.50 = 0.00 (only characters have one)\n\
                 modifiers: 0.00 × 1.00 = 0.00\n\
                 bands: unfriendly ≤ -25.00 < neutral ≤ 25.00 < friendly"
            )
        );
    }

    #[test]
    fn disposition_explains_the_worked_example() {
        // DESIGN.md §8.2.
        let mut session = riverhold();
        for line in [
            "act player steal --target merchant_ava",
            "act player steal --target merchant_ava",
            "outcome fined_by_watch player",
        ] {
            session.execute(line).expect("valid");
        }
        assert_eq!(
            session.execute("disposition captain_hale player --explain"),
            output(
                "captain_hale → player: -29.10 (unfriendly)\n\
                 affinity: -9.10 × 1.00 = -9.10, at distance 85.48 (euclidean)\n\
                 \x20 law: 75.00 vs -10.00, gap 85.00, weight 1.00\n\
                 \x20 good: 30.00 vs -6.00, gap 36.00, weight 0.25\n\
                 \x20 weights: captain_hale's own\n\
                 standing: -10.00 × 1.00 = -10.00\n\
                 kinship: 0.00 × 0.50 = 0.00 (player belongs to no factions)\n\
                 faction opinion: -20.00 × 0.50 = -10.00 (city_watch -20.00)\n\
                 modifiers: 0.00 × 1.00 = 0.00\n\
                 bands: unfriendly ≤ -25.00 < neutral ≤ 25.00 < friendly"
            )
        );
    }

    #[test]
    fn disposition_explains_where_kinship_comes_from() {
        let mut session = riverhold();
        let Ok(Outcome::Output(watch)) = session.execute("disposition city_watch vex --explain")
        else {
            panic!("expected the explanation");
        };
        assert!(
            watch.contains(
                "\nkinship: -80.00 × 0.50 = -40.00 (city_watch → lantern_guild -80.00)\n"
            ),
            "{watch}"
        );
        let Ok(Outcome::Output(temple)) =
            session.execute("disposition temple sister_mira --explain")
        else {
            panic!("expected the explanation");
        };
        assert!(
            temple.contains("\nkinship: 50.00 × 0.50 = 25.00 (both in temple 50.00)\n"),
            "{temple}"
        );
        assert!(
            temple.contains("\nstanding: 40.00 × 1.00 = 40.00\n"),
            "{temple}"
        );
        let Ok(Outcome::Output(ava)) = session.execute("disposition merchant_ava player --explain")
        else {
            panic!("expected the explanation");
        };
        assert!(
            ava.contains("\nkinship: 0.00 × 0.50 = 0.00 (merchant_ava belongs to no factions)\n"),
            "{ava}"
        );
        assert!(
            ava.contains(
                "\nfaction opinion: 0.00 × 0.50 = 0.00 (merchant_ava belongs to no factions)\n"
            ),
            "{ava}"
        );
    }

    #[test]
    fn disposition_uses_the_loaded_bands() {
        let mut session = repo();
        session
            .execute("load crates/cli/tests/fixtures/worlds/hostile")
            .expect("valid");
        assert_eq!(
            session.execute("disposition temple brother_ash"),
            output("-32.15 (hostile)")
        );
    }

    #[test]
    fn disposition_reports_unknown_or_wrong_ids_and_bad_input() {
        let mut session = riverhold();
        for (line, message) in [
            (
                "disposition city_wach player",
                "unknown observer 'city_wach' (did you mean 'city_watch'?)",
            ),
            (
                "disposition city_watch plyer",
                "unknown subject 'plyer' (did you mean 'player'?)",
            ),
            (
                "disposition player temple",
                "a disposition's subject must be a character",
            ),
        ] {
            assert_eq!(session.execute(line), command_error(message), "{line}");
        }
        let usage = command_error(
            "disposition needs the form: disposition <observer> <subject> [--explain]",
        );
        for line in [
            "disposition",
            "disposition temple",
            "disposition temple vex --why",
        ] {
            assert_eq!(session.execute(line), usage, "{line}");
        }
        assert_eq!(
            run("disposition temple vex"),
            command_error("no world is loaded yet: use load <dir> first")
        );
    }

    #[test]
    fn curve_gives_a_curves_value_at_a_point() {
        assert_eq!(
            run("curve [[0, 1.00], [50, 0.70], [100, 0.30]] at 25"),
            output("0.85")
        );
        assert_eq!(
            run("curve [[0, 50], [60, 0], [200, -50]] at -7"),
            output("50.00")
        );
        assert_eq!(run("curve 1.0 at 999"), output("1.00"));
    }

    #[test]
    fn curve_reports_invalid_curves_and_bad_numbers_as_command_errors() {
        assert_eq!(
            run("curve [[50, 1.0], [40, 0.0]] at 45"),
            command_error("curve points must have increasing x: 50.00 then 40.00")
        );
        assert_eq!(
            run("curve [[0, 1.0], [10, 2.0]] at 1.234"),
            command_error("1.234 has more than 2 decimal places")
        );
    }

    #[test]
    fn curve_needs_a_curve_and_a_point() {
        let usage = command_error(
            "curve needs the form: curve <curve> [at <x>], where <curve> is a knob, such as disposition.affinity, or a curve, such as [[0, 1.0], [100, 0.5]]",
        );
        assert_eq!(run("curve"), usage);
        assert_eq!(run("curve at 5"), usage);
    }

    #[test]
    fn calc_needs_two_numbers_and_an_operator() {
        let usage =
            command_error("calc needs the form: calc <a> <op> <b>, where <op> is + - * or /");
        assert_eq!(run("calc"), usage);
        assert_eq!(run("calc 1 +"), usage);
        assert_eq!(run("calc 1 + 2 + 3"), usage);
        assert_eq!(run("calc 1 % 2"), usage);
    }

    #[test]
    fn help_lists_every_command_with_its_usage() {
        let help = help_text();
        for usage in [
            "help",
            "quit",
            "load <dir>",
            "characters",
            "show character <id>",
            "factions",
            "show faction <id>",
            "distance <observer> <subject> [--explain]",
            "disposition <observer> <subject> [--explain]",
            "can-join <character> <faction> [--explain]",
            "join <character> <faction>",
            "leave <character> <faction>",
            "ranks <faction>",
            "promote <character> <faction> [--explain]",
            "demote <character> <faction>",
            "standing <subject> [<party>]",
            "outcomes",
            "outcome <outcome> <character>",
            "relations [<faction>]",
            "relate <from> <to> <value> [--one-way]",
            "relate <from> <to> --by <n> [--one-way]",
            "actions",
            "act <actor> <action> [--target <id>] [--scale <n>] [--explain]",
            "advance <ticks>",
            "time",
            "events [--since <seq>]",
            "journal",
            "calc <a> <op> <b>",
            "curve <curve> [at <x>]",
            "map <faction>",
            "matrix [<subject>...] [--csv]",
            "echo <text>",
            "fail <message>",
            "assert <command> == <expected>",
        ] {
            assert!(help.contains(usage), "help is missing {usage:?}:\n{help}");
        }
    }

    #[test]
    fn an_unknown_command_is_a_script_error_that_names_it() {
        let error = run("frobnicate now").unwrap_err();
        assert_eq!(error.to_string(), "unknown command 'frobnicate'");
    }

    #[test]
    fn a_matching_assert_passes_without_output() {
        assert_eq!(run("assert echo hi == hi"), passes());
    }

    #[test]
    fn a_mismatched_assert_reports_what_was_expected_and_what_came_back() {
        let error = run("assert echo hi == bye").unwrap_err();
        assert_eq!(error.to_string(), "expected 'bye', got 'hi'");
    }

    #[test]
    fn assert_compares_a_failed_command_as_its_error_text() {
        assert_eq!(run("assert fail boom == error: boom"), passes());
    }

    #[test]
    fn assert_splits_at_the_first_separator() {
        let error = run("assert echo a == b == c").unwrap_err();
        assert_eq!(error.to_string(), "expected 'b == c', got 'a'");
    }

    #[test]
    fn assert_can_expect_an_empty_result() {
        assert_eq!(run("assert echo =="), passes());
    }

    #[test]
    fn assert_without_a_separator_is_a_script_error() {
        let error = run("assert echo hi").unwrap_err();
        assert_eq!(
            error.to_string(),
            "assert needs the form: assert <command> == <expected>"
        );
    }

    #[test]
    fn assert_of_an_unknown_command_is_a_script_error() {
        let error = run("assert frobnicate == anything").unwrap_err();
        assert_eq!(error.to_string(), "unknown command 'frobnicate'");
    }

    #[test]
    fn rendering_prefixes_errors_and_shows_output_as_is() {
        assert_eq!(Outcome::Output("hi".into()).render(), "hi");
        assert_eq!(Outcome::Error("boom".into()).render(), "error: boom");
        assert_eq!(Outcome::Quit.render(), "");
    }
}

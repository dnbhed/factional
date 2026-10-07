//! Saves (T4, P-54): a session's journal and events, with the content they came from named by
//! directory and fingerprinted. Written as JSON for now; what a save holds (`SaveFile`) is
//! kept apart from how it's written, so a binary encoding can follow (T5).

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use factional_reputation::{Event, RestoreError, SavedCommand, World};

use crate::{ContentError, load_dir_fingerprinted};

/// The format name every save starts with.
const SAVE_FORMAT: &str = "factional-save";

/// The save format version this build writes and reads.
pub const SAVE_VERSION: u32 = 1;

/// Each content file's fingerprint, or `None` if it isn't there, by file name.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(BTreeMap<String, Option<String>>);

impl Fingerprint {
    /// The fingerprint of each file's text, `None` for a file that isn't there.
    pub(crate) fn of<'a>(
        files: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) -> Fingerprint {
        Fingerprint(
            files
                .into_iter()
                .map(|(file, text)| {
                    (
                        file.to_owned(),
                        text.map(|text| fingerprint_of(text.as_bytes())),
                    )
                })
                .collect(),
        )
    }

    /// The files whose fingerprints differ between the two, in name order.
    fn changed(&self, now: &Fingerprint) -> Vec<String> {
        let mut files: Vec<&String> = self.0.keys().chain(now.0.keys()).collect();
        files.sort();
        files.dedup();
        files
            .into_iter()
            .filter(|file| self.0.get(*file) != now.0.get(*file))
            .cloned()
            .collect()
    }
}

/// A file's fingerprint: its FNV-1a 64-bit hash, as `fnv1a64:<16 hex digits>`. It notices
/// any edit; it isn't meant to resist tampering.
pub fn fingerprint_of(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let hash = bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    });
    format!("fnv1a64:{hash:016x}")
}

/// Everything a save holds, in the order it's written: its format and version first, so any
/// build can tell what it's reading.
#[derive(serde::Serialize, serde::Deserialize)]
struct SaveFile {
    format: String,
    version: u32,
    content: SavedContent,
    journal: Vec<SavedCommand>,
    events: Vec<Event>,
}

/// The content a save was played on: its directory, as loaded, and each file's fingerprint.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedContent {
    dir: String,
    files: Fingerprint,
}

/// A world restored from a save, with the content directory it was loaded from.
#[derive(Debug)]
pub struct Restored {
    pub world: World,
    pub dir: String,
    pub fingerprint: Fingerprint,
}

/// Why a save can't be restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    NotJson(String),
    NotASave,
    Version { found: u64 },
    Unreadable(String),
    ContentDoesNotLoad { dir: String, problems: ContentError },
    ContentChanged { dir: String, files: Vec<String> },
    DoesNotFit(RestoreError),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::NotJson(message) => write!(f, "this save isn't valid JSON: {message}"),
            SaveError::NotASave => f.write_str("this isn't a Factional save"),
            SaveError::Version { found } => write!(
                f,
                "this save is version {found}, but this build reads version {SAVE_VERSION}"
            ),
            SaveError::Unreadable(message) => write!(f, "this save can't be read: {message}"),
            SaveError::ContentDoesNotLoad { dir, problems } => {
                let problems: Vec<String> = problems
                    .diagnostics
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                write!(
                    f,
                    "the content in {dir} doesn't load: {}",
                    problems.join("; ")
                )
            }
            SaveError::ContentChanged { dir, files } => write!(
                f,
                "the content in {dir} has changed since this save: {}",
                files.join(", ")
            ),
            SaveError::DoesNotFit(error) => write!(f, "this save doesn't fit its content: {error}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// A save of `world`, loaded from `dir` with `fingerprint`, as JSON text ending in a newline.
pub fn save(world: &World, dir: &str, fingerprint: &Fingerprint) -> String {
    let file = SaveFile {
        format: SAVE_FORMAT.to_owned(),
        version: SAVE_VERSION,
        content: SavedContent {
            dir: dir.to_owned(),
            files: fingerprint.clone(),
        },
        journal: world.saved_journal(),
        events: world.events().to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).expect("a save always serialises");
    format!("{text}\n")
}

/// Restores a save, reading its content directory from `base`. The content must be exactly
/// as it was when the session was saved.
pub fn restore(text: &str, base: &Path) -> Result<Restored, SaveError> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|error| SaveError::NotJson(error.to_string()))?;
    if json.get("format").and_then(serde_json::Value::as_str) != Some(SAVE_FORMAT) {
        return Err(SaveError::NotASave);
    }
    match json.get("version").and_then(serde_json::Value::as_u64) {
        Some(version) if version == u64::from(SAVE_VERSION) => {}
        Some(found) => return Err(SaveError::Version { found }),
        None => return Err(SaveError::Unreadable("it has no version".to_owned())),
    }
    let file: SaveFile =
        serde_json::from_value(json).map_err(|error| SaveError::Unreadable(error.to_string()))?;
    let dir = file.content.dir;
    let (content, fingerprint) = load_dir_fingerprinted(&base.join(&dir)).map_err(|problems| {
        SaveError::ContentDoesNotLoad {
            dir: dir.clone(),
            problems,
        }
    })?;
    let files = file.content.files.changed(&fingerprint);
    if !files.is_empty() {
        return Err(SaveError::ContentChanged { dir, files });
    }
    let world =
        World::restore(content, &file.journal, &file.events).map_err(SaveError::DoesNotFit)?;
    Ok(Restored {
        world,
        dir,
        fingerprint,
    })
}

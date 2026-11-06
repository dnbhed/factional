//! The `factional` binary: `factional repl` for an interactive session, `factional run
//! <script>` to replay a scenario (DECISIONS.md P-27), and for designers, `factional validate
//! <dir>`, `factional schema [<file>]`, and `factional world info <file>`, `factional world source
//! <file>`, `factional world suggestions <map>` and `factional world import <export> <map>`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand};
use factional_cli::{
    Import, compare, run_repl, run_script, validate, world_import, world_info, world_source,
    world_suggestions,
};
use factional_content::{SCHEMA_FILES, schema_text};
use factional_core::Fixed;
use factional_worldgen::{Coast, Detail, Foliage, Levels, PerKind};

#[derive(Parser)]
#[command(
    name = "factional",
    version,
    about = "Factional's command line: an interactive REPL and a scenario runner"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start an interactive session; `help` lists the commands
    Repl,
    /// Run a scenario script, stopping at the first line that goes wrong
    Run {
        /// The `.scenario` file to run
        script: PathBuf,
    },
    /// Run a scenario on two content directories and show what came out differently
    Compare {
        /// The `.scenario` file to run; it must load exactly one world
        scenario: PathBuf,
        /// The content directory to run it on first
        #[arg(long)]
        content: String,
        /// The content directory to compare it with
        #[arg(long)]
        against: String,
    },
    /// Check a content directory without starting a session
    Validate {
        /// The content directory, such as content/sample
        dir: String,
    },
    /// Print a content file's JSON Schema, for editors; without a file, list the files
    Schema {
        /// The content file, such as factions
        #[arg(value_parser = PossibleValuesParser::new(SCHEMA_FILES))]
        file: Option<String>,
    },
    /// Read world map files and Azgaar exports
    World {
        #[command(subcommand)]
        command: WorldCommand,
    },
}

#[derive(Subcommand)]
enum WorldCommand {
    /// What a map file holds: its size, chunks and overrides, and the recipe it was made from
    Info {
        /// The map file, such as world/menayland.map
        file: String,
    },
    /// What an Azgaar Full JSON export holds: its cells, vertices and features, or every problem in it
    Source {
        /// The export, such as "Menayland Full.json"
        file: String,
    },
    /// A map's suggestions: its settlements, routes and points of interest, where each lies and what the source says of it
    Suggestions {
        /// The map file, such as world/menayland.map
        map: String,
    },
    /// Rasterise an Azgaar Full JSON export into a map file: the whole map at 1 tile a pixel and seed 0 unless told
    Import {
        /// The export, such as "Menayland Full.json"
        export: String,
        /// The map file to write, such as world/menayland.map
        map: String,
        /// The source pixels to take, as <x>,<y>,<width>,<height>; the whole map if not given
        #[arg(long)]
        region: Option<String>,
        /// How many tiles a source pixel makes each way
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// The seed for generation
        #[arg(long, default_value_t = 0)]
        seed: u32,
        /// The detail's strength, as a share of the local relief: 0.00 to 1.00
        #[arg(long, default_value_t = Detail::default().strength)]
        detail: Fixed,
        /// The detail's longest wavelength, in source pixels: at least 1.00
        #[arg(long, default_value_t = Detail::default().wavelength)]
        wavelength: Fixed,
        /// The detail's octaves, each half the wavelength and strength of the one before: 1 to 8
        #[arg(long, default_value_t = Detail::default().octaves)]
        octaves: u32,
        /// How deep the sea's shallows reach, in metres: 0.00 to 1000.00
        #[arg(long, default_value_t = Coast::default().shallows)]
        shallows: Fixed,
        /// The fewest tiles a group of land or water keeps, away from the map's edge: 0 to 4096
        #[arg(long, default_value_t = Coast::default().specks)]
        specks: u32,
        /// How far apart levels are on Azgaar's height scale of 0 to 100: 1.00 to 80.00
        #[arg(long, default_value_t = Levels::default().spacing)]
        levels: Fixed,
        /// The fewest tiles a plateau or pit keeps its level with, away from the map's edge: 0
        /// to 4096
        #[arg(long, default_value_t = Levels::default().specks)]
        level_specks: u32,
        /// How much foliage: each biome's chances of a plant on a tile, times this: 0.00 to
        /// 10.00
        #[arg(long, default_value_t = Foliage::default().scale)]
        foliage: Fixed,
        /// How far apart trees stand, in tiles: 0.00 to 8.00
        #[arg(long, default_value_t = Foliage::default().spacing.tree)]
        tree_spacing: Fixed,
        /// How far apart shrubs stand, in tiles: 0.00 to 8.00
        #[arg(long, default_value_t = Foliage::default().spacing.shrub)]
        shrub_spacing: Fixed,
        /// How far apart grass stands, in tiles: 0.00 to 8.00
        #[arg(long, default_value_t = Foliage::default().spacing.grass)]
        grass_spacing: Fixed,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Repl => run_repl(),
        Command::Run { script } => run(&script),
        Command::Compare {
            scenario,
            content,
            against,
        } => {
            let source = match fs::read_to_string(&scenario) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("cannot read {}: {error}", scenario.display());
                    return ExitCode::FAILURE;
                }
            };
            let shown = scenario.display().to_string();
            match compare(&shown, &source, &content, &against, Path::new("")) {
                Ok(report) => {
                    print!("{report}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Validate { dir } => {
            let (report, loads) = validate(Path::new(""), &dir);
            print!("{report}");
            if loads {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Command::Schema { file } => {
            match file.as_deref().and_then(schema_text) {
                Some(schema) => print!("{schema}"),
                None => SCHEMA_FILES.iter().for_each(|file| println!("{file}")),
            }
            ExitCode::SUCCESS
        }
        Command::World { command } => match match command {
            WorldCommand::Info { file } => world_info(Path::new(""), &file),
            WorldCommand::Source { file } => world_source(Path::new(""), &file),
            WorldCommand::Suggestions { map } => world_suggestions(Path::new(""), &map),
            WorldCommand::Import {
                export,
                map,
                region,
                scale,
                seed,
                detail,
                wavelength,
                octaves,
                shallows,
                specks,
                levels,
                level_specks,
                foliage,
                tree_spacing,
                shrub_spacing,
                grass_spacing,
            } => world_import(
                Path::new(""),
                &Import {
                    export: &export,
                    map: &map,
                    region: region.as_deref(),
                    scale,
                    seed,
                    detail: Detail {
                        strength: detail,
                        wavelength,
                        octaves,
                    },
                    coast: Coast { shallows, specks },
                    levels: Levels {
                        spacing: levels,
                        specks: level_specks,
                    },
                    foliage: Foliage {
                        scale: foliage,
                        spacing: PerKind {
                            tree: tree_spacing,
                            shrub: shrub_spacing,
                            grass: grass_spacing,
                        },
                        ..Foliage::default()
                    },
                },
            ),
        } {
            Ok(info) => {
                println!("{info}");
                ExitCode::SUCCESS
            }
            Err(why) => {
                eprintln!("{why}");
                ExitCode::FAILURE
            }
        },
    }
}

/// Prints the script's transcript; if it stops early, reports the failing line on stderr.
fn run(path: &Path) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match run_script(&source, Path::new(".")) {
        Ok(transcript) => {
            print!("{transcript}");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            print!("{}", failure.transcript);
            eprintln!("{failure}");
            ExitCode::FAILURE
        }
    }
}

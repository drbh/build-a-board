use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use build_a_board::build;
use build_a_board::commands::{self, Ctx};
use build_a_board::process::Tools;
use build_a_board::script::BoardFile;
use clap::{Args, Parser, Subcommand};

/// Build, route, check, render, and release KiCad boards described in Starlark.
#[derive(Parser)]
#[command(name = "bb", version, about)]
struct Cli {
    /// Board directory holding `board.star`, or the board file itself.
    #[arg(short, long, global = true, env = "BB_BOARD", default_value = ".")]
    board: PathBuf,
    /// What to do; `build` when omitted (its flags work without the word `build`).
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    build: BuildArgs,
}

#[derive(Args)]
struct BuildArgs {
    /// Stop at the unrouted board.
    #[arg(long)]
    no_route: bool,
    /// Skip the connectivity schematic.
    #[arg(long)]
    no_schematic: bool,
    /// Skip rendering images.
    #[arg(long)]
    no_render: bool,
    /// Render only the top and bottom views (fast), not the perspective sheets and GLB model.
    #[arg(long)]
    quick: bool,
    /// Redo every enabled stage even if its inputs are unchanged.
    #[arg(long)]
    force: bool,
    /// FreeRouting runs before giving up.
    #[arg(long, env = "FREEROUTING_ATTEMPTS", default_value_t = 8)]
    attempts: u32,
    /// Seconds before a FreeRouting run counts as stalled.
    #[arg(long, env = "FREEROUTING_TIMEOUT_SECONDS", default_value_t = 900)]
    timeout: u64,
}

#[derive(Subcommand)]
enum Command {
    /// Generate, route, draw the schematic, and render, skipping stages whose inputs are unchanged.
    Build(BuildArgs),
    /// Check that the external tools are installed.
    Doctor,
    /// Generate the connectivity schematic and render it.
    Schematic {
        /// Directory for the SVG/PNG renders (default: output/render/<project>).
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run every validation gate.
    Check,
    /// Export fabrication files and the JLCPCB upload set.
    Export,
    /// Build the complete manufacturing release from the reviewed board.
    Prep,
    /// Pin the committed PCB and schematic as the reviewed revision.
    LockBoard,
    /// Remove generated output, keeping the KiCad project (and routed board).
    Clean {
        /// Also remove the KiCad project; files tracked by git are always kept.
        #[arg(long)]
        all: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let tools = Tools::from_env();
    let ctx = Ctx::new(BoardFile::load(&cli.board, &tools.footprints)?, tools);
    match cli.command.unwrap_or(Command::Build(cli.build)) {
        Command::Build(args) => build::build(
            &ctx,
            build::Options {
                route: !args.no_route,
                schematic: !args.no_schematic,
                render: !args.no_render,
                all_views: !args.quick,
                force: args.force,
                attempts: args.attempts,
                timeout: Duration::from_secs(args.timeout),
            },
        ),
        Command::Doctor => commands::doctor(&ctx),
        Command::Schematic { output } => commands::schematic(&ctx, output.as_deref()),
        Command::Check => commands::check(&ctx),
        Command::Export => commands::export(&ctx),
        Command::Prep => commands::prep(&ctx),
        Command::LockBoard => commands::lock_board(&ctx),
        Command::Clean { all } => commands::clean(&ctx, all),
    }
}

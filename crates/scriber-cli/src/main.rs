//! Headless entry point for Scriber.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use scriber_kernel::Solid;

#[derive(Parser)]
#[command(name = "scriber", version, about = "Scriber CAD, headless")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a reference solid and export it, proving the kernel works.
    Smoke {
        /// Where to write the STEP file.
        #[arg(short, long, default_value = "smoke.step")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Smoke { output } => match smoke(&output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn smoke(output: &PathBuf) -> Result<(), scriber_kernel::Error> {
    let block = Solid::cuboid(10.0, 10.0, 10.0)?;
    let drill = Solid::cylinder(2.0, 10.0)?;
    let bored = block.cut(&drill)?;

    bored.write_step(output)?;

    println!("wrote {} — volume {:.4}", output.display(), bored.volume()?);

    Ok(())
}

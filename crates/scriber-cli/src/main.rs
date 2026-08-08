#![forbid(unsafe_code)]

//! Headless entry point for Scriber.

// `unsafe` and C++ live only in scriber-occt, where the FFI boundary makes them
// unavoidable. This crate drives the kernel through the safe API, so `forbid`
// costs nothing and makes the rule a compile error rather than a convention.

mod backend;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use backend::{DocumentExports, KernelBackend};
use clap::{Parser, Subcommand};
use scriber_lang::diag::{Diagnostic, render};
use scriber_lang::{Value, evaluate, parse, print};

#[derive(Parser)]
#[command(name = "scriber", version, about = "Scriber CAD, headless")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Evaluate a document and run its `export` statements.
    Build {
        document: PathBuf,

        /// Let the document write outside its own directory.
        ///
        /// Without this, an `export` that resolves anywhere but the document's
        /// directory is refused rather than quietly redirected.
        #[arg(long)]
        allow_outside: bool,
    },

    /// Parse and type-check a document without producing geometry.
    ///
    /// Everything the language can decide without building is decided here, and
    /// that includes every dimension the kernel would refuse: a zero, negative,
    /// infinite or sub-tolerance extent fails `check` exactly as it fails
    /// `build`.
    ///
    /// One class of failure is left, and it is left because no amount of
    /// checking can reach it: a boolean operation that removes everything.
    /// `cut(small, large)` type-checks, and whether it leaves any solid behind
    /// is only knowable by asking the kernel to perform it. A document that
    /// passes `check` and fails `build` with "operation produced an empty
    /// result" is that case, and it is the only one.
    Check { document: PathBuf },

    /// Export one body, ignoring the document's own `export` statements.
    Export {
        document: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        body: Option<String>,
    },

    /// Reprint a document. With --check, fail if the output differs.
    Fmt {
        document: PathBuf,
        #[arg(long)]
        check: bool,
    },

    /// Print a body's volume.
    Volume {
        document: PathBuf,
        #[arg(long)]
        body: Option<String>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprint!("{failure}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Build {
            document,
            allow_outside,
        } => {
            let (source, dir) = read(&document)?;

            // Before anything is built, so that a refusal costs the user no
            // half-written export.
            if !allow_outside && let Err(refusals) = backend::confine_exports(&source, &dir) {
                // The document may also have errors of its own. Collect them
                // with a backend that builds nothing, so the user is told
                // everything at once rather than fixing an export path only to
                // be shown a typo on the next run.
                let mut diagnostics = refusals;
                let mut recording = scriber_lang::backend::RecordingBackend::default();
                if let Err(rest) = evaluate(&source, &dir, &mut recording) {
                    diagnostics.extend(rest);
                }
                diagnostics.sort_by_key(|diagnostic| diagnostic.range.start());

                return Err(report(&source, &document, &diagnostics));
            }

            let mut backend = KernelBackend::new(DocumentExports::Run);
            evaluate(&source, &dir, &mut backend)
                .map(|_| ())
                .map_err(|diagnostics| report(&source, &document, &diagnostics))
        }

        Command::Check { document } => {
            let (source, dir) = read(&document)?;
            let mut backend = scriber_lang::backend::RecordingBackend::default();
            evaluate(&source, &dir, &mut backend)
                .map(|_| ())
                .map_err(|diagnostics| report(&source, &document, &diagnostics))
        }

        Command::Export {
            document,
            output,
            body,
        } => {
            let (source, dir) = read(&document)?;
            // The document's own exports are skipped: this command was given a
            // path, and producing files it did not name would be a surprise.
            let mut backend = KernelBackend::new(DocumentExports::Skipped);
            let evaluated = evaluate(&source, &dir, &mut backend)
                .map_err(|diagnostics| report(&source, &document, &diagnostics))?;

            let chosen = pick_body(&evaluated.values, body.as_deref())?;
            backend.write(&chosen, &output)
        }

        Command::Fmt { document, check } => {
            let (source, _) = read(&document)?;
            let printed = print(&parse(&source).syntax());

            if check {
                if printed == source {
                    Ok(())
                } else {
                    Err(format!("{} is not formatted\n", document.display()))
                }
            } else {
                print!("{printed}");
                Ok(())
            }
        }

        Command::Volume { document, body } => {
            let (source, dir) = read(&document)?;
            // Asking what something weighs is not consent to overwrite last
            // week's export, so the document's `export` statements do not run.
            let mut backend = KernelBackend::new(DocumentExports::Skipped);
            let evaluated = evaluate(&source, &dir, &mut backend)
                .map_err(|diagnostics| report(&source, &document, &diagnostics))?;

            let chosen = pick_body(&evaluated.values, body.as_deref())?;
            let volume = chosen.volume().map_err(|error| error.to_string())?;
            println!("{volume:.4}");
            Ok(())
        }
    }
}

fn read(document: &Path) -> Result<(String, PathBuf), String> {
    let source = std::fs::read_to_string(document)
        .map_err(|error| format!("cannot read {}: {error}\n", document.display()))?;
    let dir = document.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok((source, dir))
}

fn report(source: &str, document: &Path, diagnostics: &[Diagnostic]) -> String {
    render(source, &document.display().to_string(), diagnostics)
}

/// The named body, or the only one if the document has exactly one.
fn pick_body(
    values: &[(String, Value<std::rc::Rc<scriber_kernel::Solid>>)],
    wanted: Option<&str>,
) -> Result<std::rc::Rc<scriber_kernel::Solid>, String> {
    let bodies: Vec<(&String, &std::rc::Rc<scriber_kernel::Solid>)> = values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Body(body) => Some((name, body)),
            Value::Quantity(_) => None,
        })
        .collect();

    match wanted {
        Some(name) => bodies
            .iter()
            .find(|(candidate, _)| candidate.as_str() == name)
            .map(|(_, body)| (*body).clone())
            .ok_or_else(|| format!("`{name}` is not a body in this document\n")),

        None if bodies.len() == 1 => Ok(bodies[0].1.clone()),

        None => Err(format!(
            "use --body to say which body — the document declares {}\n",
            if bodies.is_empty() {
                "none".to_string()
            } else {
                bodies
                    .iter()
                    .map(|(name, _)| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )),
    }
}

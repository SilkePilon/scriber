//! `scriber-lang`'s `Backend`, implemented over the real kernel, and the CLI's
//! policy on where a document is allowed to write.
//!
//! `Solid` is deliberately `!Send`/`!Sync` so it cannot escape the kernel
//! thread, which is why bodies are shared with `Rc` rather than `Arc`.

use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use scriber_kernel::Solid;
use scriber_lang::ast::{Document, Stmt};
use scriber_lang::backend::Backend;
use scriber_lang::diag::Diagnostic;
use scriber_lang::parse;

/// Whether a document's own `export` statements produce files.
pub enum DocumentExports {
    /// Run them. `build`.
    Run,

    /// Ignore them: this command names its own output, or produces none. A
    /// query must not have side effects, and `export -o` must not write files
    /// it was not asked for.
    Skipped,
}

/// Refuses, before any geometry is built, every `export` that would write
/// outside `base_dir`.
///
/// THE POLICY. `scriber-lang` resolves a document's `export` path against the
/// document's directory and hands it over unjudged: confinement is policy, and
/// the language deliberately holds none. The CLI decides, because the CLI is
/// where the user's consent lives — on the command line, not in a file they may
/// have been sent. Opening someone else's document must not be a way to have it
/// write over your files, and a document that wanted to could: the path is a
/// plain string, so `../../.ssh/config` is as easy to write as `part.step`.
///
/// The escape hatch is `--allow-outside`, which skips this check: CAD projects
/// legitimately organise output into `../out/`, so refusing outright would be
/// wrong. What is never done is rewriting the path to keep it inside — quietly
/// turning `../out/part.step` into `out/part.step` would leave the user hunting
/// for a file the document plainly says is elsewhere.
///
/// Checked up front rather than at the moment of writing so that a refusal
/// behaves like any other document error: every violation is reported at once,
/// and nothing lands on disk. Checking it inside [`Backend::export`] would let
/// a document whose second export escapes leave its first one behind.
pub fn confine_exports(source: &str, base_dir: &Path) -> Result<(), Vec<Diagnostic>> {
    let parsed = parse(source);
    let Some(document) = Document::cast(parsed.syntax()) else {
        // Not a document at all. `evaluate` says so far better than this can.
        return Ok(());
    };

    let base = resolve(base_dir);
    let mut refusals = Vec::new();

    for stmt in document.statements() {
        let Stmt::Export(export) = stmt else { continue };
        let Some(path_token) = export.path() else {
            continue;
        };

        // TWIN of the join in `scriber_lang::eval`: a document's export path
        // resolves against the document's own directory. If that ever changes
        // there, this must change with it, or the path checked here is not the
        // path written.
        let target = resolve(&base_dir.join(path_token.text().trim_matches('"')));

        if !target.starts_with(&base) {
            refusals.push(Diagnostic::error(
                format!(
                    "this document exports to {}, outside its own directory {} — \
                     pass --allow-outside to let it write there",
                    target.display(),
                    base.display()
                ),
                export.range(),
            ));
        }
    }

    if refusals.is_empty() {
        Ok(())
    } else {
        Err(refusals)
    }
}

/// Builds geometry with the real kernel.
pub struct KernelBackend {
    exports: DocumentExports,
}

impl KernelBackend {
    pub fn new(exports: DocumentExports) -> Self {
        Self { exports }
    }

    /// Writes `body` to `path`, format chosen by the extension.
    ///
    /// No policy is applied: this is for a path the user typed on the command
    /// line, where saying it *is* the consent.
    pub fn write(&self, body: &Rc<Solid>, path: &Path) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "step" | "stp" => body.write_step(path).map_err(|error| error.to_string()),
            "stl" => body.write_stl(path).map_err(|error| error.to_string()),
            other => Err(format!("cannot export `.{other}`")),
        }
    }
}

impl Backend for KernelBackend {
    type Body = Rc<Solid>;

    fn cuboid(&mut self, dx: f64, dy: f64, dz: f64) -> Result<Self::Body, String> {
        Solid::cuboid(dx, dy, dz)
            .map(Rc::new)
            .map_err(|error| error.to_string())
    }

    fn cylinder(&mut self, radius: f64, height: f64) -> Result<Self::Body, String> {
        Solid::cylinder(radius, height)
            .map(Rc::new)
            .map_err(|error| error.to_string())
    }

    fn cut(&mut self, target: &Self::Body, tool: &Self::Body) -> Result<Self::Body, String> {
        target
            .cut(tool)
            .map(Rc::new)
            .map_err(|error| error.to_string())
    }

    fn export(&mut self, body: &Self::Body, path: &Path) -> Result<(), String> {
        match self.exports {
            DocumentExports::Skipped => Ok(()),
            DocumentExports::Run => self.write(body, path),
        }
    }
}

/// Resolves `path` to an absolute, symlink-free location, without requiring it
/// to exist.
///
/// Comparing paths as written would be fooled by `sub/../../elsewhere`, and by
/// a symlink pointing out of the directory. `canonicalize` answers both but
/// needs the path to exist, and the file being exported does not exist yet. So
/// `..` is applied to the canonical form of what precedes it — which is how the
/// kernel resolves it — and only the final component is left unresolved.
fn resolve(path: &Path) -> PathBuf {
    // The common case, and the exact one: the base directory always exists, and
    // so does an export that overwrites a previous run's file.
    if let Ok(real) = path.canonicalize() {
        return real;
    }

    let absolute = match (path.is_absolute(), std::env::current_dir()) {
        (true, _) => path.to_path_buf(),
        (false, Ok(cwd)) => cwd.join(path),
        (false, Err(_)) => path.to_path_buf(),
    };

    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Follow symlinks before stepping out, or `link/..` would be
                // read as the directory `link` sits in rather than the one its
                // target sits in.
                if let Ok(real) = resolved.canonicalize() {
                    resolved = real;
                }
                if !resolved.pop() {
                    // Already at the root, where `..` is the root itself. Only
                    // reachable if the path never became absolute.
                    resolved.push(Component::ParentDir);
                }
            }
            other => resolved.push(other),
        }
    }

    // Resolve the directory the file would land in, keeping the file's own name
    // as written since nothing is there to resolve.
    let settled = match (resolved.parent(), resolved.file_name()) {
        (Some(parent), Some(name)) => parent.canonicalize().ok().map(|real| real.join(name)),
        _ => None,
    };

    settled.unwrap_or(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one place the language's extent rule and the kernel's can be
    /// compared, because this is the only crate that links both.
    ///
    /// `scriber-lang` deliberately does not depend on `scriber-kernel` — that
    /// is what keeps its thousands of property-test cases free of a C++
    /// toolchain — so `scriber_lang::eval::MIN_EXTENT` is a copy of the
    /// kernel's, and copies drift. This is the test that notices. Without the
    /// shared rule, `scriber check` accepted `cuboid(0, 1, 1)` and exited 0
    /// while `scriber build` on the same document refused it; without this
    /// test, they would be free to disagree again by one edit to either
    /// constant.
    #[test]
    fn the_languages_extent_rule_agrees_with_the_kernels() {
        use scriber_lang::eval::{MIN_EXTENT, is_buildable_extent};

        for value in [
            0.0,
            -0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e-12,
            MIN_EXTENT / 2.0,
            // The tolerance itself, where OCCT's own primitives disagree with
            // each other and where this test first earned its keep: the
            // language accepted it and the kernel did not.
            MIN_EXTENT,
            MIN_EXTENT * 1.001,
            MIN_EXTENT * 10.0,
            1.0,
            1000.0,
        ] {
            // Isolated in `dx`: the other two extents are plainly buildable, so
            // whatever the kernel decides, it decided it about this value.
            let kernel_accepts = Solid::cuboid(value, 1.0, 1.0).is_ok();

            assert_eq!(
                is_buildable_extent(value),
                kernel_accepts,
                "the language and the kernel disagree about {value}: \
                 `scriber check` and `scriber build` would give different answers"
            );
        }
    }

    /// `..` is not itself the thing being refused — leaving the directory is.
    #[test]
    fn resolve_applies_parent_components() {
        let root = std::env::temp_dir().join(format!("scriber_resolve_{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("fixture");

        let base = resolve(&nested);

        assert!(resolve(&nested.join("part.step")).starts_with(&base));
        assert!(resolve(&nested.join("sub").join("..").join("part.step")).starts_with(&base));
        assert!(!resolve(&nested.join("..").join("part.step")).starts_with(&base));
        assert!(!resolve(&nested.join("..").join("..").join("part.step")).starts_with(&base));

        // A sibling whose name merely starts with the base's: `starts_with` is
        // component-wise, so this must not be read as being inside.
        assert!(!resolve(&root.join("a").join("bb").join("part.step")).starts_with(&base));

        std::fs::remove_dir_all(&root).ok();
    }
}

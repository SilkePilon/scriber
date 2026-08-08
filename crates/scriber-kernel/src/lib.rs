#![forbid(unsafe_code)]

//! Safe geometry API over the OCCT bridge.
//!
//! Nothing outside this crate should touch `scriber_occt` directly.

// `unsafe` and C++ live only in scriber-occt, where the FFI boundary makes them
// unavoidable. This crate is the safe API over that boundary — being safe is its
// entire reason to exist — so `forbid` makes the rule a compile error rather
// than a convention, and unlike `deny` it cannot be lifted by an `allow` further
// down the tree. It is spelled per-crate here, matching scriber-lang and
// scriber-cli, rather than via `[workspace.lints]`: a workspace lint table is
// opt-in per package anyway (`lints.workspace = true`), so it would not have
// caught this crate's omission either, and it would add a second place to look
// when asking what a crate forbids.

mod error;

use std::{
    fmt,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
};

use cxx::UniquePtr;
use scriber_occt::ffi;

pub use error::Error;

/// A solid body.
///
/// Not `Send` or `Sync`: all kernel work is confined to one dedicated thread.
pub struct Solid {
    inner: UniquePtr<ffi::Shape>,
    _not_sync: PhantomData<Rc<()>>,
}

impl Solid {
    fn from_raw(inner: UniquePtr<ffi::Shape>) -> Self {
        Self {
            inner,
            _not_sync: PhantomData,
        }
    }

    /// An axis-aligned box with one corner at the origin.
    pub fn cuboid(dx: f64, dy: f64, dz: f64) -> Result<Self, Error> {
        check_extent("dx", dx)?;
        check_extent("dy", dy)?;
        check_extent("dz", dz)?;

        Ok(Self::from_raw(ffi::make_box(dx, dy, dz)?))
    }

    /// A cylinder whose axis runs along +Z with its base at the origin.
    pub fn cylinder(radius: f64, height: f64) -> Result<Self, Error> {
        check_extent("radius", radius)?;
        check_extent("height", height)?;

        Ok(Self::from_raw(ffi::make_cylinder(radius, height)?))
    }

    /// This solid with `tool` subtracted from it.
    pub fn cut(&self, tool: &Solid) -> Result<Self, Error> {
        let result = Self::from_raw(ffi::cut(&self.inner, &tool.inner)?);

        if result.volume()? <= 0.0 {
            return Err(Error::EmptyResult);
        }

        Ok(result)
    }

    /// Enclosed volume, in model units cubed.
    pub fn volume(&self) -> Result<f64, Error> {
        Ok(ffi::volume(&self.inner)?)
    }

    /// Writes this solid to `path` as a STEP file.
    pub fn write_step(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let as_str = writable_path(path, step_write_failed)?;

        // Keep OCCT's own message — it is the only clue about why a write failed.
        ffi::write_step(&self.inner, as_str)
            .map_err(|exception| step_write_failed(path.to_path_buf(), exception.what().to_owned()))
    }

    /// Writes this solid to `path` as ASCII STL.
    ///
    /// The shim triangulates before writing: OCCT does not mesh on demand, and
    /// an unmeshed shape produces a well-formed file containing zero facets.
    pub fn write_stl(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let as_str = writable_path(path, stl_write_failed)?;

        ffi::write_stl(&self.inner, as_str)
            .map_err(|exception| stl_write_failed(path.to_path_buf(), exception.what().to_owned()))
    }
}

fn step_write_failed(path: PathBuf, reason: String) -> Error {
    Error::StepWriteFailed { path, reason }
}

fn stl_write_failed(path: PathBuf, reason: String) -> Error {
    Error::StlWriteFailed { path, reason }
}

/// Checks that `path` is something OCCT can be handed, and returns it as the
/// `&str` the bridge takes.
///
/// `fail` is the caller's own error constructor, so a refusal names the format
/// the caller asked for rather than whichever of the two writers this check was
/// first written for.
fn writable_path(path: &Path, fail: fn(PathBuf, String) -> Error) -> Result<&str, Error> {
    let as_str = path
        .to_str()
        .ok_or_else(|| fail(path.to_path_buf(), "path is not valid UTF-8".to_owned()))?;

    // A NUL is legal in a Rust str but terminates a C string. The shim
    // rebuilds the path as std::string(data, size) and hands OCCT its
    // .c_str(), so without this check OCCT silently writes to the prefix
    // before the NUL and reports success — a caller asking for
    // "report\0.step" gets a file called "report" and no indication that
    // anything was substituted. Refusing is the only safe reading: we
    // cannot know which of the two paths was meant.
    if as_str.contains('\0') {
        return Err(fail(
            path.to_path_buf(),
            "path contains an interior NUL".to_owned(),
        ));
    }

    Ok(as_str)
}

/// Derive is impossible: `ffi::Shape` is opaque to Rust, so there is nothing to
/// print. This exists so `Solid` can appear in `Result`s that tests unwrap.
impl fmt::Debug for Solid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Solid").finish_non_exhaustive()
    }
}

/// The tolerance an extent must exceed.
///
/// Matches OCCT's `Precision::Confusion()`. At or below this the kernel treats
/// two points as coincident, so it will either build a "solid" it also
/// considers degenerate — returning Ok with an essentially zero volume — or
/// throw `Standard_DomainError` from deep inside a primitive builder. Rejecting
/// here is what keeps both out of the API.
pub const MIN_EXTENT: f64 = 1e-7;

/// Rejects dimensions OCCT would accept but should not, and dimensions it
/// refuses in a way the caller should not have to read a C++ exception to
/// understand.
///
/// The comparison is strict. `BRepPrimAPI_MakeBox` treats an extent of exactly
/// `Precision::Confusion()` as degenerate and throws, while
/// `BRepPrimAPI_MakeCylinder` accepts the same figure — so an inclusive bound
/// let the boundary itself through to two different fates depending on the
/// primitive, one of them an opaque `Standard_DomainError`. One rule, applied
/// before OCCT sees the number, is the only way the API can promise the same
/// answer for both.
fn check_extent(name: &'static str, value: f64) -> Result<(), Error> {
    if !is_buildable_extent(value) {
        return Err(Error::InvalidDimension { name, value });
    }

    Ok(())
}

/// Whether `value` is an extent this kernel will build at.
///
/// Public so that `scriber-lang`, which must not depend on this crate and
/// therefore keeps its own copy of the rule, can be tested against it rather
/// than trusted to match it.
pub fn is_buildable_extent(value: f64) -> bool {
    value.is_finite() && value > MIN_EXTENT
}

/// Returns this crate's own semantic version, as declared in `Cargo.toml`.
///
/// Nothing in the workspace consumes it yet — in particular `write_step` does
/// NOT stamp it into exported files. It exists so an embedder can report which
/// kernel build it is talking to.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        // Compare against the contract, not against today's number: pinning the
        // literal "0.1.0" made this test fail on every release bump without
        // catching a single real defect.
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));

        // The above alone would still pass if version() returned an empty or
        // otherwise unusable string, so pin the shape the name promises.
        let parts: Vec<&str> = version().split('.').collect();
        assert_eq!(parts.len(), 3, "not a semantic version: {}", version());
        assert!(
            parts.iter().all(|p| !p.is_empty()),
            "semantic version has an empty component: {}",
            version()
        );
    }

    #[test]
    fn cuboid_reports_its_volume() {
        let solid = Solid::cuboid(2.0, 3.0, 4.0).expect("cuboid builds");
        assert!((solid.volume().expect("volume computes") - 24.0).abs() < 1e-9);
    }

    #[test]
    fn cutting_reduces_volume() {
        let block = Solid::cuboid(10.0, 10.0, 10.0).expect("block builds");
        let drill = Solid::cylinder(2.0, 10.0).expect("cylinder builds");
        let bored = block.cut(&drill).expect("cut succeeds");

        // Pin the exact analytic value, not merely "smaller" — a cut that
        // silently did nothing would still be smaller than nothing at all.
        let expected = 1000.0 - std::f64::consts::PI * 2.0 * 2.0 * 10.0 / 4.0;
        let actual = bored.volume().expect("bored volume computes");
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn sub_tolerance_dimensions_are_rejected() {
        // Positive and finite, but below OCCT's confusion tolerance, so the
        // kernel would return Ok with geometry it considers degenerate.
        let err = Solid::cylinder(1e-12, 1.0).expect_err("must be rejected");
        assert!(
            matches!(err, Error::InvalidDimension { name: "radius", .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn the_tolerance_itself_is_rejected_by_both_primitives() {
        // OCCT does not agree with itself here: at exactly
        // `Precision::Confusion()`, `BRepPrimAPI_MakeCylinder` builds happily
        // while `BRepPrimAPI_MakeBox` throws `Standard_DomainError` — a bare
        // C++ exception surfacing as `Error::Kernel`, which tells a caller
        // nothing about which dimension was at fault. `check_extent` is strict
        // so that both report the same thing, and report it usefully.
        for (result, expected) in [
            (Solid::cuboid(MIN_EXTENT, 1.0, 1.0), "dx"),
            (Solid::cylinder(MIN_EXTENT, 1.0), "radius"),
            (Solid::cylinder(1.0, MIN_EXTENT), "height"),
        ] {
            let err = result.expect_err("the tolerance itself is not an extent");
            assert!(
                matches!(err, Error::InvalidDimension { name, .. } if name == expected),
                "got {err:?}"
            );
        }

        // And anything above it still builds, so the bound is a boundary and
        // not a blanket refusal of small parts.
        assert!(Solid::cuboid(MIN_EXTENT * 1.001, 1.0, 1.0).is_ok());
    }

    #[test]
    fn the_extent_rule_is_the_predicate_it_exposes() {
        // `is_buildable_extent` is what `scriber-cli` pins the language's copy
        // of the rule against, so it has to be the rule the kernel actually
        // applies rather than a second, agreeing-by-luck statement of it.
        for value in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            1e-12,
            MIN_EXTENT,
            MIN_EXTENT * 1.001,
            1.0,
        ] {
            assert_eq!(
                is_buildable_extent(value),
                Solid::cuboid(value, 1.0, 1.0).is_ok(),
                "disagreed about {value}"
            );
        }
    }

    #[test]
    fn cutting_away_everything_reports_an_empty_result() {
        let small = Solid::cuboid(1.0, 1.0, 1.0).expect("small builds");
        let large = Solid::cuboid(10.0, 10.0, 10.0).expect("large builds");

        let err = small.cut(&large).expect_err("must report an empty result");
        assert!(matches!(err, Error::EmptyResult), "got {err:?}");
    }

    #[test]
    fn degenerate_cuboid_is_rejected() {
        let err = Solid::cuboid(0.0, 1.0, 1.0).expect_err("must be rejected");
        assert!(
            matches!(err, Error::InvalidDimension { name: "dx", .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn negative_and_nan_cylinder_dimensions_are_rejected() {
        // OCCT accepts a zero radius and defers a negative height to a later
        // volume query, so these must be caught here rather than in the kernel.
        for (radius, height, expected) in [
            (-1.0, 10.0, "radius"),
            (0.0, 10.0, "radius"),
            (1.0, f64::NAN, "height"),
        ] {
            let Err(err) = Solid::cylinder(radius, height) else {
                panic!("must be rejected: r={radius} h={height}");
            };
            assert!(
                matches!(err, Error::InvalidDimension { name, .. } if name == expected),
                "got {err:?}"
            );
        }
    }

    #[test]
    fn step_export_writes_a_file() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let path =
            std::env::temp_dir().join(format!("scriber_kernel_export_{}.step", std::process::id()));
        std::fs::remove_file(&path).ok();

        solid.write_step(&path).expect("export succeeds");
        assert!(path.metadata().expect("file exists").len() > 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn step_export_rejects_a_path_containing_a_nul() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");

        // The prefix before the NUL is a writable path that would succeed on
        // its own, which is exactly what makes the old behaviour dangerous:
        // OCCT saw only the prefix, wrote there, and reported success.
        let prefix =
            std::env::temp_dir().join(format!("scriber_kernel_nul_{}.step", std::process::id()));
        std::fs::remove_file(&prefix).ok();
        assert!(!prefix.exists(), "could not clear {prefix:?}");

        let requested = format!("{}\u{0}ignored.step", prefix.display());
        let err = solid
            .write_step(&requested)
            .expect_err("a path with an interior NUL must be rejected");

        assert!(
            matches!(&err, Error::StepWriteFailed { reason, .. } if reason.contains("NUL")),
            "got {err:?}"
        );

        // The point of the fix: nothing was written to the truncated path.
        assert!(
            !prefix.exists(),
            "write_step truncated the path at the NUL and wrote to {prefix:?}"
        );

        std::fs::remove_file(&prefix).ok();
    }

    #[test]
    fn stl_export_writes_a_meshed_file() {
        let solid = Solid::cuboid(10.0, 10.0, 10.0).expect("cuboid builds");
        let path =
            std::env::temp_dir().join(format!("scriber_kernel_export_{}.stl", std::process::id()));
        std::fs::remove_file(&path).ok();

        solid.write_stl(&path).expect("export succeeds");

        let contents = std::fs::read_to_string(&path).expect("readable");
        assert_eq!(contents.matches("facet normal").count(), 12);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn stl_export_rejects_a_path_containing_a_nul() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");

        let prefix =
            std::env::temp_dir().join(format!("scriber_kernel_stl_nul_{}.stl", std::process::id()));
        std::fs::remove_file(&prefix).ok();
        assert!(!prefix.exists(), "could not clear {prefix:?}");

        let requested = format!("{}\u{0}ignored.stl", prefix.display());
        let err = solid
            .write_stl(&requested)
            .expect_err("a path with an interior NUL must be rejected");

        assert!(
            matches!(&err, Error::StlWriteFailed { reason, .. } if reason.contains("NUL")),
            "got {err:?}"
        );
        assert!(
            !prefix.exists(),
            "write_stl truncated the path at the NUL and wrote to {prefix:?}"
        );

        std::fs::remove_file(&prefix).ok();
    }

    #[test]
    fn step_export_reports_an_unwritable_path() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let err = solid
            .write_step("/nonexistent-directory/model.step")
            .expect_err("export must fail");
        assert!(matches!(err, Error::StepWriteFailed { .. }));
    }

    /// The message a user sees has to name the format they asked for. One
    /// variant served both writers, so an STL failure read "failed to write
    /// STEP file to model.stl" — an accurate path attached to the wrong noun.
    #[test]
    fn stl_export_reports_an_unwritable_path_as_an_stl_failure() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let err = solid
            .write_stl("/nonexistent-directory/model.stl")
            .expect_err("export must fail");

        assert!(matches!(err, Error::StlWriteFailed { .. }), "got {err:?}");

        let message = err.to_string();
        assert!(message.contains("STL file"), "{message}");
        assert!(!message.contains("STEP"), "{message}");
        assert!(message.contains("model.stl"), "{message}");
    }
}

//! Safe geometry API over the OCCT bridge.
//!
//! Nothing outside this crate should touch `scriber_occt` directly.

mod error;

use std::{fmt, marker::PhantomData, path::Path, rc::Rc};

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
        let as_str = path.to_str().ok_or_else(|| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: "path is not valid UTF-8".to_owned(),
        })?;

        // Keep OCCT's own message — it is the only clue about why a write failed.
        ffi::write_step(&self.inner, as_str).map_err(|exception| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: exception.what().to_owned(),
        })
    }
}

/// Derive is impossible: `ffi::Shape` is opaque to Rust, so there is nothing to
/// print. This exists so `Solid` can appear in `Result`s that tests unwrap.
impl fmt::Debug for Solid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Solid").finish_non_exhaustive()
    }
}

/// Smallest extent we accept.
///
/// Matches OCCT's `Precision::Confusion()`. Below this the kernel treats two
/// points as coincident, so it will happily build a "solid" it also considers
/// degenerate — returning Ok with an essentially zero volume rather than an
/// error. Rejecting here is what keeps that out of the API.
const MIN_EXTENT: f64 = 1e-7;

/// Rejects dimensions OCCT would accept but should not.
fn check_extent(name: &'static str, value: f64) -> Result<(), Error> {
    if !value.is_finite() || value < MIN_EXTENT {
        return Err(Error::InvalidDimension { name, value });
    }

    Ok(())
}

/// Returns the crate's semantic version, used to stamp exported files.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), "0.1.0");
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
    fn step_export_reports_an_unwritable_path() {
        let solid = Solid::cuboid(1.0, 1.0, 1.0).expect("cuboid builds");
        let err = solid
            .write_step("/nonexistent-directory/model.step")
            .expect_err("export must fail");
        assert!(matches!(err, Error::StepWriteFailed { .. }));
    }
}

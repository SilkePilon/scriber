use std::path::PathBuf;

/// Errors produced by geometry operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A dimension was not a finite positive number.
    ///
    /// OCCT does not reliably reject these — `make_cylinder(0.0, 1.0)` returns
    /// a valid-looking shape of zero volume, and a negative height only fails
    /// later during a volume query. We validate at the boundary instead.
    #[error("{name} must be a finite positive number, got {value}")]
    InvalidDimension { name: &'static str, value: f64 },

    /// OCCT raised a failure. Carries the kernel's own message.
    #[error("geometry kernel error: {0}")]
    Kernel(String),

    /// OCCT declined to write the STEP file.
    #[error("failed to write STEP file to {path}: {reason}")]
    StepWriteFailed { path: PathBuf, reason: String },

    /// A boolean operation produced no geometry.
    #[error("operation produced an empty result")]
    EmptyResult,
}

impl From<cxx::Exception> for Error {
    fn from(exception: cxx::Exception) -> Self {
        Error::Kernel(exception.what().to_owned())
    }
}

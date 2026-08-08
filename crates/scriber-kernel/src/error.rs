use std::path::PathBuf;

/// Errors produced by geometry operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A dimension was not a usable finite length.
    ///
    /// OCCT does not reliably reject these — `make_cylinder(0.0, 1.0)` returns
    /// a valid-looking shape of zero volume, a negative height only fails
    /// later during a volume query, and anything at or below OCCT's confusion
    /// tolerance yields geometry the kernel treats as degenerate, reporting
    /// either nothing at all or a bare `Standard_DomainError` depending on
    /// which primitive was asked. We validate at the boundary instead.
    #[error("{name} must be finite and greater than 1e-7, got {value}")]
    InvalidDimension { name: &'static str, value: f64 },

    /// OCCT raised a failure. Carries the kernel's own message.
    #[error("geometry kernel error: {0}")]
    Kernel(String),

    /// OCCT declined to write the STEP file.
    #[error("failed to write STEP file to {path}: {reason}")]
    StepWriteFailed { path: PathBuf, reason: String },

    /// OCCT declined to write the STL file.
    ///
    /// Split from [`Error::StepWriteFailed`] rather than shared with it: one
    /// variant covering both formats made an STL failure report "failed to
    /// write STEP file to part.stl", which is a lie about the operation the
    /// user asked for and sends them looking in the wrong place.
    #[error("failed to write STL file to {path}: {reason}")]
    StlWriteFailed { path: PathBuf, reason: String },

    /// A boolean operation produced no geometry.
    #[error("operation produced an empty result")]
    EmptyResult,
}

impl From<cxx::Exception> for Error {
    fn from(exception: cxx::Exception) -> Self {
        Error::Kernel(exception.what().to_owned())
    }
}

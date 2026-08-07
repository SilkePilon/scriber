//! The only place in Scriber where C++ is called.
//!
//! Every function here maps one-to-one onto a small C++ shim in `shim.cpp`
//! that calls OpenCASCADE. Nothing else in the workspace links OCCT.

#[cxx::bridge(namespace = "scriber")]
pub mod ffi {
    unsafe extern "C++" {
        include!("scriber-occt/src/shim.hpp");

        /// An OCCT `TopoDS_Shape`, owned by C++.
        type Shape;

        /// An axis-aligned box with one corner at the origin.
        fn make_box(dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;

        /// Enclosed volume of a solid, in model units cubed.
        fn volume(shape: &Shape) -> Result<f64>;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    #[test]
    fn box_has_expected_volume() {
        let shape = ffi::make_box(2.0, 3.0, 4.0).expect("box builds");
        let volume = ffi::volume(&shape).expect("volume computes");
        assert!(
            (volume - 24.0).abs() < 1e-9,
            "expected volume 24.0, got {volume}"
        );
    }

    #[test]
    fn degenerate_box_is_an_error_not_a_crash() {
        // OCCT raises Standard_DomainError here. If the shim's guard were
        // missing, this would abort the test process instead of returning Err.
        let result = ffi::make_box(0.0, 1.0, 1.0);
        assert!(result.is_err(), "expected a zero-width box to be rejected");
    }
}

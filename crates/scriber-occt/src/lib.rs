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

        /// A cylinder whose axis runs along +Z with its base at the origin.
        fn make_cylinder(radius: f64, height: f64) -> Result<UniquePtr<Shape>>;

        /// Boolean subtraction: `target` with `tool` removed.
        fn cut(target: &Shape, tool: &Shape) -> Result<UniquePtr<Shape>>;

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
        let error = ffi::make_box(0.0, 1.0, 1.0)
            .err()
            .expect("expected a zero-width box to be rejected");

        // Pin the class name too, so a regression to a generic message is caught.
        assert!(
            error.what().contains("Standard_DomainError"),
            "expected the OCCT exception class in the message, got: {}",
            error.what()
        );
    }

    #[test]
    fn cut_removes_the_tool_volume() {
        // A 10×10×10 block with a full-height radius-2 cylinder bored out.
        let block = ffi::make_box(10.0, 10.0, 10.0).expect("block builds");
        let drill = ffi::make_cylinder(2.0, 10.0).expect("cylinder builds");
        let bored = ffi::cut(&block, &drill).expect("cut succeeds");

        // The cylinder is centred on the origin corner, so exactly one
        // quarter of it lies inside the block.
        let quarter_cylinder = std::f64::consts::PI * 2.0 * 2.0 * 10.0 / 4.0;
        let expected = 1000.0 - quarter_cylinder;
        let actual = ffi::volume(&bored).expect("volume computes");

        assert!(
            (actual - expected).abs() < 1e-6,
            "expected volume {expected}, got {actual}"
        );
    }

    #[test]
    fn degenerate_cylinder_is_an_error_not_a_crash() {
        // Same contract as the degenerate box: the failure must surface as an
        // Err carrying the OCCT exception class, not abort the process.
        //
        // Note the brief's suggested input, radius 0.0, is NOT rejected by
        // OCCT 7.9.3 -- BRepPrimAPI_MakeCylinder happily returns a shape whose
        // volume is 0.0 (as do height 0.0 and a NaN radius). A negative radius
        // is what actually raises, as Standard_ConstructionError rather than
        // the box's Standard_DomainError. Rejecting the merely-degenerate
        // inputs is validation for the safe wrapper in Task 5, not for this
        // bridge; what is tested here is that the guard translates the raise.
        let error = ffi::make_cylinder(-1.0, 1.0)
            .err()
            .expect("expected a negative-radius cylinder to be rejected");

        assert!(
            error.what().contains("Standard_ConstructionError"),
            "expected the OCCT exception class in the message, got: {}",
            error.what()
        );
    }
}

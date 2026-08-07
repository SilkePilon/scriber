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
        fn make_box(dx: f64, dy: f64, dz: f64) -> UniquePtr<Shape>;

        /// Enclosed volume of a solid, in model units cubed.
        fn volume(shape: &Shape) -> f64;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    #[test]
    fn box_has_expected_volume() {
        let shape = ffi::make_box(2.0, 3.0, 4.0);
        let volume = ffi::volume(&shape);
        assert!(
            (volume - 24.0).abs() < 1e-9,
            "expected volume 24.0, got {volume}"
        );
    }
}

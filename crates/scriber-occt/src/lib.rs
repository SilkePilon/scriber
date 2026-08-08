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

        /// Writes `shape` to `path` as AP214 STEP.
        fn write_step(shape: &Shape, path: &str) -> Result<()>;

        /// Writes `shape` to `path` as ASCII STL, meshing it first.
        fn write_stl(shape: &Shape, path: &str) -> Result<()>;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    /// Serialises every test that enters OCCT.
    ///
    /// The harness runs these on one thread per core, but OCCT is not
    /// thread-safe and Scriber's design confines all kernel work to a single
    /// dedicated thread. `silence_kernel_console()` in `shim.hpp` is the
    /// sharpest edge: its function-local static makes the *initialisation*
    /// once-only, but the work it does is `RemovePrinters` on OCCT's global
    /// messenger, and a thread already inside `Message::DefaultMessenger()`
    /// can be reading that same printer list — an unsynchronised mutation of
    /// shared state, i.e. a data race, however rarely it bites.
    ///
    /// Stress-testing 32 threads through make/cut/volume did not make OCCT
    /// 7.9.3 misbehave, so this is insurance rather than a fix for an observed
    /// flake: it keeps the test suite honest to the threading model the rest
    /// of the codebase promises, and costs nothing (these tests are ~10ms).
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Takes the kernel lock, ignoring poisoning.
    ///
    /// A poisoned lock only means some earlier test's assertion failed, which
    /// says nothing about OCCT's state. Unwrapping would turn one real failure
    /// into a cascade of unrelated ones and bury the actual cause.
    fn lock_kernel() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn box_has_expected_volume() {
        let _kernel = lock_kernel();

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
        let _kernel = lock_kernel();

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
        let _kernel = lock_kernel();

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
        let _kernel = lock_kernel();

        let error = ffi::make_cylinder(-1.0, 1.0)
            .err()
            .expect("expected a negative-radius cylinder to be rejected");

        assert!(
            error.what().contains("Standard_ConstructionError"),
            "expected the OCCT exception class in the message, got: {}",
            error.what()
        );
    }

    #[test]
    fn write_step_produces_a_valid_header() {
        let _kernel = lock_kernel();

        let shape = ffi::make_box(1.0, 1.0, 1.0).expect("box builds");
        let path = std::env::temp_dir().join(format!(
            "scriber_occt_write_step_{}.step",
            std::process::id()
        ));
        let path_str = path.to_str().expect("temp path is valid UTF-8");

        // A leftover file from an earlier run would satisfy both assertions
        // below even if this write did nothing, so start from a clean slate.
        std::fs::remove_file(&path).ok();
        assert!(!path.exists(), "could not clear the stale file at {path:?}");

        ffi::write_step(&shape, path_str).expect("write_step succeeds");

        let contents = std::fs::read_to_string(&path).expect("STEP file is readable");
        assert!(
            contents.starts_with("ISO-10303-21;"),
            "file does not start with a STEP header: {:?}",
            &contents[..contents.len().min(40)]
        );
        assert!(
            contents.contains("END-ISO-10303-21;"),
            "STEP file is truncated"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn write_stl_produces_a_non_empty_solid() {
        // Same convention as every other test in this module: OCCT is not
        // thread-safe, so kernel tests serialize.
        let _kernel = lock_kernel();

        let shape = ffi::make_box(10.0, 10.0, 10.0).expect("box builds");
        let path =
            std::env::temp_dir().join(format!("scriber_occt_write_stl_{}.stl", std::process::id()));
        std::fs::remove_file(&path).ok();

        ffi::write_stl(&shape, path.to_str().expect("utf-8 path")).expect("write_stl succeeds");

        let contents = std::fs::read_to_string(&path).expect("STL is readable");
        assert!(
            contents.starts_with("solid"),
            "not an ASCII STL: {:?}",
            &contents[..20.min(contents.len())]
        );
        // A cube is 12 triangles. Zero facets would mean the shape was never
        // meshed, which is the failure mode this test exists to catch.
        assert_eq!(
            contents.matches("facet normal").count(),
            12,
            "wrong facet count"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unwritable_step_path_reports_the_status() {
        // Scope note: this is NOT an abort-prevention test. OCCT's STEP writer
        // never lets a Standard_Failure escape Transfer() or Write() -- it
        // catches the raise itself and reports IFSelect_RetStop, whose own
        // header glosses it as "indicates end or stop (such as Raise)". Every
        // failure reachable through this bridge lands there (missing parent, a
        // directory, an empty path, /dev/full, a read-only mount, a symlink
        // loop), and the degenerate shapes that are constructible here
        // (zero/NaN/infinite dimensions, the empty compound from cutting a box
        // with itself) all transfer and write successfully. So the shim raises
        // std::runtime_error, which cxx handles unaided; guard()'s
        // Standard_Failure clause is exercised by the primitive tests above,
        // not by this one.
        //
        // What this pins is the message: OCCT reports the useful detail to its
        // messenger rather than through the exception, so the shim has to fold
        // the status in itself. Task 5's Error::StepWriteFailed { path, reason }
        // gets the path from Rust, which already has it; the shim deliberately
        // leaves it out of the message to avoid printing it twice.
        let _kernel = lock_kernel();

        let shape = ffi::make_box(1.0, 1.0, 1.0).expect("box builds");
        let error = ffi::write_step(&shape, "/nonexistent-directory-scriber/model.step")
            .expect_err("expected a write into a missing directory to be rejected");

        let message = error.what();
        assert!(
            message.contains("IFSelect_RetStop"),
            "expected the IFSelect_RetStop status in the message, got: {message}"
        );
    }
}

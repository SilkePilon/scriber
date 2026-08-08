//! The seam between the language and geometry.
//!
//! `scriber-lang` never links OCCT. It states what it needs here, and
//! `scriber-cli` supplies an implementation over `scriber-kernel`. That keeps
//! the language's tests free of a C++ toolchain, which is what makes running
//! thousands of property-test cases practical.

use std::path::Path;

/// Geometry operations the language can call.
///
/// All lengths are millimetres: the dimension system has already run.
pub trait Backend {
    type Body: Clone;

    fn cuboid(&mut self, dx: f64, dy: f64, dz: f64) -> Result<Self::Body, String>;

    fn cylinder(&mut self, radius: f64, height: f64) -> Result<Self::Body, String>;

    fn cut(&mut self, target: &Self::Body, tool: &Self::Body) -> Result<Self::Body, String>;

    fn export(&mut self, body: &Self::Body, path: &Path) -> Result<(), String>;
}

/// Records calls instead of producing geometry.
///
/// Lets a test assert exactly which operations a document performed, which is
/// far sharper than comparing volumes, and needs no OCCT.
#[derive(Debug, Default)]
pub struct RecordingBackend {
    pub calls: Vec<String>,
}

impl RecordingBackend {
    /// Bodies are indices into `calls`, so a logged `#2` names the call that
    /// produced that body.
    fn record(&mut self, call: String) -> usize {
        self.calls.push(call);
        self.calls.len() - 1
    }
}

impl Backend for RecordingBackend {
    type Body = usize;

    fn cuboid(&mut self, dx: f64, dy: f64, dz: f64) -> Result<usize, String> {
        Ok(self.record(format!("cuboid({dx}, {dy}, {dz})")))
    }

    fn cylinder(&mut self, radius: f64, height: f64) -> Result<usize, String> {
        Ok(self.record(format!("cylinder({radius}, {height})")))
    }

    fn cut(&mut self, target: &usize, tool: &usize) -> Result<usize, String> {
        Ok(self.record(format!("cut(#{target}, #{tool})")))
    }

    fn export(&mut self, body: &usize, path: &Path) -> Result<(), String> {
        self.record(format!("export(#{body}, {})", path.display()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_recording_backend_logs_calls_in_order() {
        let mut backend = RecordingBackend::default();

        let a = backend.cuboid(1.0, 2.0, 3.0).unwrap();
        let b = backend.cylinder(4.0, 5.0).unwrap();
        let c = backend.cut(&a, &b).unwrap();
        backend.export(&c, Path::new("out.step")).unwrap();

        assert_eq!(
            backend.calls,
            vec![
                "cuboid(1, 2, 3)".to_string(),
                "cylinder(4, 5)".to_string(),
                "cut(#0, #1)".to_string(),
                "export(#2, out.step)".to_string(),
            ]
        );
    }
}

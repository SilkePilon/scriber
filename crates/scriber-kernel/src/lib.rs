//! Safe geometry API over the OCCT bridge.

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
}

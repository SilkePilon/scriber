use std::process::Command;

#[test]
fn smoke_command_writes_a_step_file() {
    let output_path =
        std::env::temp_dir().join(format!("scriber_cli_smoke_{}.step", std::process::id()));
    std::fs::remove_file(&output_path).ok();

    let output = Command::new(env!("CARGO_BIN_EXE_scriber"))
        .arg("smoke")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("binary runs");

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&output_path).expect("STEP file written");
    assert!(contents.starts_with("ISO-10303-21;"));
    // Read the geometry back out of the file rather than trusting the printed
    // volume, which comes from an in-process query. The cylindrical face only
    // exists if the cut actually landed in what was exported.
    assert!(
        contents.contains("CYLINDRICAL_SURFACE"),
        "exported solid has no cylindrical face, so the cut was not written"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Pin the value, not just the label: a boolean that silently did nothing
    // would print 1000.0000 and still contain the word "volume".
    assert!(stdout.contains("968.5841"), "stdout was: {stdout}");

    std::fs::remove_file(&output_path).ok();
}

#[test]
fn smoke_command_reports_an_unwritable_output_path() {
    // A directory that cannot exist, so the STEP export has nowhere to land.
    let output_path = std::env::temp_dir()
        .join("scriber_cli_missing_directory")
        .join("model.step");
    assert!(!output_path.exists(), "fixture path must not exist");

    let output = Command::new(env!("CARGO_BIN_EXE_scriber"))
        .arg("smoke")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("binary runs");

    assert!(
        !output.status.success(),
        "command unexpectedly succeeded, stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "stderr was: {stderr}");
    assert!(
        stderr.contains(&output_path.display().to_string()),
        "stderr must name the offending path, was: {stderr}"
    );
    assert!(!output_path.exists(), "nothing should have been written");
}

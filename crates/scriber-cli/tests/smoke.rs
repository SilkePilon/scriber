use std::process::Command;

#[test]
fn smoke_command_writes_a_step_file() {
    let output_path = std::env::temp_dir().join("scriber_cli_smoke.step");
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("volume"), "stdout was: {stdout}");

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

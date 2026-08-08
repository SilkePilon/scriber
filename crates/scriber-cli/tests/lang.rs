use std::process::Command;

fn scriber() -> Command {
    Command::new(env!("CARGO_BIN_EXE_scriber"))
}

fn write(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("scriber_lang_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("doc.scr");
    std::fs::write(&path, contents).expect("write document");
    path
}

/// A document one directory below the fixture root, so that `../` in an
/// `export` lands somewhere this test owns rather than in the temp directory
/// at large.
///
/// Unlike [`write`], this clears the fixture first: these tests assert that
/// files were *not* written, and a leftover from an earlier run under a reused
/// pid would make that assertion pass or fail for the wrong reason.
fn write_nested(name: &str, contents: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("scriber_lang_{}_{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();

    let dir = root.join("doc");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("doc.scr");
    std::fs::write(&path, contents).expect("write document");
    path
}

#[test]
fn build_runs_the_documents_exports() {
    let doc = write(
        "build",
        "units mm\n\
         param w = 60\n\
         body plate = cuboid(w, 40, 12)\n\
         body hole = cylinder(radius = 5, height = 12)\n\
         body part = cut(plate, hole)\n\
         export \"part.step\" from part\n\
         export \"part.stl\" from part\n",
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dir = doc.parent().unwrap();
    let step = std::fs::read_to_string(dir.join("part.step")).expect("STEP written");
    assert!(step.starts_with("ISO-10303-21;"));
    // Proves the cut actually happened rather than exporting the plate.
    assert!(step.contains("CYLINDRICAL_SURFACE"), "boolean did not run");

    let stl = std::fs::read_to_string(dir.join("part.stl")).expect("STL written");
    assert!(stl.contains("facet normal"), "STL has no facets");
}

#[test]
fn check_reports_errors_and_exits_non_zero() {
    let doc = write("check", "param a = nope\nparam b = alsonope\n");

    let output = scriber().arg("check").arg(&doc).output().expect("runs");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Both errors, not just the first.
    assert!(stderr.contains("nope"), "{stderr}");
    assert!(stderr.contains("alsonope"), "{stderr}");
}

#[test]
fn a_document_with_errors_writes_no_files() {
    let doc = write(
        "partial",
        "body a = cuboid(1,1,1)\nexport \"a.step\" from a\nparam bad = nope\n",
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(!output.status.success());
    assert!(
        !doc.parent().unwrap().join("a.step").exists(),
        "wrote a file anyway"
    );
}

// --------------------------------------------------------------------------
// `check` and `build` must agree.
//
// `check` exists to answer "is this document good?". It evaluates against
// `RecordingBackend`, which builds nothing, so any rule that lived only in the
// kernel was a rule `check` could not see — and a document it passed could
// still fail to build. The extent rule therefore lives in `scriber-lang`, in
// front of every backend. What is left over is documented in `check --help`.
// --------------------------------------------------------------------------

#[test]
fn check_refuses_a_degenerate_dimension_just_as_build_does() {
    // The exact reproduction from the review: `check` used to exit 0 on this
    // while `build` on the same document failed in the kernel.
    let doc = write_nested("degenerate", "body b = cuboid(0, 1, 1)\n");

    let checked = scriber().arg("check").arg(&doc).output().expect("runs");
    assert!(
        !checked.status.success(),
        "check passed a document build cannot build"
    );

    let built = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(!built.status.success());

    // Same message from both, so fixing what `check` reported is enough.
    let from_check = String::from_utf8_lossy(&checked.stderr).into_owned();
    let from_build = String::from_utf8_lossy(&built.stderr).into_owned();
    assert_eq!(from_check, from_build, "check and build disagree");
    assert!(from_check.contains("`dx`"), "{from_check}");
    assert!(from_check.contains("1e-7"), "{from_check}");
}

#[test]
fn build_reports_a_real_kernel_rejection_and_writes_no_file() {
    // The one failure `check` cannot reach, end to end through the real kernel:
    // cutting a large solid out of a small one type-checks and leaves nothing
    // behind, which only OCCT can discover. `check` passing here is the gap
    // `check --help` documents; `build` failing cleanly is what makes it
    // survivable.
    let doc = write_nested(
        "empty_cut",
        "body small = cuboid(1, 1, 1)\n\
         body large = cuboid(10, 10, 10)\n\
         body gone = cut(small, large)\n\
         export \"gone.step\" from gone\n",
    );

    let checked = scriber().arg("check").arg(&doc).output().expect("runs");
    assert!(
        checked.status.success(),
        "check now sees this — update `check --help`, which says it cannot: {}",
        String::from_utf8_lossy(&checked.stderr)
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(
        !output.status.success(),
        "an empty boolean result built successfully"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The kernel's own words, carried through the Backend seam unchanged, and
    // pointed at the statement that produced them.
    assert!(stderr.contains("empty result"), "{stderr}");
    assert!(stderr.contains("cut(small, large)"), "{stderr}");

    assert!(
        !doc.parent().unwrap().join("gone.step").exists(),
        "wrote a file for a build that failed"
    );
}

#[test]
fn check_help_admits_the_one_thing_it_cannot_check() {
    // The gap above is only survivable if a user is told about it, and `--help`
    // is where they would look. Pinned so the text cannot quietly drift away
    // from the behaviour.
    let output = scriber().arg("check").arg("--help").output().expect("runs");
    assert!(output.status.success());

    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("empty result"), "{help}");
    assert!(help.contains("cut("), "{help}");
}

#[test]
fn fmt_check_accepts_an_unchanged_document() {
    let doc = write("fmt", "units mm\n\n# note\nparam x = 1\n");

    let output = scriber()
        .arg("fmt")
        .arg("--check")
        .arg(&doc)
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn volume_reports_the_bored_volume() {
    let doc = write(
        "volume",
        "body plate = cuboid(10, 10, 10)\n\
         body hole = cylinder(radius = 2, height = 10)\n\
         body part = cut(plate, hole)\n",
    );

    let output = scriber()
        .arg("volume")
        .arg(&doc)
        .arg("--body")
        .arg("part")
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("968.5841"), "stdout was: {stdout}");
}

// --------------------------------------------------------------------------
// Where a document is allowed to write.
//
// `scriber-lang` joins an `export` path onto the document's directory and stops
// there — deliberately, because confinement is policy and the language holds
// none. The CLI's policy: a document writes inside its own directory, and
// anywhere else only with `--allow-outside` on the command line. Opening a
// document someone sent you must not be a way to have it write over your files.
// --------------------------------------------------------------------------

#[test]
fn an_export_that_escapes_the_document_directory_is_refused() {
    let doc = write_nested(
        "escape",
        "body a = cuboid(1, 1, 1)\nexport \"../escaped.step\" from a\nparam bad = nope\n",
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(
        !output.status.success(),
        "escaping export was allowed: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let escaped = doc.parent().unwrap().parent().unwrap().join("escaped.step");
    assert!(!escaped.exists(), "wrote outside the document's directory");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The refusal has to say what to do about it, or the user's only recourse
    // is to edit a document they may not own.
    assert!(stderr.contains("--allow-outside"), "{stderr}");
    assert!(stderr.contains("escaped.step"), "{stderr}");
    // And it must not swallow the document's own errors: refusing early is not
    // a reason to make the user discover the rest one run at a time.
    assert!(stderr.contains("nope"), "{stderr}");
}

#[test]
fn an_absolute_export_path_outside_the_document_directory_is_refused() {
    let elsewhere = std::env::temp_dir().join(format!(
        "scriber_lang_{}_absolute_target.step",
        std::process::id()
    ));
    std::fs::remove_file(&elsewhere).ok();

    let doc = write_nested(
        "absolute",
        &format!(
            "body a = cuboid(1, 1, 1)\nexport \"{}\" from a\n",
            elsewhere.display()
        ),
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(
        !output.status.success(),
        "absolute export was allowed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!elsewhere.exists(), "wrote to an absolute path");
}

#[test]
fn a_refused_export_costs_the_documents_other_exports_too() {
    // The refusal is decided before anything is built, so the export written
    // above the offending one must not survive. Checking confinement at the
    // moment of writing instead would leave `first.step` on disk next to a
    // failed build — an artefact that looks fresh and is not.
    let doc = write_nested(
        "refused",
        "body a = cuboid(1, 1, 1)\n\
         export \"first.step\" from a\n\
         export \"../escaped.step\" from a\n\
         export \"last.step\" from a\n",
    );

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(!output.status.success());

    let dir = doc.parent().unwrap();
    assert!(!dir.join("first.step").exists(), "wrote before refusing");
    assert!(!dir.join("last.step").exists(), "wrote after refusing");
    assert!(!dir.parent().unwrap().join("escaped.step").exists());
}

#[test]
fn a_relative_export_that_stays_inside_is_allowed() {
    // `..` is not what is refused — leaving the directory is. A path that walks
    // out and back in resolves to somewhere the document may write, so it must
    // be accepted, which is what makes this a resolution check rather than a
    // search for two dots.
    let doc = write_nested(
        "inside",
        "body a = cuboid(1, 1, 1)\nbody sub = cuboid(1, 1, 1)\nexport \"nested/../kept.step\" from a\n",
    );
    std::fs::create_dir_all(doc.parent().unwrap().join("nested")).expect("subdirectory");

    let output = scriber().arg("build").arg(&doc).output().expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(doc.parent().unwrap().join("kept.step").exists());
}

#[test]
fn allow_outside_permits_an_escaping_export() {
    let doc = write_nested(
        "allowed",
        "body a = cuboid(1, 1, 1)\nexport \"../escaped.step\" from a\n",
    );

    let output = scriber()
        .arg("build")
        .arg("--allow-outside")
        .arg(&doc)
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let escaped = doc.parent().unwrap().parent().unwrap().join("escaped.step");
    let step = std::fs::read_to_string(&escaped).expect("STEP written outside");
    assert!(step.starts_with("ISO-10303-21;"));
}

#[test]
fn export_writes_only_the_output_it_was_given() {
    // `export` names its own output, so the document's own `export` statements
    // are not run: asking for one file must not produce three.
    let doc = write_nested(
        "explicit",
        "body a = cuboid(1, 1, 1)\nexport \"unwanted.step\" from a\n",
    );
    let wanted = doc.parent().unwrap().join("wanted.step");

    let output = scriber()
        .arg("export")
        .arg(&doc)
        .arg("--output")
        .arg(&wanted)
        .arg("--body")
        .arg("a")
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let step = std::fs::read_to_string(&wanted).expect("STEP written");
    assert!(step.starts_with("ISO-10303-21;"));
    assert!(
        !doc.parent().unwrap().join("unwanted.step").exists(),
        "ran the document's own export as well"
    );
}

#[test]
fn volume_writes_no_files() {
    // A query must not have side effects: asking what something weighs is not
    // consent to overwrite last week's export.
    let doc = write_nested(
        "query",
        "body a = cuboid(10, 10, 10)\nexport \"unwanted.step\" from a\n",
    );

    let output = scriber().arg("volume").arg(&doc).output().expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !doc.parent().unwrap().join("unwanted.step").exists(),
        "a volume query wrote a file"
    );
}

//! Snapshots of rendered errors, so message quality is version-controlled.
//!
//! Review a changed snapshot as carefully as changed code: `cargo insta review`.

use scriber_lang::backend::RecordingBackend;
use scriber_lang::diag::render;
use scriber_lang::evaluate;

fn rendered(source: &str) -> String {
    let mut backend = RecordingBackend::default();
    match evaluate(source, std::path::Path::new("/tmp"), &mut backend) {
        Ok(_) => panic!("expected this document to fail"),
        Err(diagnostics) => render(source, "doc.scr", &diagnostics),
    }
}

#[test]
fn undeclared_name() {
    insta::assert_snapshot!(rendered("param a = missing\n"));
}

#[test]
fn duplicate_declaration() {
    insta::assert_snapshot!(rendered("param a = 1\nparam a = 2\n"));
}

#[test]
fn dimension_mismatch() {
    insta::assert_snapshot!(rendered("param a = 1mm + 45deg\n"));
}

#[test]
fn multiplying_two_lengths() {
    insta::assert_snapshot!(rendered("param a = 1mm * 2mm\n"));
}

#[test]
fn wrong_arity() {
    insta::assert_snapshot!(rendered("body b = cuboid(1, 2)\n"));
}

#[test]
fn wrong_arity_in_an_unclosed_call() {
    // The call is missing its `)`, so both errors are drawn from spans that
    // end where the line does. Neither caret may reach line 2: the second
    // statement is well-formed, and pointing at it sends the reader hunting
    // for a mistake that is not there.
    insta::assert_snapshot!(rendered("body b = cuboid(1, 2\nbody b2 = cuboid(1,2,3)\n"));
}

#[test]
fn a_degenerate_extent() {
    // Reported by the language, in front of every backend, so `check` says this
    // and `build` says this. The parameter is named because `cuboid` alone does
    // not tell the user which of the three arguments is the zero.
    insta::assert_snapshot!(rendered("body b = cuboid(1, 0, 1)\n"));
}

#[test]
fn several_errors_at_once() {
    insta::assert_snapshot!(rendered("param a = nope\nparam b = alsonope\nbody c = 5\n"));
}

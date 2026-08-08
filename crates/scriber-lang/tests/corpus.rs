//! Every document in `tests/corpus` must parse, round-trip and evaluate.
//!
//! Anything that breaks becomes a permanent regression test by being added
//! here as a file.

use scriber_lang::backend::RecordingBackend;
use scriber_lang::{evaluate, parse, print};

#[test]
fn every_corpus_document_round_trips_and_evaluates() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("corpus directory exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("scr") {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("readable document");
        let parsed = parse(&source);

        assert!(
            parsed.errors.is_empty(),
            "{}: {:?}",
            path.display(),
            parsed.errors
        );
        assert_eq!(
            print(&parsed.syntax()),
            source,
            "{} does not round-trip",
            path.display()
        );

        let mut backend = RecordingBackend::default();
        if let Err(diagnostics) = evaluate(&source, &dir, &mut backend) {
            let messages: Vec<String> = diagnostics.into_iter().map(|d| d.message).collect();
            panic!("{} failed to evaluate: {messages:?}", path.display());
        }

        checked += 1;
    }

    assert!(
        checked >= 2,
        "expected at least two corpus documents, found {checked}"
    );
}

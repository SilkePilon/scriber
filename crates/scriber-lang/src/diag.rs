//! Diagnostics and their rendering.
//!
//! The same type serves the CLI now and the Script view in a later milestone,
//! so it carries spans rather than pre-rendered text.

use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};
use rowan::TextRange;

use crate::parser::SyntaxError;

/// One problem with a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange,
    /// A secondary span, such as the earlier declaration of a duplicate name.
    pub note: Option<(String, TextRange)>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, range: TextRange) -> Self {
        Self {
            message: message.into(),
            range,
            note: None,
        }
    }

    pub fn with_note(mut self, message: impl Into<String>, range: TextRange) -> Self {
        self.note = Some((message.into(), range));
        self
    }
}

impl From<SyntaxError> for Diagnostic {
    fn from(error: SyntaxError) -> Self {
        Diagnostic::error(error.message, error.range)
    }
}

/// Renders diagnostics with source excerpts and caret spans.
pub fn render(source: &str, path: &str, diagnostics: &[Diagnostic]) -> String {
    let file = SimpleFile::new(path, source);
    let config = Config::default();
    let mut buffer = String::new();

    for diagnostic in diagnostics {
        let mut labels = vec![Label::primary((), to_span(source, diagnostic.range))];
        if let Some((message, range)) = &diagnostic.note {
            labels.push(Label::secondary((), to_span(source, *range)).with_message(message));
        }

        let rendered = CsDiagnostic::error()
            .with_message(&diagnostic.message)
            .with_labels(labels);

        // Rendering is best effort: a span that does not fit the source must not
        // cost the user the other diagnostics, and must never take down the CLI.
        if term::emit_to_string(&mut buffer, &config, &file, &rendered).is_err() {
            buffer.push_str("error: ");
            buffer.push_str(&diagnostic.message);
            buffer.push('\n');
        }
    }

    buffer
}

/// Converts a syntax-tree range into a byte span the renderer can slice.
///
/// Spans reach the renderer from a parser that recovers from anything, so they
/// are clamped to the source and snapped outwards to character boundaries: a
/// span pointing past the end of the file, or into the middle of a multi-byte
/// character, must still render rather than panic.
fn to_span(source: &str, range: TextRange) -> std::ops::Range<usize> {
    // `TextRange` guarantees `start <= end`, and clamping preserves that, so
    // the result is never inverted.
    let start = floor_boundary(source, usize::from(range.start()));
    let end = ceil_boundary(source, usize::from(range.end()));
    start..end
}

fn floor_boundary(source: &str, mut index: usize) -> usize {
    index = index.min(source.len());
    while !source.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(source: &str, mut index: usize) -> usize {
    index = index.min(source.len());
    while !source.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::{TextRange, TextSize};

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::new(start), TextSize::new(end))
    }

    #[test]
    fn renders_a_diagnostic_with_source_context() {
        let source = "param x = 1\nparam x = 2\n";
        let diagnostic = Diagnostic::error("`x` is already declared", range(18, 19))
            .with_note("first declared here", range(6, 7));

        let rendered = render(source, "doc.scr", &[diagnostic]);

        assert!(rendered.contains("`x` is already declared"), "{rendered}");
        assert!(rendered.contains("first declared here"), "{rendered}");
        assert!(rendered.contains("doc.scr"), "{rendered}");
    }

    #[test]
    fn renders_every_diagnostic_not_just_the_first() {
        let source = "param = 1\nparam = 2\n";
        let rendered = render(
            source,
            "doc.scr",
            &[
                Diagnostic::error("first problem", range(6, 7)),
                Diagnostic::error("second problem", range(16, 17)),
            ],
        );

        assert!(rendered.contains("first problem"), "{rendered}");
        assert!(rendered.contains("second problem"), "{rendered}");
    }

    /// Every span here breaks a naive renderer in some way. None of them may
    /// panic: a malformed document is exactly when diagnostics are needed, and
    /// a panic in rendering would take the CLI down instead of reporting.
    #[test]
    fn awkward_spans_still_render() {
        let source = "param x = 1\nparam ø = 2\n";
        let cases: &[(&str, TextRange)] = &[
            ("at the very start", range(0, 1)),
            ("empty at the very start", range(0, 0)),
            ("at the very end", range(23, 24)),
            ("empty at the very end", range(24, 24)),
            ("over a multi-byte character", range(18, 20)),
            ("split inside a multi-byte character", range(19, 20)),
            ("a whole line including its newline", range(0, 12)),
            ("past the end of the file", range(80, 90)),
        ];
        // An inverted range is not a case: `TextRange::new` asserts
        // `start <= end`, so one cannot be built to hand to `render`.

        for (name, span) in cases {
            let rendered = render(source, "doc.scr", &[Diagnostic::error(*name, *span)]);
            assert!(rendered.contains(name), "{name}: {rendered}");
            // Clamping is what earns this: handed a span past the end of the
            // file, the renderer draws a caret at the end rather than a bare
            // excerpt with nothing marked.
            assert!(rendered.contains('^'), "{name} marked nothing: {rendered}");
        }
    }

    #[test]
    fn a_note_after_the_primary_span_still_renders() {
        let source = "param x = 1\nparam x = 2\n";
        let diagnostic = Diagnostic::error("declared out of order", range(6, 7))
            .with_note("and again over here", range(18, 19));

        let rendered = render(source, "doc.scr", &[diagnostic]);

        assert!(rendered.contains("declared out of order"), "{rendered}");
        assert!(rendered.contains("and again over here"), "{rendered}");
    }

    #[test]
    fn a_syntax_error_becomes_a_diagnostic() {
        let parsed = crate::parse("cuboid(");
        let errors = &parsed.errors;
        assert!(
            !errors.is_empty(),
            "expected the parser to report something"
        );

        let diagnostics: Vec<Diagnostic> = errors.iter().cloned().map(Diagnostic::from).collect();
        assert_eq!(diagnostics[0].message, errors[0].message);
        assert_eq!(diagnostics[0].range, errors[0].range);

        let rendered = render("cuboid(", "doc.scr", &diagnostics);
        assert!(rendered.contains(&diagnostics[0].message), "{rendered}");
    }
}

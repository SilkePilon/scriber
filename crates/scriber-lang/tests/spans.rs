//! The invariant behind every span: a node ends at the last character its
//! author actually wrote.
//!
//! A node's range becomes a diagnostic's caret. When the parser eats trivia
//! while looking for a token that turns out not to be there, that trivia lands
//! inside whatever node is about to close, and the node's range runs on past
//! the text it describes — at the end of a line, onto the *next* statement,
//! which has nothing wrong with it.
//!
//! Three separate instances of that bug were found by reading rather than by a
//! test, because the round-trip property cannot see it: `print(parse(x)) == x`
//! is insensitive to *which* node owns a piece of trivia, only to whether some
//! node does. This is the test that can see it, run over every corpus document
//! and a spread of deliberately broken input.

use proptest::prelude::*;
use scriber_lang::syntax::SyntaxKind;
use scriber_lang::{parse, print};

/// Reports every node whose last token is trivia.
///
/// The message names the node and its range, and quotes the trailing trivia,
/// so the next instance is diagnosable from the failure alone rather than by
/// re-deriving it from a tree dump.
fn violations(source: &str) -> Vec<String> {
    let root = parse(source).syntax();
    let mut found = Vec::new();

    for node in root.descendants() {
        // The root spans the whole file by construction — that is what makes
        // printing it reproduce the source — so its final newline is not a
        // violation. Every node inside it is.
        if node.kind() == SyntaxKind::Document {
            continue;
        }

        let Some(last) = node.last_token() else {
            // An empty node owns no text and so cannot end inside trivia.
            continue;
        };

        if last.kind().is_trivia() {
            found.push(format!(
                "{:?}@{:?} ends in {:?} {:?} — its range runs past the last character written",
                node.kind(),
                node.text_range(),
                last.kind(),
                last.text().to_string(),
            ));
        }
    }

    found
}

/// Every node in every one of these must end on a token the user typed.
fn check(source: &str) {
    let found = violations(source);
    assert!(
        found.is_empty(),
        "source: {source:?}\n  {}",
        found.join("\n  ")
    );

    // Losslessness is what makes the invariant non-trivial: a parser could
    // satisfy the check above by dropping the trivia instead of relocating it.
    assert_eq!(print(&parse(source).syntax()), source, "source: {source:?}");
}

#[test]
fn no_corpus_node_ends_in_trivia() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("corpus directory exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("scr") {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("readable document");
        let found = violations(&source);
        assert!(
            found.is_empty(),
            "{}:\n  {}",
            path.display(),
            found.join("\n  ")
        );

        checked += 1;
    }

    assert!(checked >= 2, "expected at least two corpus documents");
}

#[test]
fn no_node_of_a_well_formed_document_ends_in_trivia() {
    for source in [
        "",
        "\n",
        "units mm\n",
        "param x = 1\n",
        "param x = 1mm + 2mm\n",
        "param x = -(3 + 4) / 2\n",
        "body b = cuboid(1, 2, 3)\n",
        "body b = cylinder(radius = 2, height = 10,)\n",
        "body b = cut(cuboid(9, 9, 9), cylinder(1, 9))\n",
        "body a = cuboid(1,1,1)\nexport \"a.step\" from a\n",
        "units mm\n\n# a note\n\nparam w = 60  # trailing\n\n\n",
        "param x = 1   \n   param y = 2   \n",
        "param x = (  1  +  2  )  \n",
        "body b = cuboid( 1 , 2 , 3 )\n",
    ] {
        check(source);
    }
}

#[test]
fn no_node_of_a_malformed_document_ends_in_trivia() {
    // Every one of these is a token short somewhere, which is exactly when the
    // parser is tempted to eat trivia looking for something that is not there.
    // Each is written with a following statement so that a node overrunning its
    // line is caught rather than merely reaching the end of the file.
    for source in [
        // Unclosed calls.
        "body b = cuboid(1, 2\nbody b2 = cuboid(1,2,3)\n",
        "body b = cuboid(\nparam y = 2\n",
        "body b = cuboid(1,\nparam y = 2\n",
        "body b = f(x =\nparam y = 2\n",
        "body b = cut(cuboid(1,1,1), cylinder(1\nparam y = 2\n",
        // Unclosed parens.
        "param x = (1 + 2\nparam y = 2\n",
        "param x = (\nparam y = 2\n",
        "param x = ((1)\nparam y = 2\n",
        "param x = (1 + \nparam y = 2\n",
        // Missing `=`.
        "param x\n\nbody y = 2\n",
        "param x 1\nparam y = 2\n",
        "body b\nparam y = 2\n",
        // Missing operands.
        "param x = 1 +\nparam y = 2\n",
        "param x = 1 + \nparam y = 2\n",
        "param x = *\nparam y = 2\n",
        "param x = -\nparam y = 2\n",
        // A stray quote, which the lexer can only call an error token.
        "param x = \"\nparam y = 2\n",
        "param x = \"unterminated\nparam y = 2\n",
        "export \"a.step\nbody a = cuboid(1,1,1)\n",
        // Missing names and keywords.
        "param = 1\nparam y = 2\n",
        "units\nparam y = 2\n",
        "units \nparam y = 2\n",
        "export\nparam y = 2\n",
        "export \"a.step\" from\nparam y = 2\n",
        "export \"a.step\" from \nparam y = 2\n",
        "!!!\nparam y = 2\n",
        ")))\nparam y = 2\n",
        // Trailing trivia with nothing after it: the file ends mid-statement,
        // so there is no next line to run onto and the only thing a node can
        // overrun is the end of what was typed.
        "param x = 1 +   ",
        "param x =   ",
        "param x   ",
        "body b = cuboid(1,   ",
        "units   ",
        "export   ",
        "!!!   ",
        "param x = 1  # note",
        "body b = cuboid(1, 2  # note\nparam y = 2\n",
    ] {
        check(source);
    }
}

/// Fragments assembled into documents, valid and invalid alike.
///
/// A twin of the generator in `tests/roundtrip.rs`, weighted towards the
/// half-typed statements that make the parser go looking for a token that is
/// not there — which is the only way this bug is ever produced.
fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("units mm".to_string()),
        Just("param x = 1".to_string()),
        Just("param y = 2.5mm".to_string()),
        Just("param z = x + y * 2".to_string()),
        Just("body b = cuboid(1, 2, 3)".to_string()),
        Just("body c = cylinder(radius = 2, height = 10,)".to_string()),
        Just("export \"out.step\" from d".to_string()),
        Just("# a comment".to_string()),
        // Each of these is a token short somewhere.
        Just("param = 1".to_string()),
        Just("param x".to_string()),
        Just("param x =".to_string()),
        Just("param x = 1 +".to_string()),
        Just("param x = (1 + 2".to_string()),
        Just("body e = cuboid(".to_string()),
        Just("body e = cuboid(1,".to_string()),
        Just("body e = f(x =".to_string()),
        Just("body e = f(x = 1".to_string()),
        Just("units".to_string()),
        Just("export".to_string()),
        Just("export \"a.step\" from".to_string()),
        Just("param x = \"".to_string()),
        Just("!!!".to_string()),
    ]
}

/// Separators a careless parser would attach to the node before them.
fn separator() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("  \n".to_string()),
        Just("\t\n".to_string()),
        Just(" # trailing\n".to_string()),
        Just("   ".to_string()),
        Just(" ".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// The same invariant, over documents nobody thought to write down.
    #[test]
    fn no_generated_document_has_a_node_ending_in_trivia(
        parts in prop::collection::vec((fragment(), separator()), 0..8),
        leading in separator(),
    ) {
        let mut source = leading;
        for (fragment, separator) in parts {
            source.push_str(&fragment);
            source.push_str(&separator);
        }

        let found = violations(&source);
        prop_assert!(found.is_empty(), "source: {:?}\n  {}", source, found.join("\n  "));
        prop_assert_eq!(print(&parse(&source).syntax()), source);
    }

    /// And over structural noise, which is what a half-typed document looks
    /// like before it looks like anything.
    #[test]
    fn no_node_of_arbitrary_text_ends_in_trivia(
        source in r#"[a-z_0-9(),=+*/#"' \t\n.-]{0,80}"#,
    ) {
        let found = violations(&source);
        prop_assert!(found.is_empty(), "source: {:?}\n  {}", source, found.join("\n  "));
    }
}

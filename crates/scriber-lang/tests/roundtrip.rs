//! The property the whole design rests on: parsing and reprinting any input
//! reproduces it byte for byte.
//!
//! The generator deliberately produces the things a naive printer destroys —
//! comments, blank runs, odd spacing, trailing commas — because a generator
//! that only emits tidy input proves nothing.

use proptest::prelude::*;
use scriber_lang::{parse, print};

/// Fragments assembled into documents, valid and invalid alike.
fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("units mm".to_string()),
        Just("param x = 1".to_string()),
        Just("param y = 2.5mm".to_string()),
        Just("param z = x + y * 2".to_string()),
        Just("body b = cuboid(1, 2, 3)".to_string()),
        Just("body c = cylinder(radius = 2, height = 10,)".to_string()),
        Just("body d = cut(b, c)".to_string()),
        Just("export \"out.step\" from d".to_string()),
        Just("# a comment".to_string()),
        // Malformed on purpose: recovery must stay lossless too.
        Just("param = 1".to_string()),
        Just("body e = cuboid(".to_string()),
        Just("!!!".to_string()),
        Just("param q = -(3 + 4) / 2".to_string()),
    ]
}

/// Separators that a careless printer would normalise away.
fn separator() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("\n\n\n".to_string()),
        Just("  \n".to_string()),
        Just("\t\n".to_string()),
        Just(" # trailing\n".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn printing_a_parsed_document_reproduces_it(
        parts in prop::collection::vec((fragment(), separator()), 0..12),
        leading in separator(),
    ) {
        let mut source = leading;
        for (fragment, separator) in parts {
            source.push_str(&fragment);
            source.push_str(&separator);
        }

        let printed = print(&parse(&source).syntax());
        prop_assert_eq!(printed, source);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// The generator above assembles known-shaped fragments. This one throws
    /// arbitrary structural noise at the parser instead — half-typed documents
    /// look like this, and the property must hold for them too.
    #[test]
    fn printing_arbitrary_text_reproduces_it(
        source in r#"[a-z_0-9(),=+*/#"' \t\n.-]{0,80}"#,
    ) {
        let printed = print(&parse(&source).syntax());
        prop_assert_eq!(printed, source);
    }

    /// Anything at all, including non-ASCII the lexer can only call `Error`.
    #[test]
    fn printing_any_unicode_reproduces_it(source in ".{0,40}") {
        let printed = print(&parse(&source).syntax());
        prop_assert_eq!(printed, source);
    }
}

/// Inputs that could make a recursive-descent parser recurse without bound.
/// A stack overflow aborts the process, so these assert termination as much
/// as losslessness.
#[test]
fn pathological_nesting_terminates_and_round_trips() {
    for source in [
        format!("param x = {}", "(".repeat(100_000)),
        format!("param x = {}1{}", "(".repeat(50_000), ")".repeat(50_000)),
        format!("param x = {}1", "-".repeat(100_000)),
        format!("body b = {}1{}", "f(".repeat(50_000), ")".repeat(50_000)),
        format!("param x = {}1", "1 + ".repeat(100_000)),
        format!("body b = f({}1)", "1, ".repeat(100_000)),
        format!("{}\n", "!".repeat(100_000)),
    ] {
        assert_eq!(print(&parse(&source).syntax()), source);
    }
}

/// Lexer quirks from task 2 that leave a surprising token stream behind.
#[test]
fn lexer_quirks_still_round_trip() {
    for source in [
        "param x = \"unterminated\nparam y = 2\n",
        "param x = 1e5\nparam y = 0x1f\n",
        "param x = )))\n,,,\nbody b = f(,)\n",
        "export\nexport \"a\"\nexport \"a\" from\n",
        "units\nunits units\n",
        "param x = (1 + )\nparam y = f(1, , 2)\n",
        "\u{1F600} param x = 1\n",
        "",
        "\n",
        "   ",
    ] {
        let parse = parse(source);
        assert_eq!(print(&parse.syntax()), source, "source: {source:?}");
    }
}

#[test]
fn reparsing_printed_output_is_stable() {
    let source = "units mm\n\n# note\nparam x = 1\nbody b = cuboid(x, x, x)\n";
    let once = print(&parse(source).syntax());
    let twice = print(&parse(&once).syntax());
    assert_eq!(once, source);
    assert_eq!(twice, once);
}

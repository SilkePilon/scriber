# Scriber Milestone 1 — The Language Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `scriber-lang` — a document parses, type-checks, evaluates, and produces geometry — and prove that a document can be reprinted byte for byte.

**Architecture:** A `rowan` lossless concrete syntax tree holds every byte of source including whitespace and comments, so reprinting is concatenating the tree's tokens. A typed AST layer gives ergonomic access. Evaluation runs against a `Backend` trait, so the language's tests never touch OCCT.

**Tech Stack:** Rust 2024, `rowan` 0.17 (CST), `logos` 0.16 (lexer), `codespan-reporting` 0.13 (diagnostics), `proptest` 1.11 (round-trip property), `insta` 1.48 (diagnostic snapshots).

## Global Constraints

- Rust edition **2024**; minimum toolchain **1.97.1**.
- All `unsafe` and all C++ live in `scriber-occt`. **`scriber-lang` must contain no `unsafe`** — note that rowan's own documentation example uses `unsafe { transmute }` for `kind_from_raw`; this plan uses a safe table lookup instead. Do not copy the upstream example.
- OCCT is **always dynamically linked**. Never statically link it — this is a licensing gate under the Open CASCADE LGPL exception.
- Application license is **MIT OR Apache-2.0**, declared in every crate's `Cargo.toml`.
- `scriber-lang` must **not** depend on `scriber-kernel` or `scriber-occt`. Geometry is reached only through the `Backend` trait.
- A document with any error produces **no** geometry and **no** files.
- Lengths normalize to millimetres, angles to radians, at the literal.
- Evaluation is strictly top to bottom. Forward references are errors.
- Every dependency pinned to an exact version or a caret on a published crate. Commit after every task; never commit a failing suite.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `crates/scriber-lang/Cargo.toml` | Crate manifest; no kernel dependency |
| `crates/scriber-lang/src/lib.rs` | Public surface: `parse`, `evaluate`, re-exports |
| `crates/scriber-lang/src/syntax.rs` | `SyntaxKind`, the `Language` impl, type aliases |
| `crates/scriber-lang/src/lexer.rs` | Source text to a flat token list, trivia included |
| `crates/scriber-lang/src/parser.rs` | Recovering parser; tokens to CST |
| `crates/scriber-lang/src/ast.rs` | Typed views over CST nodes |
| `crates/scriber-lang/src/units.rs` | `Dimension`, `Quantity`, unit parsing, the arithmetic table |
| `crates/scriber-lang/src/diag.rs` | `Diagnostic`, severity, spans, rendering |
| `crates/scriber-lang/src/eval.rs` | Evaluates a document against a `Backend` |
| `crates/scriber-lang/src/backend.rs` | The `Backend` trait and `RecordingBackend` |
| `crates/scriber-lang/tests/roundtrip.rs` | The `print(parse(x)) == x` property test |
| `crates/scriber-lang/tests/corpus/*.scr` | Documents rebuilt every CI run |
| `crates/scriber-cli/src/backend.rs` | `KernelBackend`: `Backend` over `scriber-kernel` |
| `crates/scriber-occt/src/shim.{hpp,cpp}` | Gains `write_stl` |
| `crates/scriber-kernel/src/lib.rs` | Gains `Solid::write_stl` |

`syntax.rs` and `ast.rs` are the only files that know rowan exists. Everything else works with typed views.

---

### Task 1: Crate skeleton, SyntaxKind, and lossless printing

This task establishes the representation everything else depends on, and proves the round-trip property on a hand-built tree before any parser exists.

**Files:**
- Create: `crates/scriber-lang/Cargo.toml`
- Create: `crates/scriber-lang/src/lib.rs`
- Create: `crates/scriber-lang/src/syntax.rs`
- Modify: `Cargo.toml` (workspace members and dependencies)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `SyntaxKind` — a `#[repr(u16)]` enum of every token and node kind
  - `SyntaxKind::from_u16(raw: u16) -> Option<SyntaxKind>`
  - `ScriberLang` — the `rowan::Language` impl
  - `type SyntaxNode = rowan::SyntaxNode<ScriberLang>`, `SyntaxToken`, `SyntaxElement`
  - `fn print(node: &SyntaxNode) -> String`

- [ ] **Step 1: Add dependencies to the workspace manifest**

In the root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
rowan = "0.17.0"
logos = "0.16.1"
codespan-reporting = "0.13.1"
proptest = "1.11.0"
insta = "1.48.0"
```

And add the crate to `members`:

```toml
members = ["crates/scriber-kernel", "crates/scriber-occt", "crates/scriber-cli", "crates/scriber-lang"]
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/scriber-lang/Cargo.toml`:

```toml
[package]
name = "scriber-lang"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "The Scriber document language: lexer, parser, typed AST, and evaluator."

[dependencies]
rowan.workspace = true
logos.workspace = true
codespan-reporting.workspace = true

[dev-dependencies]
proptest.workspace = true
insta.workspace = true
```

- [ ] **Step 3: Write the failing test**

Create `crates/scriber-lang/src/syntax.rs` with only this test module at the bottom for now (the rest comes in step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_u16() {
        // The Language impl converts kinds to u16 and back on every tree
        // access. rowan's own example does this with unsafe transmute; we
        // use a table, so this test is what guarantees the table is complete.
        for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
            let raw = *kind as u16;
            assert_eq!(raw as usize, index, "ALL is out of order at {kind:?}");
            assert_eq!(SyntaxKind::from_u16(raw), Some(*kind));
        }

        assert_eq!(SyntaxKind::from_u16(SyntaxKind::ALL.len() as u16), None);
    }

    #[test]
    fn printing_a_tree_reproduces_its_text() {
        // Build `param x` by hand: no parser exists yet.
        let mut builder = rowan::GreenNodeBuilder::new();
        builder.start_node(SyntaxKind::Document.into());
        builder.token(SyntaxKind::ParamKw.into(), "param");
        builder.token(SyntaxKind::Whitespace.into(), " ");
        builder.token(SyntaxKind::Ident.into(), "x");
        builder.finish_node();

        let node = SyntaxNode::new_root(builder.finish());
        assert_eq!(print(&node), "param x");
    }
}
```

- [ ] **Step 4: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/syntax.rs`:

```rust
//! The concrete syntax tree: kinds, the rowan `Language` impl, and printing.
//!
//! This module and `ast` are the only places that know rowan exists.

/// Declares `SyntaxKind` together with a safe `u16` conversion.
///
/// rowan requires converting kinds to `u16` and back. Its own example uses
/// `unsafe { transmute }`; this crate forbids `unsafe`, so the macro also
/// emits a lookup table. Declaration order defines the discriminants, so
/// `ALL` and the enum can never drift apart.
macro_rules! syntax_kinds {
    ($($kind:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($kind),+
        }

        impl SyntaxKind {
            /// Every kind, in discriminant order.
            pub const ALL: &'static [SyntaxKind] = &[$(SyntaxKind::$kind),+];

            /// Safe inverse of `kind as u16`.
            pub fn from_u16(raw: u16) -> Option<SyntaxKind> {
                SyntaxKind::ALL.get(raw as usize).copied()
            }
        }
    };
}

syntax_kinds![
    // Trivia — carries no meaning, but is preserved so printing is lossless.
    Whitespace,
    Comment,
    // Tokens.
    Number,
    String,
    Ident,
    UnitsKw,
    ParamKw,
    BodyKw,
    ExportKw,
    FromKw,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    Eq,
    // A token or span the parser could not make sense of.
    Error,
    // Nodes.
    Document,
    UnitsDecl,
    ParamDecl,
    BodyDecl,
    ExportStmt,
    BinExpr,
    UnaryExpr,
    ParenExpr,
    Literal,
    NameRef,
    CallExpr,
    ArgList,
    Arg,
];

impl SyntaxKind {
    /// Trivia is skipped when walking meaningful children.
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::Whitespace | SyntaxKind::Comment)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// The language tag rowan generics are instantiated with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScriberLang {}

impl rowan::Language for ScriberLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_u16(raw.0).expect("every raw kind originates from SyntaxKind")
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<ScriberLang>;
pub type SyntaxToken = rowan::SyntaxToken<ScriberLang>;
pub type SyntaxElement = rowan::SyntaxElement<ScriberLang>;

/// Renders a tree back to text.
///
/// This is exact by construction: every byte of the source, trivia included,
/// is a token in the tree, so printing is concatenation. The round-trip
/// property test guards this rather than establishing it.
pub fn print(node: &SyntaxNode) -> String {
    node.text().to_string()
}
```

- [ ] **Step 5: Wire up the library root**

Create `crates/scriber-lang/src/lib.rs`:

```rust
//! The Scriber document language.
//!
//! A document is parsed into a lossless concrete syntax tree, viewed through
//! a typed AST, and evaluated against a [`Backend`] that supplies geometry.
//! This crate never links OCCT.

pub mod syntax;

pub use syntax::{SyntaxKind, SyntaxNode, SyntaxToken, print};
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 2 tests — `every_kind_round_trips_through_u16` and `printing_a_tree_reproduces_its_text`

- [ ] **Step 7: Confirm the no-unsafe and no-kernel constraints**

Run: `cargo rustc -p scriber-lang --lib -- -D unsafe_code && grep -n 'scriber-kernel\|scriber-occt' crates/scriber-lang/Cargo.toml`
Expected: the compile succeeds and the grep prints nothing.

The lint is used rather than grepping for the word `unsafe`, because the source
contains that word in comments explaining why the macro replaces rowan's
`transmute`. A grep would fail on its own documentation; the lint checks the
actual constraint.

- [ ] **Step 8: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/scriber-lang
git commit -m "feat(lang): add crate skeleton, SyntaxKind, and lossless printing"
```

---

### Task 2: The lexer

**Files:**
- Create: `crates/scriber-lang/src/lexer.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: `SyntaxKind` from Task 1.
- Produces: `fn tokenize(source: &str) -> Vec<Token>` where `pub struct Token { pub kind: SyntaxKind, pub text: String }`. The concatenation of every `text` equals the input exactly.

Note on numbers: a literal and its unit suffix lex as **one** `Number` token, so `12mm` is a single token. That avoids an ambiguity — a separate unit token `m` would be indistinguishable from an identifier `m`. `units.rs` splits the suffix in Task 6.

- [ ] **Step 1: Write the failing test**

Create `crates/scriber-lang/src/lexer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::SyntaxKind;

    fn kinds(source: &str) -> Vec<SyntaxKind> {
        tokenize(source).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_a_param_declaration() {
        assert_eq!(
            kinds("param width = 60"),
            vec![
                SyntaxKind::ParamKw,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::Eq,
                SyntaxKind::Whitespace,
                SyntaxKind::Number,
            ]
        );
    }

    #[test]
    fn a_number_carries_its_unit_suffix() {
        // One token, not two: a bare `m` token would be ambiguous with an
        // identifier, so the suffix stays attached and units.rs splits it.
        let tokens = tokenize("12mm");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, SyntaxKind::Number);
        assert_eq!(tokens[0].text, "12mm");
    }

    #[test]
    fn keywords_are_distinguished_from_identifiers() {
        assert_eq!(kinds("body"), vec![SyntaxKind::BodyKw]);
        assert_eq!(kinds("bodyguard"), vec![SyntaxKind::Ident]);
    }

    #[test]
    fn comments_and_newlines_are_trivia() {
        assert_eq!(
            kinds("# hi\nx"),
            vec![SyntaxKind::Comment, SyntaxKind::Whitespace, SyntaxKind::Ident]
        );
    }

    #[test]
    fn an_unterminated_string_is_an_error_token() {
        assert_eq!(kinds("\"abc"), vec![SyntaxKind::Error]);
    }

    #[test]
    fn concatenating_tokens_reproduces_the_source() {
        let source = "units mm\n\n# note\nparam x = 1.5in  # trailing\n";
        let joined: String = tokenize(source).into_iter().map(|t| t.text).collect();
        assert_eq!(joined, source);
    }
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-lang lexer`
Expected: FAIL to compile — `cannot find function 'tokenize' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/lexer.rs`:

```rust
//! Source text to a flat token list.
//!
//! Every byte of the input ends up in exactly one token, trivia included, so
//! the parser can build a lossless tree.

use logos::Logos;

use crate::syntax::SyntaxKind;

/// One lexed token. `text` is owned so the parser need not carry the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub text: String,
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
enum RawToken {
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    #[regex(r"#[^\n]*")]
    Comment,

    // A number keeps its unit suffix: `12mm` is one token. logos prefers the
    // longest match, so this beats Ident for `12mm` and loses to it for `mm`.
    #[regex(r"[0-9]+(\.[0-9]+)?[A-Za-z]*")]
    Number,

    #[regex(r#""[^"\n]*""#)]
    String,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,

    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token("=")]
    Eq,
}

/// Splits `source` into tokens. Never fails: anything unrecognised becomes an
/// `Error` token so the input is still reproduced exactly.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut lexer = RawToken::lexer(source);

    while let Some(result) = lexer.next() {
        let text = lexer.slice().to_string();
        let kind = match result {
            Ok(raw) => classify(raw, &text),
            Err(()) => SyntaxKind::Error,
        };

        // Merge runs of Error so a garbled region is one token, not many.
        if kind == SyntaxKind::Error {
            if let Some(last) = out.last_mut() {
                let last: &mut Token = last;
                if last.kind == SyntaxKind::Error {
                    last.text.push_str(&text);
                    continue;
                }
            }
        }

        out.push(Token { kind, text });
    }

    out
}

fn classify(raw: RawToken, text: &str) -> SyntaxKind {
    match raw {
        RawToken::Whitespace => SyntaxKind::Whitespace,
        RawToken::Comment => SyntaxKind::Comment,
        RawToken::Number => SyntaxKind::Number,
        RawToken::String => SyntaxKind::String,
        RawToken::Plus => SyntaxKind::Plus,
        RawToken::Minus => SyntaxKind::Minus,
        RawToken::Star => SyntaxKind::Star,
        RawToken::Slash => SyntaxKind::Slash,
        RawToken::LParen => SyntaxKind::LParen,
        RawToken::RParen => SyntaxKind::RParen,
        RawToken::Comma => SyntaxKind::Comma,
        RawToken::Eq => SyntaxKind::Eq,
        RawToken::Ident => match text {
            "units" => SyntaxKind::UnitsKw,
            "param" => SyntaxKind::ParamKw,
            "body" => SyntaxKind::BodyKw,
            "export" => SyntaxKind::ExportKw,
            "from" => SyntaxKind::FromKw,
            _ => SyntaxKind::Ident,
        },
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add below `pub mod syntax;`:

```rust
pub mod lexer;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 8 tests

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add the lexer"
```

---

### Task 3: The parser and the round-trip property

The parser is the heart of the milestone. It must consume every token — including trivia and errors — so the tree reproduces the source exactly.

**Files:**
- Create: `crates/scriber-lang/src/parser.rs`
- Create: `crates/scriber-lang/tests/roundtrip.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: `tokenize`, `Token` from Task 2; `SyntaxKind`, `SyntaxNode`, `print` from Task 1.
- Produces:
  - `pub struct Parse { pub green: rowan::GreenNode, pub errors: Vec<SyntaxError> }`
  - `pub struct SyntaxError { pub message: String, pub range: rowan::TextRange }`
  - `impl Parse { pub fn syntax(&self) -> SyntaxNode }`
  - `pub fn parse(source: &str) -> Parse`

- [ ] **Step 1: Write the failing tests**

Create `crates/scriber-lang/src/parser.rs` with this test module (implementation follows in step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::print;

    fn tree(source: &str) -> String {
        print(&parse(source).syntax())
    }

    #[test]
    fn reproduces_source_exactly() {
        let source = "units mm\n\n# a note\nparam width = 60\nbody b = cuboid(1, 2, 3)\n";
        assert_eq!(tree(source), source);
    }

    #[test]
    fn reproduces_malformed_source_exactly() {
        // Recovery must not drop bytes, or the tree stops being lossless.
        let source = "param = = 5\nbody b = cuboid(\n";
        assert_eq!(tree(source), source);
        assert!(!parse(source).errors.is_empty());
    }

    #[test]
    fn recovers_and_reports_every_bad_statement() {
        let source = "param = 1\nparam = 2\nparam ok = 3\n";
        let parse = parse(source);
        assert_eq!(parse.errors.len(), 2, "errors: {:?}", parse.errors);
        assert_eq!(print(&parse.syntax()), source);
    }

    #[test]
    fn parses_precedence_left_associatively() {
        // 1 + 2 * 3 must nest as 1 + (2 * 3); the shape is asserted via the
        // debug tree so a precedence regression is visible.
        let parse = parse("param x = 1 + 2 * 3");
        let debug = format!("{:#?}", parse.syntax());
        assert!(parse.errors.is_empty(), "{:?}", parse.errors);
        assert!(debug.matches("BinExpr").count() == 2, "{debug}");
    }

    #[test]
    fn accepts_named_and_trailing_commas_in_calls() {
        let source = "body b = cylinder(radius = 2, height = 10,)";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "{:?}", parse.errors);
        assert_eq!(print(&parse.syntax()), source);
    }
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p scriber-lang parser`
Expected: FAIL to compile — `cannot find function 'parse' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/parser.rs`:

```rust
//! A recovering parser producing a lossless tree.
//!
//! Recovery is at statement boundaries: on an unexpected token the parser
//! wraps the rest of the line in an `Error` node and resumes at the next
//! statement keyword. Every token is emitted regardless, so the tree always
//! reproduces the source.

use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};

use crate::lexer::{Token, tokenize};
use crate::syntax::{SyntaxKind, SyntaxNode};

/// A syntax error with the span it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

/// The result of parsing: a tree plus every error found.
#[derive(Debug, Clone)]
pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

/// Parses `source`. Never panics and never loses input.
pub fn parse(source: &str) -> Parse {
    let mut parser = Parser {
        tokens: tokenize(source),
        pos: 0,
        offset: TextSize::new(0),
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    };

    parser.document();

    Parse { green: parser.builder.finish(), errors: parser.errors }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    offset: TextSize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

impl Parser {
    fn document(&mut self) {
        self.builder.start_node(SyntaxKind::Document.into());
        self.eat_trivia();

        while !self.at_end() {
            self.statement();
            self.eat_trivia();
        }

        self.builder.finish_node();
    }

    fn statement(&mut self) {
        match self.current() {
            Some(SyntaxKind::UnitsKw) => self.simple_stmt(SyntaxKind::UnitsDecl, |p| {
                p.bump();
                p.expect(SyntaxKind::Ident, "expected a unit after `units`");
            }),
            Some(SyntaxKind::ParamKw) => self.binding(SyntaxKind::ParamDecl),
            Some(SyntaxKind::BodyKw) => self.binding(SyntaxKind::BodyDecl),
            Some(SyntaxKind::ExportKw) => self.simple_stmt(SyntaxKind::ExportStmt, |p| {
                p.bump();
                p.expect(SyntaxKind::String, "expected a quoted path after `export`");
                p.eat_trivia();
                if p.current() == Some(SyntaxKind::FromKw) {
                    p.bump();
                    p.expect(SyntaxKind::Ident, "expected a body name after `from`");
                }
            }),
            _ => self.error_statement("expected `units`, `param`, `body` or `export`"),
        }
    }

    fn simple_stmt(&mut self, kind: SyntaxKind, body: impl FnOnce(&mut Self)) {
        self.builder.start_node(kind.into());
        body(self);
        self.builder.finish_node();
    }

    fn binding(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
        self.bump();
        self.expect(SyntaxKind::Ident, "expected a name");
        self.eat_trivia();
        self.expect(SyntaxKind::Eq, "expected `=`");
        self.eat_trivia();
        self.expr(0);
        self.builder.finish_node();
    }

    /// Precedence climbing. `min_bp` is the minimum binding power to accept.
    fn expr(&mut self, min_bp: u8) {
        let checkpoint = self.builder.checkpoint();
        self.unary();

        loop {
            self.eat_trivia();
            let Some(op) = self.current() else { break };
            let Some(bp) = binding_power(op) else { break };
            if bp < min_bp {
                break;
            }

            self.builder.start_node_at(checkpoint, SyntaxKind::BinExpr.into());
            self.bump();
            self.eat_trivia();
            // Left-associative: the right operand needs strictly higher power.
            self.expr(bp + 1);
            self.builder.finish_node();
        }
    }

    fn unary(&mut self) {
        self.eat_trivia();
        if self.current() == Some(SyntaxKind::Minus) {
            self.builder.start_node(SyntaxKind::UnaryExpr.into());
            self.bump();
            self.unary();
            self.builder.finish_node();
            return;
        }

        self.primary();
    }

    fn primary(&mut self) {
        self.eat_trivia();
        match self.current() {
            Some(SyntaxKind::Number) => {
                self.builder.start_node(SyntaxKind::Literal.into());
                self.bump();
                self.builder.finish_node();
            }
            Some(SyntaxKind::LParen) => {
                self.builder.start_node(SyntaxKind::ParenExpr.into());
                self.bump();
                self.expr(0);
                self.eat_trivia();
                self.expect(SyntaxKind::RParen, "expected `)`");
                self.builder.finish_node();
            }
            Some(SyntaxKind::Ident) => {
                let checkpoint = self.builder.checkpoint();
                self.bump();
                if self.current() == Some(SyntaxKind::LParen) {
                    self.builder.start_node_at(checkpoint, SyntaxKind::CallExpr.into());
                    self.arg_list();
                    self.builder.finish_node();
                } else {
                    self.builder.start_node_at(checkpoint, SyntaxKind::NameRef.into());
                    self.builder.finish_node();
                }
            }
            _ => self.error_here("expected a number, name or `(`"),
        }
    }

    fn arg_list(&mut self) {
        self.builder.start_node(SyntaxKind::ArgList.into());
        self.bump(); // `(`

        loop {
            self.eat_trivia();
            match self.current() {
                None | Some(SyntaxKind::RParen) => break,
                _ => {}
            }

            self.builder.start_node(SyntaxKind::Arg.into());
            // A named argument is `ident =`; anything else is positional.
            if self.current() == Some(SyntaxKind::Ident) && self.peek_non_trivia(1) == Some(SyntaxKind::Eq) {
                self.bump();
                self.eat_trivia();
                self.bump();
            }
            self.expr(0);
            self.builder.finish_node();

            self.eat_trivia();
            if self.current() == Some(SyntaxKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }

        self.eat_trivia();
        self.expect(SyntaxKind::RParen, "expected `)`");
        self.builder.finish_node();
    }

    /// Wraps everything up to the next statement keyword in an error node.
    fn error_statement(&mut self, message: &str) {
        let start = self.offset;
        self.builder.start_node(SyntaxKind::Error.into());

        while !self.at_end() && !self.at_statement_start() {
            self.bump_raw();
        }

        self.builder.finish_node();
        self.errors.push(SyntaxError {
            message: message.to_string(),
            range: TextRange::new(start, self.offset),
        });
    }

    fn error_here(&mut self, message: &str) {
        let start = self.offset;
        if !self.at_end() && !self.at_statement_start() {
            self.builder.start_node(SyntaxKind::Error.into());
            self.bump_raw();
            self.builder.finish_node();
        }
        self.errors.push(SyntaxError {
            message: message.to_string(),
            range: TextRange::new(start, self.offset),
        });
    }

    fn expect(&mut self, kind: SyntaxKind, message: &str) {
        self.eat_trivia();
        if self.current() == Some(kind) {
            self.bump();
        } else {
            self.error_here(message);
        }
    }

    fn at_statement_start(&self) -> bool {
        matches!(
            self.current(),
            Some(SyntaxKind::UnitsKw)
                | Some(SyntaxKind::ParamKw)
                | Some(SyntaxKind::BodyKw)
                | Some(SyntaxKind::ExportKw)
        )
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn current(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.pos).map(|t| t.kind)
    }

    /// The kind `n` non-trivia tokens ahead of the cursor.
    fn peek_non_trivia(&self, n: usize) -> Option<SyntaxKind> {
        self.tokens[self.pos..]
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .nth(n)
            .map(|t| t.kind)
    }

    /// Emits trivia so it lands in the tree rather than being skipped.
    fn eat_trivia(&mut self) {
        while self.current().is_some_and(SyntaxKind::is_trivia) {
            self.bump_raw();
        }
    }

    fn bump(&mut self) {
        self.eat_trivia();
        self.bump_raw();
    }

    fn bump_raw(&mut self) {
        let Some(token) = self.tokens.get(self.pos) else { return };
        self.builder.token(token.kind.into(), &token.text);
        self.offset += TextSize::of(token.text.as_str());
        self.pos += 1;
    }
}

fn binding_power(kind: SyntaxKind) -> Option<u8> {
    match kind {
        SyntaxKind::Plus | SyntaxKind::Minus => Some(1),
        SyntaxKind::Star | SyntaxKind::Slash => Some(3),
        _ => None,
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod parser;

pub use parser::{Parse, SyntaxError, parse};
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 13 tests

- [ ] **Step 6: Write the round-trip property test**

Create `crates/scriber-lang/tests/roundtrip.rs`:

```rust
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

#[test]
fn reparsing_printed_output_is_stable() {
    let source = "units mm\n\n# note\nparam x = 1\nbody b = cuboid(x, x, x)\n";
    let once = print(&parse(source).syntax());
    let twice = print(&parse(&once).syntax());
    assert_eq!(once, source);
    assert_eq!(twice, once);
}
```

- [ ] **Step 7: Run the property test**

Run: `cargo test -p scriber-lang --test roundtrip`
Expected: PASS, 2 tests. `printing_a_parsed_document_reproduces_it` runs 2000 cases.

If it fails, proptest prints a minimal shrunk input — fix the parser so that input round-trips. **Do not weaken the generator or the assertion**; a failure here means the tree is genuinely losing bytes, which is the one thing this milestone must not do.

- [ ] **Step 8: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 9: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add the recovering parser and the round-trip property"
```

---

### Task 4: Typed AST views

**Files:**
- Create: `crates/scriber-lang/src/ast.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: `SyntaxNode`, `SyntaxKind`, `SyntaxToken` from Task 1.
- Produces:
  - `pub struct Document(SyntaxNode)` with `fn cast(SyntaxNode) -> Option<Self>` and `fn statements(&self) -> impl Iterator<Item = Stmt>`
  - `pub enum Stmt { Units(UnitsDecl), Param(ParamDecl), Body(BodyDecl), Export(ExportStmt) }`
  - `UnitsDecl::unit(&self) -> Option<SyntaxToken>`
  - `ParamDecl::name(&self) -> Option<SyntaxToken>`, `ParamDecl::value(&self) -> Option<Expr>`
  - `BodyDecl::name(&self) -> Option<SyntaxToken>`, `BodyDecl::value(&self) -> Option<Expr>`
  - `ExportStmt::path(&self) -> Option<SyntaxToken>`, `ExportStmt::body(&self) -> Option<SyntaxToken>`
  - `pub enum Expr { Bin(BinExpr), Unary(UnaryExpr), Paren(ParenExpr), Literal(Literal), Name(NameRef), Call(CallExpr) }`
  - `BinExpr::lhs/rhs(&self) -> Option<Expr>`, `BinExpr::op(&self) -> Option<SyntaxKind>`
  - `UnaryExpr::operand(&self) -> Option<Expr>`
  - `ParenExpr::inner(&self) -> Option<Expr>`
  - `Literal::token(&self) -> Option<SyntaxToken>`
  - `NameRef::token(&self) -> Option<SyntaxToken>`
  - `CallExpr::callee(&self) -> Option<SyntaxToken>`, `CallExpr::args(&self) -> impl Iterator<Item = Arg>`
  - `Arg::name(&self) -> Option<SyntaxToken>`, `Arg::value(&self) -> Option<Expr>`
  - Every node type exposes `fn syntax(&self) -> &SyntaxNode` and `fn range(&self) -> rowan::TextRange`

Accessors return `Option` because the tree may come from a document with errors. That is rowan's model and the reason the evaluator reports rather than panics.

- [ ] **Step 1: Write the failing test**

Create `crates/scriber-lang/src/ast.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn document(source: &str) -> Document {
        Document::cast(parse(source).syntax()).expect("root is a Document")
    }

    #[test]
    fn reads_a_param_declaration() {
        let doc = document("param width = 60");
        let stmts: Vec<Stmt> = doc.statements().collect();
        assert_eq!(stmts.len(), 1);

        let Stmt::Param(param) = &stmts[0] else { panic!("expected a param, got {stmts:?}") };
        assert_eq!(param.name().unwrap().text(), "width");
        assert!(matches!(param.value(), Some(Expr::Literal(_))));
    }

    #[test]
    fn reads_a_binary_expression_with_correct_nesting() {
        let doc = document("param x = 1 + 2 * 3");
        let Some(Stmt::Param(param)) = doc.statements().next() else { panic!() };
        let Some(Expr::Bin(add)) = param.value() else { panic!("expected a binary expr") };

        assert_eq!(add.op(), Some(crate::SyntaxKind::Plus));
        assert!(matches!(add.lhs(), Some(Expr::Literal(_))));
        // The multiply must be the right operand, proving precedence.
        assert!(matches!(add.rhs(), Some(Expr::Bin(_))));
    }

    #[test]
    fn reads_call_arguments_positional_and_named() {
        let doc = document("body b = cylinder(2, height = 10)");
        let Some(Stmt::Body(body)) = doc.statements().next() else { panic!() };
        let Some(Expr::Call(call)) = body.value() else { panic!("expected a call") };

        assert_eq!(call.callee().unwrap().text(), "cylinder");
        let args: Vec<Arg> = call.args().collect();
        assert_eq!(args.len(), 2);
        assert!(args[0].name().is_none());
        assert_eq!(args[1].name().unwrap().text(), "height");
    }

    #[test]
    fn reads_an_export_with_and_without_from() {
        let doc = document("export \"a.step\" from part\nexport \"b.stl\"");
        let stmts: Vec<Stmt> = doc.statements().collect();

        let Stmt::Export(first) = &stmts[0] else { panic!() };
        assert_eq!(first.path().unwrap().text(), "\"a.step\"");
        assert_eq!(first.body().unwrap().text(), "part");

        let Stmt::Export(second) = &stmts[1] else { panic!() };
        assert!(second.body().is_none());
    }
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-lang ast`
Expected: FAIL to compile — `cannot find type 'Document' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/ast.rs`:

```rust
//! Typed views over the concrete syntax tree.
//!
//! Every accessor returns `Option` because a tree may come from a document
//! with errors. Callers report rather than panic.

use rowan::TextRange;

use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Declares a newtype over a `SyntaxNode` of one kind.
macro_rules! ast_node {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(SyntaxNode);

        impl $name {
            pub fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == $kind { Some(Self(node)) } else { None }
            }

            pub fn syntax(&self) -> &SyntaxNode {
                &self.0
            }

            pub fn range(&self) -> TextRange {
                self.0.text_range()
            }
        }
    };
}

ast_node!(Document, SyntaxKind::Document);
ast_node!(UnitsDecl, SyntaxKind::UnitsDecl);
ast_node!(ParamDecl, SyntaxKind::ParamDecl);
ast_node!(BodyDecl, SyntaxKind::BodyDecl);
ast_node!(ExportStmt, SyntaxKind::ExportStmt);
ast_node!(BinExpr, SyntaxKind::BinExpr);
ast_node!(UnaryExpr, SyntaxKind::UnaryExpr);
ast_node!(ParenExpr, SyntaxKind::ParenExpr);
ast_node!(Literal, SyntaxKind::Literal);
ast_node!(NameRef, SyntaxKind::NameRef);
ast_node!(CallExpr, SyntaxKind::CallExpr);
ast_node!(ArgList, SyntaxKind::ArgList);
ast_node!(Arg, SyntaxKind::Arg);

/// First child token of `kind`, ignoring trivia.
fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == kind)
}

/// First child node that is an expression.
fn expr_child(node: &SyntaxNode) -> Option<Expr> {
    node.children().find_map(Expr::cast)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Units(UnitsDecl),
    Param(ParamDecl),
    Body(BodyDecl),
    Export(ExportStmt),
}

impl Stmt {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::UnitsDecl => UnitsDecl::cast(node).map(Stmt::Units),
            SyntaxKind::ParamDecl => ParamDecl::cast(node).map(Stmt::Param),
            SyntaxKind::BodyDecl => BodyDecl::cast(node).map(Stmt::Body),
            SyntaxKind::ExportStmt => ExportStmt::cast(node).map(Stmt::Export),
            _ => None,
        }
    }

    pub fn range(&self) -> TextRange {
        match self {
            Stmt::Units(n) => n.range(),
            Stmt::Param(n) => n.range(),
            Stmt::Body(n) => n.range(),
            Stmt::Export(n) => n.range(),
        }
    }
}

impl Document {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ {
        self.0.children().filter_map(Stmt::cast)
    }
}

impl UnitsDecl {
    pub fn unit(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }
}

impl ParamDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }

    pub fn value(&self) -> Option<Expr> {
        expr_child(&self.0)
    }
}

impl BodyDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }

    pub fn value(&self) -> Option<Expr> {
        expr_child(&self.0)
    }
}

impl ExportStmt {
    pub fn path(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::String)
    }

    /// The name after `from`, if present.
    pub fn body(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Bin(BinExpr),
    Unary(UnaryExpr),
    Paren(ParenExpr),
    Literal(Literal),
    Name(NameRef),
    Call(CallExpr),
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::BinExpr => BinExpr::cast(node).map(Expr::Bin),
            SyntaxKind::UnaryExpr => UnaryExpr::cast(node).map(Expr::Unary),
            SyntaxKind::ParenExpr => ParenExpr::cast(node).map(Expr::Paren),
            SyntaxKind::Literal => Literal::cast(node).map(Expr::Literal),
            SyntaxKind::NameRef => NameRef::cast(node).map(Expr::Name),
            SyntaxKind::CallExpr => CallExpr::cast(node).map(Expr::Call),
            _ => None,
        }
    }

    pub fn range(&self) -> TextRange {
        match self {
            Expr::Bin(n) => n.range(),
            Expr::Unary(n) => n.range(),
            Expr::Paren(n) => n.range(),
            Expr::Literal(n) => n.range(),
            Expr::Name(n) => n.range(),
            Expr::Call(n) => n.range(),
        }
    }
}

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).next()
    }

    pub fn rhs(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }

    pub fn op(&self) -> Option<SyntaxKind> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .map(|token| token.kind())
            .find(|kind| {
                matches!(
                    kind,
                    SyntaxKind::Plus | SyntaxKind::Minus | SyntaxKind::Star | SyntaxKind::Slash
                )
            })
    }
}

impl UnaryExpr {
    pub fn operand(&self) -> Option<Expr> {
        expr_child(&self.0)
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        expr_child(&self.0)
    }
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Number)
    }
}

impl NameRef {
    pub fn token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Ident)
    }

    pub fn args(&self) -> impl Iterator<Item = Arg> + '_ {
        self.0
            .children()
            .find_map(ArgList::cast)
            .into_iter()
            .flat_map(|list| list.0.children().filter_map(Arg::cast).collect::<Vec<_>>())
    }
}

impl Arg {
    /// The name of a named argument, or `None` if positional.
    pub fn name(&self) -> Option<SyntaxToken> {
        // A name is only present when followed by `=`, which the parser has
        // already decided; here the `=` token's presence confirms it.
        token(&self.0, SyntaxKind::Eq)?;
        token(&self.0, SyntaxKind::Ident)
    }

    pub fn value(&self) -> Option<Expr> {
        expr_child(&self.0)
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod ast;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 19 tests

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add typed AST views over the CST"
```

---

### Task 5: Units and dimensions

**Files:**
- Create: `crates/scriber-lang/src/units.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum Dimension { Length, Angle, Scalar }`
  - `pub struct Quantity { pub value: f64, pub dimension: Dimension }` — `value` is millimetres for `Length`, radians for `Angle`
  - `pub enum LengthUnit { Mm, Cm, M, In, Ft }` with `fn parse(&str) -> Option<Self>` and `fn to_mm(self) -> f64`
  - `pub fn parse_number(text: &str, default: LengthUnit) -> Result<Quantity, UnitError>` — splits `12mm` into value and suffix
  - `pub enum UnitError { UnknownUnit(String), MalformedNumber(String) }`
  - `Quantity::add/sub/mul/div(self, rhs, default) -> Result<Quantity, DimError>`
  - `pub struct DimError { pub message: String }`
  - `Quantity::coerce_to(self, target: Dimension, default: LengthUnit) -> Result<Quantity, DimError>`

- [ ] **Step 1: Write the failing tests**

Create `crates/scriber-lang/src/units.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn q(value: f64, dimension: Dimension) -> Quantity {
        Quantity { value, dimension }
    }

    #[test]
    fn parses_a_bare_number_as_a_scalar() {
        let parsed = parse_number("60", LengthUnit::Mm).unwrap();
        assert_eq!(parsed.dimension, Dimension::Scalar);
        assert_eq!(parsed.value, 60.0);
    }

    #[test]
    fn normalises_lengths_to_millimetres() {
        assert_eq!(parse_number("1cm", LengthUnit::Mm).unwrap().value, 10.0);
        assert_eq!(parse_number("1in", LengthUnit::Mm).unwrap().value, 25.4);
        assert_eq!(parse_number("1m", LengthUnit::Mm).unwrap().dimension, Dimension::Length);
    }

    #[test]
    fn normalises_angles_to_radians() {
        let parsed = parse_number("180deg", LengthUnit::Mm).unwrap();
        assert_eq!(parsed.dimension, Dimension::Angle);
        assert!((parsed.value - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn rejects_an_unknown_suffix() {
        assert!(matches!(
            parse_number("5furlong", LengthUnit::Mm),
            Err(UnitError::UnknownUnit(_))
        ));
    }

    #[test]
    fn a_scalar_reads_as_a_length_when_added_to_one() {
        // `units mm` declares a default *length* unit, so 1 means 1mm here.
        let sum = q(5.0, Dimension::Length)
            .add(q(1.0, Dimension::Scalar), LengthUnit::Mm)
            .unwrap();
        assert_eq!(sum.dimension, Dimension::Length);
        assert_eq!(sum.value, 6.0);
    }

    #[test]
    fn a_scalar_does_not_read_as_an_angle() {
        // There is no declared default angle unit, so this must stay an error.
        assert!(
            q(1.0, Dimension::Angle)
                .add(q(1.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn multiplying_two_lengths_is_rejected() {
        assert!(
            q(2.0, Dimension::Length)
                .mul(q(3.0, Dimension::Length), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn scaling_a_length_keeps_it_a_length() {
        let scaled = q(2.0, Dimension::Length)
            .mul(q(3.0, Dimension::Scalar), LengthUnit::Mm)
            .unwrap();
        assert_eq!(scaled.dimension, Dimension::Length);
        assert_eq!(scaled.value, 6.0);
    }

    #[test]
    fn dividing_like_by_like_yields_a_scalar() {
        let ratio = q(6.0, Dimension::Length)
            .div(q(2.0, Dimension::Length), LengthUnit::Mm)
            .unwrap();
        assert_eq!(ratio.dimension, Dimension::Scalar);
        assert_eq!(ratio.value, 3.0);
    }

    #[test]
    fn dividing_a_scalar_by_a_length_is_rejected() {
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(2.0, Dimension::Length), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn dividing_by_zero_is_an_error_not_an_infinity() {
        assert!(
            q(1.0, Dimension::Scalar)
                .div(q(0.0, Dimension::Scalar), LengthUnit::Mm)
                .is_err()
        );
    }

    #[test]
    fn coercing_a_scalar_to_a_length_uses_document_units() {
        let coerced = q(12.0, Dimension::Scalar)
            .coerce_to(Dimension::Length, LengthUnit::Cm)
            .unwrap();
        assert_eq!(coerced.value, 120.0);
        assert_eq!(coerced.dimension, Dimension::Length);
    }

    #[test]
    fn coercing_an_angle_to_a_length_is_rejected() {
        assert!(
            q(1.0, Dimension::Angle)
                .coerce_to(Dimension::Length, LengthUnit::Mm)
                .is_err()
        );
    }
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p scriber-lang units`
Expected: FAIL to compile — `cannot find type 'Dimension' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/units.rs`:

```rust
//! Dimensions, quantities, and the arithmetic table.
//!
//! Lengths normalise to millimetres and angles to radians at the literal, so
//! everything downstream sees one representation.

/// The three dimensions a value can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Length,
    Angle,
    Scalar,
}

impl Dimension {
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Length => "length",
            Dimension::Angle => "angle",
            Dimension::Scalar => "scalar",
        }
    }
}

/// A value together with its dimension. `value` is millimetres for a length
/// and radians for an angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub dimension: Dimension,
}

/// A dimension rule violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimError {
    pub message: String,
}

impl DimError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

/// Failure while reading a numeric literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitError {
    UnknownUnit(String),
    MalformedNumber(String),
}

/// The length units a document may declare or a literal may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Mm,
    Cm,
    M,
    In,
    Ft,
}

impl LengthUnit {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mm" => Some(LengthUnit::Mm),
            "cm" => Some(LengthUnit::Cm),
            "m" => Some(LengthUnit::M),
            "in" => Some(LengthUnit::In),
            "ft" => Some(LengthUnit::Ft),
            _ => None,
        }
    }

    pub fn to_mm(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Cm => 10.0,
            LengthUnit::M => 1000.0,
            LengthUnit::In => 25.4,
            LengthUnit::Ft => 304.8,
        }
    }
}

/// Splits a numeric literal into value and optional unit suffix.
///
/// The lexer keeps `12mm` as one token precisely so this split happens where
/// a good error message can be produced.
pub fn parse_number(text: &str, default: LengthUnit) -> Result<Quantity, UnitError> {
    let split = text
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (number, suffix) = text.split_at(split);

    let value: f64 = number
        .parse()
        .map_err(|_| UnitError::MalformedNumber(text.to_string()))?;

    if suffix.is_empty() {
        return Ok(Quantity { value, dimension: Dimension::Scalar });
    }

    if let Some(unit) = LengthUnit::parse(suffix) {
        return Ok(Quantity { value: value * unit.to_mm(), dimension: Dimension::Length });
    }

    match suffix {
        "deg" => Ok(Quantity { value: value.to_radians(), dimension: Dimension::Angle }),
        "rad" => Ok(Quantity { value, dimension: Dimension::Angle }),
        _ => Err(UnitError::UnknownUnit(suffix.to_string())),
    }
}

impl Quantity {
    /// Reinterprets a scalar as a length in document units. Any other
    /// conversion is rejected.
    pub fn coerce_to(self, target: Dimension, default: LengthUnit) -> Result<Quantity, DimError> {
        if self.dimension == target {
            return Ok(self);
        }

        if self.dimension == Dimension::Scalar && target == Dimension::Length {
            return Ok(Quantity {
                value: self.value * default.to_mm(),
                dimension: Dimension::Length,
            });
        }

        Err(DimError::new(format!(
            "expected {}, found {}",
            target.name(),
            self.dimension.name()
        )))
    }

    pub fn add(self, rhs: Quantity, default: LengthUnit) -> Result<Quantity, DimError> {
        self.additive(rhs, default, "add", |a, b| a + b)
    }

    pub fn sub(self, rhs: Quantity, default: LengthUnit) -> Result<Quantity, DimError> {
        self.additive(rhs, default, "subtract", |a, b| a - b)
    }

    fn additive(
        self,
        rhs: Quantity,
        default: LengthUnit,
        verb: &str,
        apply: fn(f64, f64) -> f64,
    ) -> Result<Quantity, DimError> {
        use Dimension::{Length, Scalar};

        let (lhs, rhs) = match (self.dimension, rhs.dimension) {
            (a, b) if a == b => (self, rhs),
            // A scalar reads as a length, because `units` declares a default
            // length unit. There is no default angle unit, so angles do not
            // get the same treatment.
            (Length, Scalar) => (self, rhs.coerce_to(Length, default)?),
            (Scalar, Length) => (self.coerce_to(Length, default)?, rhs),
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot {verb} {} and {}",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity { value: apply(lhs.value, rhs.value), dimension: lhs.dimension })
    }

    pub fn mul(self, rhs: Quantity, _default: LengthUnit) -> Result<Quantity, DimError> {
        use Dimension::Scalar;

        let dimension = match (self.dimension, rhs.dimension) {
            (Scalar, other) | (other, Scalar) => other,
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot multiply {} by {} — no operation takes a derived dimension",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity { value: self.value * rhs.value, dimension })
    }

    pub fn div(self, rhs: Quantity, _default: LengthUnit) -> Result<Quantity, DimError> {
        use Dimension::Scalar;

        if rhs.value == 0.0 {
            return Err(DimError::new("division by zero"));
        }

        let dimension = match (self.dimension, rhs.dimension) {
            (a, b) if a == b => Scalar,
            (other, Scalar) => other,
            (a, b) => {
                return Err(DimError::new(format!(
                    "cannot divide {} by {}",
                    a.name(),
                    b.name()
                )));
            }
        };

        Ok(Quantity { value: self.value / rhs.value, dimension })
    }

    pub fn neg(self) -> Quantity {
        Quantity { value: -self.value, dimension: self.dimension }
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod units;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 32 tests

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add dimensions, quantities and unit conversion"
```

---

### Task 6: Diagnostics

**Files:**
- Create: `crates/scriber-lang/src/diag.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: `SyntaxError` from Task 3.
- Produces:
  - `pub struct Diagnostic { pub message: String, pub range: rowan::TextRange, pub note: Option<(String, rowan::TextRange)> }`
  - `Diagnostic::error(message, range) -> Self` and `Diagnostic::with_note(self, message, range) -> Self`
  - `pub fn render(source: &str, path: &str, diagnostics: &[Diagnostic]) -> String`
  - `impl From<SyntaxError> for Diagnostic`

- [ ] **Step 1: Write the failing test**

Create `crates/scriber-lang/src/diag.rs` with this test module:

```rust
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
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-lang diag`
Expected: FAIL to compile — `cannot find type 'Diagnostic' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/diag.rs`:

```rust
//! Diagnostics and their rendering.
//!
//! The same type serves the CLI now and the Script view in a later milestone,
//! so it carries spans rather than pre-rendered text.

use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, termcolor::NoColor};
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
        Self { message: message.into(), range, note: None }
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
    let config = term::Config::default();
    let mut buffer = NoColor::new(Vec::new());

    for diagnostic in diagnostics {
        let mut labels = vec![Label::primary((), to_span(diagnostic.range))];
        if let Some((message, range)) = &diagnostic.note {
            labels.push(Label::secondary((), to_span(*range)).with_message(message));
        }

        let rendered = CsDiagnostic::error()
            .with_message(&diagnostic.message)
            .with_labels(labels);

        // Writing to an in-memory buffer cannot fail.
        term::emit(&mut buffer, &config, &file, &rendered).expect("in-memory write");
    }

    String::from_utf8(buffer.into_inner()).expect("codespan emits valid UTF-8")
}

fn to_span(range: TextRange) -> std::ops::Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod diag;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 34 tests

If `codespan-reporting` 0.13's API differs from the calls above — the `Label::primary` file-id type or `with_labels` in particular — adjust to the version actually resolved and record what changed in your report. Do not downgrade the crate to fit the plan.

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add diagnostics and rendering"
```

---

### Task 7: The Backend trait and the recording backend

**Files:**
- Create: `crates/scriber-lang/src/backend.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: `Quantity` from Task 5.
- Produces:
  - `pub trait Backend { type Body: Clone; fn cuboid(&mut self, dx: f64, dy: f64, dz: f64) -> Result<Self::Body, String>; fn cylinder(&mut self, radius: f64, height: f64) -> Result<Self::Body, String>; fn cut(&mut self, target: &Self::Body, tool: &Self::Body) -> Result<Self::Body, String>; fn export(&mut self, body: &Self::Body, path: &std::path::Path) -> Result<(), String>; }`
  - `pub struct RecordingBackend { pub calls: Vec<String> }` implementing `Backend` with `type Body = usize`

Lengths cross this boundary as bare `f64` millimetres. The dimension system has already done its work by then, and the backend has no business re-checking it.

- [ ] **Step 1: Write the failing test**

Create `crates/scriber-lang/src/backend.rs` with this test module:

```rust
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
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-lang backend`
Expected: FAIL to compile — `cannot find type 'RecordingBackend' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/backend.rs`:

```rust
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
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod backend;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 35 tests

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add the Backend seam and a recording backend"
```

---

### Task 8: The evaluator

**Files:**
- Create: `crates/scriber-lang/src/eval.rs`
- Modify: `crates/scriber-lang/src/lib.rs`

**Interfaces:**
- Consumes: everything from Tasks 3 to 7.
- Produces:
  - `pub enum Value<B> { Quantity(Quantity), Body(B) }`
  - `pub struct Evaluated<B> { pub values: Vec<(String, Value<B>)> }`
  - `pub fn evaluate<B: Backend>(source: &str, base_dir: &std::path::Path, backend: &mut B) -> Result<Evaluated<B::Body>, Vec<Diagnostic>>`

`base_dir` is the directory a document's own `export` paths resolve against.

- [ ] **Step 1: Write the failing tests**

Create `crates/scriber-lang/src/eval.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::RecordingBackend;
    use std::path::Path;

    fn run(source: &str) -> Result<RecordingBackend, Vec<Diagnostic>> {
        let mut backend = RecordingBackend::default();
        evaluate(source, Path::new("/tmp"), &mut backend)?;
        Ok(backend)
    }

    fn errors(source: &str) -> Vec<String> {
        match run(source) {
            Ok(_) => panic!("expected failure"),
            Err(diagnostics) => diagnostics.into_iter().map(|d| d.message).collect(),
        }
    }

    #[test]
    fn evaluates_a_document_into_kernel_calls() {
        let backend = run(
            "units mm\n\
             param w = 60\n\
             body plate = cuboid(w, 40, 12)\n\
             body hole = cylinder(radius = 5, height = 12)\n\
             body part = cut(plate, hole)\n\
             export \"part.step\" from part\n",
        )
        .unwrap();

        assert_eq!(
            backend.calls,
            vec![
                "cuboid(60, 40, 12)".to_string(),
                "cylinder(5, 12)".to_string(),
                "cut(#0, #1)".to_string(),
                "export(#2, /tmp/part.step)".to_string(),
            ]
        );
    }

    #[test]
    fn document_units_scale_bare_numbers() {
        let backend = run("units cm\nbody b = cuboid(1, 2, 3)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(10, 20, 30)".to_string()]);
    }

    #[test]
    fn reports_every_error_not_just_the_first() {
        let messages = errors("param a = nope\nparam b = alsonope\n");
        assert_eq!(messages.len(), 2, "{messages:?}");
    }

    #[test]
    fn rejects_a_forward_reference() {
        let messages = errors("param a = b\nparam b = 1\n");
        assert!(messages[0].contains("`b`"), "{messages:?}");
    }

    #[test]
    fn rejects_a_duplicate_declaration() {
        let messages = errors("param a = 1\nparam a = 2\n");
        assert!(messages[0].contains("already declared"), "{messages:?}");
    }

    #[test]
    fn rejects_binding_geometry_to_a_param() {
        let messages = errors("param a = cuboid(1, 2, 3)\n");
        assert!(messages[0].contains("param"), "{messages:?}");
    }

    #[test]
    fn rejects_binding_a_number_to_a_body() {
        let messages = errors("body a = 5\n");
        assert!(messages[0].contains("body"), "{messages:?}");
    }

    #[test]
    fn rejects_a_dimension_mismatch() {
        let messages = errors("param a = 1mm + 45deg\n");
        assert!(messages[0].contains("add"), "{messages:?}");
    }

    #[test]
    fn rejects_wrong_arity() {
        let messages = errors("body b = cuboid(1, 2)\n");
        assert!(messages[0].contains("expects"), "{messages:?}");
    }

    #[test]
    fn rejects_an_unknown_function() {
        let messages = errors("body b = sphere(1)\n");
        assert!(messages[0].contains("sphere"), "{messages:?}");
    }

    #[test]
    fn evaluates_builtin_maths() {
        let backend = run("param s = sqrt(16)\nbody b = cuboid(s, 1, 1)\n").unwrap();
        assert_eq!(backend.calls, vec!["cuboid(4, 1, 1)".to_string()]);
    }

    #[test]
    fn export_without_from_requires_exactly_one_body() {
        let messages = errors(
            "body a = cuboid(1,1,1)\nbody b = cuboid(2,2,2)\nexport \"x.step\"\n",
        );
        assert!(messages[0].contains("which body"), "{messages:?}");
    }

    #[test]
    fn rejects_an_unknown_export_extension() {
        let messages = errors("body a = cuboid(1,1,1)\nexport \"x.obj\" from a\n");
        assert!(messages[0].contains("obj"), "{messages:?}");
    }

    #[test]
    fn a_document_with_errors_produces_no_calls() {
        let mut backend = RecordingBackend::default();
        let result = evaluate(
            "body a = cuboid(1,1,1)\nexport \"x.step\" from a\nparam bad = nope\n",
            Path::new("/tmp"),
            &mut backend,
        );

        assert!(result.is_err());
        assert!(backend.calls.is_empty(), "geometry ran anyway: {:?}", backend.calls);
    }
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p scriber-lang eval`
Expected: FAIL to compile — `cannot find function 'evaluate' in this scope`

- [ ] **Step 3: Write the implementation**

Put this above the test module in `crates/scriber-lang/src/eval.rs`:

```rust
//! Evaluates a document against a [`Backend`].
//!
//! Two passes. The first resolves names, checks dimensions and validates
//! calls, collecting every diagnostic it can. Only if that pass is clean does
//! the second pass run geometry — so a document with any error produces no
//! files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rowan::TextRange;

use crate::ast::{Document, Expr, Stmt};
use crate::backend::Backend;
use crate::diag::Diagnostic;
use crate::parse;
use crate::syntax::SyntaxKind;
use crate::units::{Dimension, LengthUnit, Quantity, UnitError, parse_number};

/// A bound value: either a number with a dimension, or geometry.
#[derive(Debug, Clone)]
pub enum Value<B> {
    Quantity(Quantity),
    Body(B),
}

/// Every binding a document produced, in declaration order.
#[derive(Debug)]
pub struct Evaluated<B> {
    pub values: Vec<(String, Value<B>)>,
}

/// Parses and evaluates `source`.
///
/// `base_dir` is what a document's own `export` paths resolve against.
pub fn evaluate<B: Backend>(
    source: &str,
    base_dir: &Path,
    backend: &mut B,
) -> Result<Evaluated<B::Body>, Vec<Diagnostic>> {
    let parsed = parse(source);
    let mut diagnostics: Vec<Diagnostic> =
        parsed.errors.iter().cloned().map(Diagnostic::from).collect();

    let Some(document) = Document::cast(parsed.syntax()) else {
        diagnostics.push(Diagnostic::error(
            "not a document",
            TextRange::new(0.into(), 0.into()),
        ));
        return Err(diagnostics);
    };

    let mut evaluator = Evaluator {
        units: LengthUnit::Mm,
        units_declared: false,
        scope: HashMap::new(),
        order: Vec::new(),
        diagnostics,
        base_dir: base_dir.to_path_buf(),
    };

    evaluator.check(&document);

    if !evaluator.diagnostics.is_empty() {
        return Err(evaluator.diagnostics);
    }

    evaluator.run(&document, backend)
}

/// What a name was bound to, without the geometry itself.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Quantity(Dimension),
    Body,
}

struct Evaluator {
    units: LengthUnit,
    units_declared: bool,
    scope: HashMap<String, (Kind, TextRange)>,
    order: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    base_dir: PathBuf,
}

impl Evaluator {
    fn error(&mut self, message: impl Into<String>, range: TextRange) {
        self.diagnostics.push(Diagnostic::error(message, range));
    }

    // ---- pass one: names, dimensions, calls -----------------------------

    fn check(&mut self, document: &Document) {
        for stmt in document.statements() {
            match stmt {
                Stmt::Units(decl) => {
                    let range = decl.range();
                    let Some(token) = decl.unit() else { continue };

                    if self.units_declared {
                        self.error("`units` is already declared", range);
                    } else if !self.order.is_empty() {
                        self.error("`units` must come before any other statement", range);
                    }

                    match LengthUnit::parse(token.text()) {
                        Some(unit) => self.units = unit,
                        None => self.error(
                            format!("`{}` is not a length unit", token.text()),
                            token.text_range(),
                        ),
                    }
                    self.units_declared = true;
                }

                Stmt::Param(decl) => {
                    let range = decl.range();
                    let Some(name) = decl.name() else { continue };
                    let Some(value) = decl.value() else { continue };

                    let kind = self.check_expr(&value);
                    if let Some(Kind::Body) = kind {
                        self.error("a `param` cannot hold geometry — use `body`", range);
                    }
                    self.declare(name.text(), kind.unwrap_or(Kind::Quantity(Dimension::Scalar)), name.text_range());
                }

                Stmt::Body(decl) => {
                    let range = decl.range();
                    let Some(name) = decl.name() else { continue };
                    let Some(value) = decl.value() else { continue };

                    let kind = self.check_expr(&value);
                    if let Some(Kind::Quantity(_)) = kind {
                        self.error("a `body` must hold geometry — use `param`", range);
                    }
                    self.declare(name.text(), Kind::Body, name.text_range());
                }

                Stmt::Export(stmt) => self.check_export(&stmt),
            }
        }
    }

    fn declare(&mut self, name: &str, kind: Kind, range: TextRange) {
        if let Some((_, previous)) = self.scope.get(name) {
            let previous = *previous;
            self.diagnostics.push(
                Diagnostic::error(format!("`{name}` is already declared"), range)
                    .with_note("first declared here", previous),
            );
            return;
        }

        self.scope.insert(name.to_string(), (kind, range));
        self.order.push(name.to_string());
    }

    fn check_export(&mut self, stmt: &crate::ast::ExportStmt) {
        let range = stmt.range();
        let Some(path_token) = stmt.path() else { return };
        let path = path_token.text().trim_matches('"').to_string();

        match extension_format(&path) {
            None => self.error(
                format!("cannot export `{path}` — expected a .step, .stp or .stl extension"),
                path_token.text_range(),
            ),
            Some(_) => {}
        }

        match stmt.body() {
            Some(name) => {
                if !self.scope.contains_key(name.text()) {
                    self.error(format!("`{}` is not declared", name.text()), name.text_range());
                } else if !matches!(self.scope[name.text()].0, Kind::Body) {
                    self.error(format!("`{}` is not a body", name.text()), name.text_range());
                }
            }
            None => {
                let bodies: Vec<&String> = self
                    .order
                    .iter()
                    .filter(|name| matches!(self.scope[*name].0, Kind::Body))
                    .collect();

                if bodies.len() != 1 {
                    self.error(
                        format!(
                            "`export` needs `from <name>` to say which body — the document declares {}",
                            if bodies.is_empty() {
                                "none".to_string()
                            } else {
                                bodies.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
                            }
                        ),
                        range,
                    );
                }
            }
        }
    }

    /// Returns the kind an expression produces, or `None` if it was invalid.
    fn check_expr(&mut self, expr: &Expr) -> Option<Kind> {
        match expr {
            Expr::Literal(literal) => {
                let token = literal.token()?;
                match parse_number(token.text(), self.units) {
                    Ok(quantity) => Some(Kind::Quantity(quantity.dimension)),
                    Err(UnitError::UnknownUnit(unit)) => {
                        self.error(format!("`{unit}` is not a unit"), token.text_range());
                        None
                    }
                    Err(UnitError::MalformedNumber(text)) => {
                        self.error(format!("`{text}` is not a number"), token.text_range());
                        None
                    }
                }
            }

            Expr::Name(name) => {
                let token = name.token()?;
                match self.scope.get(token.text()) {
                    Some((kind, _)) => Some(*kind),
                    None => {
                        self.error(
                            format!("`{}` is not declared — names must be declared before use", token.text()),
                            token.text_range(),
                        );
                        None
                    }
                }
            }

            Expr::Paren(paren) => self.check_expr(&paren.inner()?),
            Expr::Unary(unary) => self.check_expr(&unary.operand()?),

            Expr::Bin(bin) => {
                let lhs = self.check_expr(&bin.lhs()?)?;
                let rhs = self.check_expr(&bin.rhs()?)?;
                let op = bin.op()?;

                let (Kind::Quantity(a), Kind::Quantity(b)) = (lhs, rhs) else {
                    self.error("arithmetic needs numbers, not geometry", expr.range());
                    return None;
                };

                let one = Quantity { value: 1.0, dimension: a };
                let two = Quantity { value: 1.0, dimension: b };
                // Probe the dimension rules with value 1.0 on both sides. The
                // denominator is deliberately non-zero: division by zero is a
                // runtime concern, not a dimension error, and must not be
                // reported here where the operands are placeholders.
                let result = match op {
                    SyntaxKind::Plus => one.add(two, self.units),
                    SyntaxKind::Minus => one.sub(two, self.units),
                    SyntaxKind::Star => one.mul(two, self.units),
                    SyntaxKind::Slash => one.div(two, self.units),
                    _ => Ok(one),
                };

                match result {
                    Ok(quantity) => Some(Kind::Quantity(quantity.dimension)),
                    Err(error) => {
                        self.error(error.message, expr.range());
                        None
                    }
                }
            }

            Expr::Call(call) => {
                let callee = call.callee()?;
                let args: Vec<crate::ast::Arg> = call.args().collect();
                let kinds: Vec<Option<Kind>> =
                    args.iter().map(|arg| arg.value().and_then(|e| self.check_expr(&e))).collect();

                let signature = match signature_of(callee.text()) {
                    Some(signature) => signature,
                    None => {
                        self.error(
                            format!("`{}` is not a known function", callee.text()),
                            callee.text_range(),
                        );
                        return None;
                    }
                };

                if args.len() != signature.params.len() {
                    self.error(
                        format!(
                            "`{}` expects {} argument{}, found {}",
                            callee.text(),
                            signature.params.len(),
                            if signature.params.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                        call.range(),
                    );
                    return None;
                }

                for (arg, kind) in args.iter().zip(&kinds) {
                    if let Some(name) = arg.name() {
                        if !signature.params.contains(&name.text()) {
                            self.error(
                                format!("`{}` has no parameter `{}`", callee.text(), name.text()),
                                name.text_range(),
                            );
                        }
                    }

                    match (signature.wants_body, kind) {
                        (true, Some(Kind::Quantity(_))) => {
                            self.error("expected geometry here", arg.range());
                        }
                        (false, Some(Kind::Body)) => {
                            self.error("expected a number here", arg.range());
                        }
                        _ => {}
                    }
                }

                Some(signature.result)
            }
        }
    }

    // ---- pass two: geometry ---------------------------------------------

    fn run<B: Backend>(
        &mut self,
        document: &Document,
        backend: &mut B,
    ) -> Result<Evaluated<B::Body>, Vec<Diagnostic>> {
        let mut values: HashMap<String, Value<B::Body>> = HashMap::new();
        let mut order: Vec<(String, Value<B::Body>)> = Vec::new();
        let mut errors: Vec<Diagnostic> = Vec::new();

        for stmt in document.statements() {
            let bound = match &stmt {
                Stmt::Param(decl) => decl.name().zip(decl.value()),
                Stmt::Body(decl) => decl.name().zip(decl.value()),
                _ => None,
            };

            if let Some((name, expr)) = bound {
                match self.eval_expr(&expr, &values, backend) {
                    Ok(value) => {
                        values.insert(name.text().to_string(), value.clone());
                        order.push((name.text().to_string(), value));
                    }
                    Err(diagnostic) => errors.push(diagnostic),
                }
                continue;
            }

            if let Stmt::Export(export) = &stmt {
                let Some(path_token) = export.path() else { continue };
                let path = path_token.text().trim_matches('"');
                let target = self.base_dir.join(path);

                let body_name = match export.body() {
                    Some(name) => name.text().to_string(),
                    None => order
                        .iter()
                        .rev()
                        .find(|(_, value)| matches!(value, Value::Body(_)))
                        .map(|(name, _)| name.clone())
                        .unwrap_or_default(),
                };

                match values.get(&body_name) {
                    Some(Value::Body(body)) => {
                        if let Err(message) = backend.export(body, &target) {
                            errors.push(Diagnostic::error(message, export.range()));
                        }
                    }
                    _ => errors.push(Diagnostic::error(
                        format!("`{body_name}` is not a body"),
                        export.range(),
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(Evaluated { values: order })
        } else {
            Err(errors)
        }
    }

    fn eval_expr<B: Backend>(
        &self,
        expr: &Expr,
        scope: &HashMap<String, Value<B::Body>>,
        backend: &mut B,
    ) -> Result<Value<B::Body>, Diagnostic> {
        match expr {
            Expr::Literal(literal) => {
                let token = literal.token().ok_or_else(|| bad(expr))?;
                let quantity =
                    parse_number(token.text(), self.units).map_err(|_| bad(expr))?;
                Ok(Value::Quantity(quantity))
            }

            Expr::Name(name) => {
                let token = name.token().ok_or_else(|| bad(expr))?;
                scope.get(token.text()).cloned().ok_or_else(|| bad(expr))
            }

            Expr::Paren(paren) => {
                self.eval_expr(&paren.inner().ok_or_else(|| bad(expr))?, scope, backend)
            }

            Expr::Unary(unary) => {
                let inner =
                    self.eval_expr(&unary.operand().ok_or_else(|| bad(expr))?, scope, backend)?;
                match inner {
                    Value::Quantity(quantity) => Ok(Value::Quantity(quantity.neg())),
                    Value::Body(_) => Err(Diagnostic::error(
                        "cannot negate geometry",
                        expr.range(),
                    )),
                }
            }

            Expr::Bin(bin) => {
                let lhs = self.eval_expr(&bin.lhs().ok_or_else(|| bad(expr))?, scope, backend)?;
                let rhs = self.eval_expr(&bin.rhs().ok_or_else(|| bad(expr))?, scope, backend)?;
                let op = bin.op().ok_or_else(|| bad(expr))?;

                let (Value::Quantity(a), Value::Quantity(b)) = (lhs, rhs) else {
                    return Err(Diagnostic::error(
                        "arithmetic needs numbers, not geometry",
                        expr.range(),
                    ));
                };

                let result = match op {
                    SyntaxKind::Plus => a.add(b, self.units),
                    SyntaxKind::Minus => a.sub(b, self.units),
                    SyntaxKind::Star => a.mul(b, self.units),
                    SyntaxKind::Slash => a.div(b, self.units),
                    _ => return Err(bad(expr)),
                };

                result
                    .map(Value::Quantity)
                    .map_err(|error| Diagnostic::error(error.message, expr.range()))
            }

            Expr::Call(call) => {
                let callee = call.callee().ok_or_else(|| bad(expr))?;
                let mut evaluated = Vec::new();
                for arg in call.args() {
                    let value = arg.value().ok_or_else(|| bad(expr))?;
                    evaluated.push(self.eval_expr(&value, scope, backend)?);
                }

                self.call(callee.text(), &evaluated, expr.range(), backend)
            }
        }
    }

    fn call<B: Backend>(
        &self,
        name: &str,
        args: &[Value<B::Body>],
        range: TextRange,
        backend: &mut B,
    ) -> Result<Value<B::Body>, Diagnostic> {
        let number = |index: usize| -> Result<Quantity, Diagnostic> {
            match &args[index] {
                Value::Quantity(quantity) => Ok(*quantity),
                Value::Body(_) => Err(Diagnostic::error("expected a number here", range)),
            }
        };

        let length = |index: usize| -> Result<f64, Diagnostic> {
            number(index)?
                .coerce_to(Dimension::Length, self.units)
                .map(|quantity| quantity.value)
                .map_err(|error| Diagnostic::error(error.message, range))
        };

        let body = |index: usize| -> Result<B::Body, Diagnostic> {
            match &args[index] {
                Value::Body(body) => Ok(body.clone()),
                Value::Quantity(_) => Err(Diagnostic::error("expected geometry here", range)),
            }
        };

        let scalar = |index: usize| -> Result<f64, Diagnostic> {
            let quantity = number(index)?;
            if quantity.dimension == Dimension::Scalar {
                Ok(quantity.value)
            } else {
                Err(Diagnostic::error(
                    format!("expected a scalar, found {}", quantity.dimension.name()),
                    range,
                ))
            }
        };

        let angle = |index: usize| -> Result<f64, Diagnostic> {
            let quantity = number(index)?;
            if quantity.dimension == Dimension::Angle {
                Ok(quantity.value)
            } else {
                Err(Diagnostic::error(
                    format!("expected an angle, found {}", quantity.dimension.name()),
                    range,
                ))
            }
        };

        let quantity = |value: f64, dimension: Dimension| Ok(Value::Quantity(Quantity { value, dimension }));

        match name {
            "cuboid" => backend
                .cuboid(length(0)?, length(1)?, length(2)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "cylinder" => backend
                .cylinder(length(0)?, length(1)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "cut" => backend
                .cut(&body(0)?, &body(1)?)
                .map(Value::Body)
                .map_err(|message| Diagnostic::error(message, range)),

            "sqrt" => quantity(scalar(0)?.sqrt(), Dimension::Scalar),
            "floor" => quantity(scalar(0)?.floor(), Dimension::Scalar),
            "ceil" => quantity(scalar(0)?.ceil(), Dimension::Scalar),
            "sin" => quantity(angle(0)?.sin(), Dimension::Scalar),
            "cos" => quantity(angle(0)?.cos(), Dimension::Scalar),
            "tan" => quantity(angle(0)?.tan(), Dimension::Scalar),

            "abs" => {
                let value = number(0)?;
                quantity(value.value.abs(), value.dimension)
            }

            "min" | "max" => {
                let a = number(0)?;
                let b = number(1)?;
                // Reuse the additive rules so scalar-and-length agree.
                let unified = a
                    .add(Quantity { value: 0.0, dimension: b.dimension }, self.units)
                    .map_err(|error| Diagnostic::error(error.message, range))?;
                let b = b
                    .coerce_to(unified.dimension, self.units)
                    .map_err(|error| Diagnostic::error(error.message, range))?;
                let a = a
                    .coerce_to(unified.dimension, self.units)
                    .map_err(|error| Diagnostic::error(error.message, range))?;

                let value = if name == "min" { a.value.min(b.value) } else { a.value.max(b.value) };
                quantity(value, unified.dimension)
            }

            _ => Err(Diagnostic::error(format!("`{name}` is not a known function"), range)),
        }
    }
}

fn bad(expr: &Expr) -> Diagnostic {
    Diagnostic::error("could not evaluate this expression", expr.range())
}

/// What a builtin accepts and produces.
struct Signature {
    params: &'static [&'static str],
    wants_body: bool,
    result: Kind,
}

fn signature_of(name: &str) -> Option<Signature> {
    let quantity = Kind::Quantity(Dimension::Scalar);
    Some(match name {
        "cuboid" => Signature { params: &["dx", "dy", "dz"], wants_body: false, result: Kind::Body },
        "cylinder" => Signature { params: &["radius", "height"], wants_body: false, result: Kind::Body },
        "cut" => Signature { params: &["target", "tool"], wants_body: true, result: Kind::Body },
        "sqrt" | "floor" | "ceil" | "sin" | "cos" | "tan" | "abs" => {
            Signature { params: &["x"], wants_body: false, result: quantity }
        }
        "min" | "max" => Signature { params: &["x", "y"], wants_body: false, result: quantity },
        _ => return None,
    })
}

/// STEP or STL, decided by extension.
pub fn extension_format(path: &str) -> Option<&'static str> {
    let lowered = path.to_ascii_lowercase();
    if lowered.ends_with(".step") || lowered.ends_with(".stp") {
        Some("step")
    } else if lowered.ends_with(".stl") {
        Some("stl")
    } else {
        None
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/scriber-lang/src/lib.rs`, add:

```rust
pub mod eval;

pub use eval::{Evaluated, Value, evaluate};
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p scriber-lang`
Expected: PASS, 49 tests

- [ ] **Step 6: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 7: Commit**

```bash
git add crates/scriber-lang
git commit -m "feat(lang): add the evaluator"
```

---

### Task 9: STL export in the kernel

The one kernel change this milestone makes, because the CLI must write `.stl`.

**Files:**
- Modify: `crates/scriber-occt/src/shim.hpp`
- Modify: `crates/scriber-occt/src/shim.cpp`
- Modify: `crates/scriber-occt/src/lib.rs`
- Modify: `crates/scriber-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `ffi::Shape`, `guard()` from the existing bridge.
- Produces: `ffi::write_stl(shape: &Shape, path: &str) -> Result<()>` and `Solid::write_stl(&self, path: impl AsRef<Path>) -> Result<(), Error>`.

OCCT will not mesh a shape on demand for STL: a shape without triangulation writes an empty file. `BRepMesh_IncrementalMesh` must run first.

- [ ] **Step 1: Write the failing test**

Add to the tests module in `crates/scriber-occt/src/lib.rs`:

```rust
    #[test]
    fn write_stl_produces_a_non_empty_solid() {
        // Same convention as every other test in this module: OCCT is not
        // thread-safe, so kernel tests serialize.
        let _kernel = lock_kernel();

        let shape = ffi::make_box(10.0, 10.0, 10.0).expect("box builds");
        let path = std::env::temp_dir()
            .join(format!("scriber_occt_write_stl_{}.stl", std::process::id()));
        std::fs::remove_file(&path).ok();

        ffi::write_stl(&shape, path.to_str().expect("utf-8 path")).expect("write_stl succeeds");

        let contents = std::fs::read_to_string(&path).expect("STL is readable");
        assert!(contents.starts_with("solid"), "not an ASCII STL: {:?}", &contents[..20.min(contents.len())]);
        // A cube is 12 triangles. Zero facets would mean the shape was never
        // meshed, which is the failure mode this test exists to catch.
        assert_eq!(contents.matches("facet normal").count(), 12, "wrong facet count");

        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p scriber-occt write_stl`
Expected: FAIL to compile — `cannot find function 'write_stl' in module 'ffi'`

- [ ] **Step 3: Declare it in the header**

In `crates/scriber-occt/src/shim.hpp`, below the `write_step` declaration:

```cpp
void write_stl(const Shape &shape, rust::Str path);
```

- [ ] **Step 4: Implement it**

In `crates/scriber-occt/src/shim.cpp`, add these includes:

```cpp
#include <BRepMesh_IncrementalMesh.hxx>
#include <StlAPI_Writer.hxx>
```

And this function inside `namespace scriber`:

```cpp
void write_stl(const Shape &shape, rust::Str path) {
  guard([&] {
    silence_kernel_console();

    // STL is a mesh format. OCCT does not triangulate on demand, so an
    // unmeshed shape writes a valid-looking file with zero facets.
    BRepMesh_IncrementalMesh mesher(shape.inner, 0.01);
    if (!mesher.IsDone()) {
      throw std::runtime_error("STL meshing failed");
    }

    const std::string target(path.data(), path.size());

    StlAPI_Writer writer;
    if (!writer.Write(shape.inner, target.c_str())) {
      throw std::runtime_error("STL write failed");
    }
  });
}
```

- [ ] **Step 5: Declare it in the bridge**

In `crates/scriber-occt/src/lib.rs`, inside `unsafe extern "C++"`:

```rust
        /// Writes `shape` to `path` as ASCII STL, meshing it first.
        fn write_stl(shape: &Shape, path: &str) -> Result<()>;
```

- [ ] **Step 6: Run the bridge test**

Run: `cargo test -p scriber-occt`
Expected: PASS, all tests including `write_stl_produces_a_non_empty_solid`

- [ ] **Step 7: Add the safe wrapper with its test**

In `crates/scriber-kernel/src/lib.rs`, add to the tests module:

```rust
    #[test]
    fn stl_export_writes_a_meshed_file() {
        let solid = Solid::cuboid(10.0, 10.0, 10.0).expect("cuboid builds");
        let path = std::env::temp_dir()
            .join(format!("scriber_kernel_export_{}.stl", std::process::id()));
        std::fs::remove_file(&path).ok();

        solid.write_stl(&path).expect("export succeeds");

        let contents = std::fs::read_to_string(&path).expect("readable");
        assert_eq!(contents.matches("facet normal").count(), 12);

        std::fs::remove_file(&path).ok();
    }
```

And this method inside `impl Solid`:

```rust
    /// Writes this solid to `path` as ASCII STL.
    pub fn write_stl(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let as_str = path.to_str().ok_or_else(|| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: "path is not valid UTF-8".to_owned(),
        })?;

        if as_str.contains('\0') {
            return Err(Error::StepWriteFailed {
                path: path.to_path_buf(),
                reason: "path contains an interior NUL".to_owned(),
            });
        }

        ffi::write_stl(&self.inner, as_str).map_err(|exception| Error::StepWriteFailed {
            path: path.to_path_buf(),
            reason: exception.what().to_owned(),
        })
    }
```

- [ ] **Step 8: Run the workspace tests**

Run: `cargo test --workspace`
Expected: PASS, all tests

- [ ] **Step 9: Confirm the licensing gate still holds**

Run: `cargo build --bin scriber && ./scripts/check-dynamic-occt.sh target/debug/scriber`
Expected: `OK: target/debug/scriber dynamically links OCCT.`

- [ ] **Step 10: Commit**

```bash
git add crates/scriber-occt crates/scriber-kernel
git commit -m "feat(kernel): add STL export"
```

---

### Task 10: The CLI

**Files:**
- Create: `crates/scriber-cli/src/backend.rs`
- Modify: `crates/scriber-cli/src/main.rs`
- Modify: `crates/scriber-cli/Cargo.toml`
- Create: `crates/scriber-cli/tests/lang.rs`
- Delete: the `smoke` subcommand and `crates/scriber-cli/tests/smoke.rs`

**Interfaces:**
- Consumes: `evaluate`, `Backend`, `parse`, `print`, `render` from `scriber-lang`; `Solid` from `scriber-kernel`.
- Produces: `KernelBackend` implementing `Backend` with `type Body = Rc<Solid>`, and the five subcommands.

`Solid` is deliberately `!Send`/`!Sync`, so `Rc` is correct here and `Arc` would not compile.

- [ ] **Step 1: Add the dependency**

In `crates/scriber-cli/Cargo.toml`, add under `[dependencies]`:

```toml
scriber-lang = { path = "../scriber-lang", version = "0.1.0" }
```

- [ ] **Step 2: Write the failing integration tests**

Create `crates/scriber-cli/tests/lang.rs`:

```rust
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
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

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
    assert!(!doc.parent().unwrap().join("a.step").exists(), "wrote a file anyway");
}

#[test]
fn fmt_check_accepts_an_unchanged_document() {
    let doc = write("fmt", "units mm\n\n# note\nparam x = 1\n");

    let output = scriber().arg("fmt").arg("--check").arg(&doc).output().expect("runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn volume_reports_the_bored_volume() {
    let doc = write(
        "volume",
        "body plate = cuboid(10, 10, 10)\n\
         body hole = cylinder(radius = 2, height = 10)\n\
         body part = cut(plate, hole)\n",
    );

    let output = scriber().arg("volume").arg(&doc).arg("--body").arg("part").output().expect("runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("968.5841"), "stdout was: {stdout}");
}
```

- [ ] **Step 3: Run them to confirm they fail**

Run: `cargo test -p scriber-cli --test lang`
Expected: FAIL — the `build` subcommand does not exist

- [ ] **Step 4: Write the kernel backend**

Create `crates/scriber-cli/src/backend.rs`:

```rust
//! `scriber-lang`'s `Backend`, implemented over the real kernel.
//!
//! `Solid` is deliberately `!Send`/`!Sync` so it cannot escape the kernel
//! thread, which is why bodies are shared with `Rc` rather than `Arc`.

use std::path::Path;
use std::rc::Rc;

use scriber_kernel::Solid;
use scriber_lang::backend::Backend;

#[derive(Default)]
pub struct KernelBackend;

impl Backend for KernelBackend {
    type Body = Rc<Solid>;

    fn cuboid(&mut self, dx: f64, dy: f64, dz: f64) -> Result<Self::Body, String> {
        Solid::cuboid(dx, dy, dz).map(Rc::new).map_err(|error| error.to_string())
    }

    fn cylinder(&mut self, radius: f64, height: f64) -> Result<Self::Body, String> {
        Solid::cylinder(radius, height).map(Rc::new).map_err(|error| error.to_string())
    }

    fn cut(&mut self, target: &Self::Body, tool: &Self::Body) -> Result<Self::Body, String> {
        target.cut(tool).map(Rc::new).map_err(|error| error.to_string())
    }

    fn export(&mut self, body: &Self::Body, path: &Path) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "step" | "stp" => body.write_step(path).map_err(|error| error.to_string()),
            "stl" => body.write_stl(path).map_err(|error| error.to_string()),
            other => Err(format!("cannot export `.{other}`")),
        }
    }
}
```

- [ ] **Step 5: Rewrite the CLI**

Replace `crates/scriber-cli/src/main.rs` entirely:

```rust
//! Headless entry point for Scriber.

mod backend;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use backend::KernelBackend;
use clap::{Parser, Subcommand};
use scriber_lang::diag::{Diagnostic, render};
use scriber_lang::{Value, evaluate, parse, print};

#[derive(Parser)]
#[command(name = "scriber", version, about = "Scriber CAD, headless")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Evaluate a document and run its `export` statements.
    Build { document: PathBuf },

    /// Parse and type-check a document without producing geometry.
    Check { document: PathBuf },

    /// Export one body, ignoring the document's own `export` statements.
    Export {
        document: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        body: Option<String>,
    },

    /// Reprint a document. With --check, fail if the output differs.
    Fmt {
        document: PathBuf,
        #[arg(long)]
        check: bool,
    },

    /// Print a body's volume.
    Volume {
        document: PathBuf,
        #[arg(long)]
        body: Option<String>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprint!("{failure}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Build { document } => {
            let (source, dir) = read(&document)?;
            let mut backend = KernelBackend;
            evaluate(&source, &dir, &mut backend)
                .map(|_| ())
                .map_err(|diagnostics| report(&source, &document, &diagnostics))
        }

        Command::Check { document } => {
            let (source, dir) = read(&document)?;
            let mut backend = scriber_lang::backend::RecordingBackend::default();
            evaluate(&source, &dir, &mut backend)
                .map(|_| ())
                .map_err(|diagnostics| report(&source, &document, &diagnostics))
        }

        Command::Export { document, output, body } => {
            let (source, dir) = read(&document)?;
            let mut backend = KernelBackend;
            let evaluated = evaluate(&source, &dir, &mut backend)
                .map_err(|diagnostics| report(&source, &document, &diagnostics))?;

            let chosen = pick_body(&evaluated.values, body.as_deref())?;
            scriber_lang::backend::Backend::export(&mut backend, &chosen, &output)
        }

        Command::Fmt { document, check } => {
            let (source, _) = read(&document)?;
            let printed = print(&parse(&source).syntax());

            if check {
                if printed == source {
                    Ok(())
                } else {
                    Err(format!("{} is not formatted\n", document.display()))
                }
            } else {
                print!("{printed}");
                Ok(())
            }
        }

        Command::Volume { document, body } => {
            let (source, dir) = read(&document)?;
            let mut backend = KernelBackend;
            let evaluated = evaluate(&source, &dir, &mut backend)
                .map_err(|diagnostics| report(&source, &document, &diagnostics))?;

            let chosen = pick_body(&evaluated.values, body.as_deref())?;
            let volume = chosen.volume().map_err(|error| error.to_string())?;
            println!("{volume:.4}");
            Ok(())
        }
    }
}

fn read(document: &Path) -> Result<(String, PathBuf), String> {
    let source = std::fs::read_to_string(document)
        .map_err(|error| format!("cannot read {}: {error}\n", document.display()))?;
    let dir = document.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok((source, dir))
}

fn report(source: &str, document: &Path, diagnostics: &[Diagnostic]) -> String {
    render(source, &document.display().to_string(), diagnostics)
}

/// The named body, or the only one if the document has exactly one.
fn pick_body(
    values: &[(String, Value<std::rc::Rc<scriber_kernel::Solid>>)],
    wanted: Option<&str>,
) -> Result<std::rc::Rc<scriber_kernel::Solid>, String> {
    let bodies: Vec<(&String, &std::rc::Rc<scriber_kernel::Solid>)> = values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Body(body) => Some((name, body)),
            Value::Quantity(_) => None,
        })
        .collect();

    match wanted {
        Some(name) => bodies
            .iter()
            .find(|(candidate, _)| candidate.as_str() == name)
            .map(|(_, body)| (*body).clone())
            .ok_or_else(|| format!("`{name}` is not a body in this document\n")),

        None if bodies.len() == 1 => Ok(bodies[0].1.clone()),

        None => Err(format!(
            "use --body to say which body — the document declares {}\n",
            if bodies.is_empty() {
                "none".to_string()
            } else {
                bodies.iter().map(|(name, _)| format!("`{name}`")).collect::<Vec<_>>().join(", ")
            }
        )),
    }
}
```

- [ ] **Step 6: Delete the retired smoke test**

```bash
git rm crates/scriber-cli/tests/smoke.rs
```

The `smoke` subcommand proved the M0 pipeline; `build` supersedes it.

- [ ] **Step 7: Run the tests**

Run: `cargo test --workspace`
Expected: PASS, all tests including the five in `crates/scriber-cli/tests/lang.rs`

- [ ] **Step 8: Run it by hand**

```bash
mkdir -p /tmp/scriber-demo
printf 'units mm\nparam w = 60\nbody plate = cuboid(w, 40, 12)\nbody hole = cylinder(radius = 5, height = 12)\nbody part = cut(plate, hole)\nexport "part.step" from part\n' > /tmp/scriber-demo/doc.scr
cargo run -p scriber-cli -- build /tmp/scriber-demo/doc.scr
head -c 13 /tmp/scriber-demo/part.step
```

Expected: `ISO-10303-21;`

- [ ] **Step 9: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 10: Commit**

```bash
git add -A crates/scriber-cli
git commit -m "feat(cli): evaluate documents, retire the smoke command"
```

---

### Task 11: Corpus, golden diagnostics, and CI

**Files:**
- Create: `crates/scriber-lang/tests/corpus.rs`
- Create: `crates/scriber-lang/tests/corpus/plate.scr`
- Create: `crates/scriber-lang/tests/corpus/units.scr`
- Create: `crates/scriber-lang/tests/diagnostics.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: no code interfaces; a regression net.

- [ ] **Step 1: Write the corpus documents**

Create `crates/scriber-lang/tests/corpus/plate.scr`:

```
# A plate with a bore through it.
units mm

param width  = 60
param height = 40
param bore   = width / 12

body plate = cuboid(width, height, 12)
body hole  = cylinder(radius = bore, height = 12)
body part  = cut(plate, hole)

export "part.step" from part
```

Create `crates/scriber-lang/tests/corpus/units.scr`:

```
# Every unit form the language accepts.
units cm

param a = 1        # a bare number reads as 1cm
param b = 25.4mm
param c = 1in
param d = 2ft
param e = 45deg
param f = 0.5rad
param g = a + b
param h = max(a, b)
param i = sqrt(16)
param j = -(a + b) / 2

body block = cuboid(a, b, c)
```

- [ ] **Step 2: Write the corpus test**

Create `crates/scriber-lang/tests/corpus.rs`:

```rust
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

        assert!(parsed.errors.is_empty(), "{}: {:?}", path.display(), parsed.errors);
        assert_eq!(print(&parsed.syntax()), source, "{} does not round-trip", path.display());

        let mut backend = RecordingBackend::default();
        if let Err(diagnostics) = evaluate(&source, &dir, &mut backend) {
            let messages: Vec<String> = diagnostics.into_iter().map(|d| d.message).collect();
            panic!("{} failed to evaluate: {messages:?}", path.display());
        }

        checked += 1;
    }

    assert!(checked >= 2, "expected at least two corpus documents, found {checked}");
}
```

- [ ] **Step 3: Write the golden diagnostics test**

Create `crates/scriber-lang/tests/diagnostics.rs`:

```rust
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
fn several_errors_at_once() {
    insta::assert_snapshot!(rendered("param a = nope\nparam b = alsonope\nbody c = 5\n"));
}
```

- [ ] **Step 4: Generate and review the snapshots**

Run: `cargo test -p scriber-lang --test diagnostics`
Expected: FAIL — insta reports new snapshots

Run: `cargo insta review` (install with `cargo install cargo-insta` if absent) and accept each snapshot **only after reading it**. These are the error messages users will see; a snapshot that reads badly is a bug to fix in `eval.rs`, not a snapshot to accept.

If `cargo insta` cannot be installed, accept them by setting `INSTA_UPDATE=always cargo test -p scriber-lang --test diagnostics`, then read every file under `crates/scriber-lang/tests/snapshots/` and report their contents.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 6: Document the language in the README**

In `README.md`, replace the `## Usage` section body with:

````markdown
Scriber models are written as documents. Create `plate.scr`:

```
units mm

param width  = 60
param height = 40
param bore   = width / 12

body plate = cuboid(width, height, 12)
body hole  = cylinder(radius = bore, height = 12)
body part  = cut(plate, hole)

export "part.step" from part
export "part.stl"  from part
```

Then build it:

```sh
flatpak run io.github.SilkePilon.Scriber build ~/Documents/plate.scr
```

Both files appear next to the document. Other commands:

| Command | Does |
| --- | --- |
| `build <doc>` | evaluate and run the document's exports |
| `check <doc>` | report errors without producing geometry |
| `export <doc> -o <file> [--body <name>]` | export one body to a chosen path |
| `fmt <doc> [--check]` | reprint a document |
| `volume <doc> [--body <name>]` | print a body's volume |

Milestone 1 is the language. There is no GUI yet, and no sketches, fillets or
selectors — those arrive in later milestones.
````

Leave the Open CASCADE acknowledgement exactly as it is; it is a license obligation.

- [ ] **Step 7: Confirm the attribution survived**

Run: `grep -c 'Open CASCADE Technology software' README.md`
Expected: `1` or greater

- [ ] **Step 8: Check formatting and lints**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output, exit code 0

- [ ] **Step 9: Commit**

```bash
git add crates/scriber-lang README.md
git commit -m "test(lang): add the corpus and golden diagnostics"
```

---

## Milestone 1 Definition of Done

- [ ] `cargo test --workspace` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [ ] The round-trip property passes 2000 generated cases, including malformed input.
- [ ] `cargo rustc -p scriber-lang --lib -- -D unsafe_code` compiles clean.
- [ ] `scriber-lang` does not depend on `scriber-kernel` or `scriber-occt`.
- [ ] `scriber build` produces a STEP file containing `CYLINDRICAL_SURFACE` and an STL with 12 facets for a cube.
- [ ] A document with any error writes no files and exits non-zero.
- [ ] `scripts/check-dynamic-occt.sh` still passes.
- [ ] CI green on `main`.

## What Milestone 1 deliberately does not do

No feature DAG, no incremental rebuild, no memoization, no rollback, no undo, no selector resolution, no project directory format, no git tooling, and no GUI. Each belongs to a later milestone with its own spec.

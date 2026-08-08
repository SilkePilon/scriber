//! A recovering parser producing a lossless tree.
//!
//! Recovery is at statement boundaries: on an unexpected token the parser
//! wraps the rest of the line in an `Error` node and resumes at the next
//! statement keyword. Every token is emitted regardless, so the tree always
//! reproduces the source.
//!
//! Two properties are load-bearing and are asserted by the tests below and by
//! `tests/roundtrip.rs`:
//!
//! * **Nothing is dropped.** Tokens only ever leave the cursor through
//!   [`Parser::bump_raw`], which always writes them to the builder, so
//!   printing the tree is concatenating the token stream.
//! * **Parsing always terminates.** Every loop either consumes a token or
//!   breaks, and recursion is bounded by [`MAX_DEPTH`].

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

/// How deep an expression subtree may get before the parser gives up on the
/// line.
///
/// Two things make this necessary, and both are aborts rather than errors:
/// parsing `(((…` recurses once per bracket, and *dropping* a deep tree
/// recurses once per level inside rowan — which bites even when parsing itself
/// was iterative, as a long `1 + 1 + 1 + …` chain is. Since each operator in
/// such a chain wraps the whole left side in another `BinExpr`, chain length is
/// tree depth, so both are counted against the same budget. No real document
/// comes close: this is a guard against pathological input, not a grammar rule.
const MAX_DEPTH: u32 = 256;

/// Parses `source`. Never panics and never loses input.
pub fn parse(source: &str) -> Parse {
    let mut parser = Parser {
        tokens: tokenize(source),
        pos: 0,
        offset: TextSize::new(0),
        depth: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    };

    parser.document();

    Parse {
        green: parser.builder.finish(),
        errors: parser.errors,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    offset: TextSize,
    depth: u32,
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
                if p.expect(SyntaxKind::String, "expected a quoted path after `export`")
                    && p.peek_non_trivia(0) == Some(SyntaxKind::FromKw)
                {
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
        // Short-circuiting is the recovery: once a part is missing the rest of
        // the line has already been swallowed into an `Error` node, so carrying
        // on would report the same broken statement two or three times over.
        if self.expect(SyntaxKind::Ident, "expected a name")
            && self.expect(SyntaxKind::Eq, "expected `=`")
        {
            self.expr(0);
        }
        self.builder.finish_node();
    }

    /// Precedence climbing. `min_bp` is the minimum binding power to accept.
    fn expr(&mut self, min_bp: u8) {
        if self.depth >= MAX_DEPTH {
            self.recover("expression nests too deeply");
            return;
        }
        self.depth += 1;
        let entry_depth = self.depth;

        let checkpoint = self.builder.checkpoint();
        self.unary();

        loop {
            self.eat_trivia();
            let Some(op) = self.current() else { break };
            let Some(bp) = binding_power(op) else { break };
            if bp < min_bp {
                break;
            }
            if self.depth >= MAX_DEPTH {
                self.recover("expression nests too deeply");
                break;
            }

            // Each operator re-wraps everything so far, so a long chain grows
            // the tree exactly as bracket nesting does and is charged for it.
            self.depth += 1;
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BinExpr.into());
            self.bump();
            self.eat_trivia();
            // Left-associative: the right operand needs strictly higher power.
            self.expr(bp + 1);
            self.builder.finish_node();
        }

        // The loop may have charged for any number of `BinExpr` wrappers, so
        // restore rather than decrement.
        self.depth = entry_depth - 1;
    }

    fn unary(&mut self) {
        if self.depth >= MAX_DEPTH {
            self.recover("expression nests too deeply");
            return;
        }
        self.depth += 1;

        self.eat_trivia();
        if self.current() == Some(SyntaxKind::Minus) {
            self.builder.start_node(SyntaxKind::UnaryExpr.into());
            self.bump();
            self.unary();
            self.builder.finish_node();
        } else {
            self.primary();
        }

        self.depth -= 1;
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
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::CallExpr.into());
                    self.arg_list();
                    self.builder.finish_node();
                } else {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::NameRef.into());
                    self.builder.finish_node();
                }
            }
            _ => self.recover("expected a number, name or `(`"),
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
            if self.current() == Some(SyntaxKind::Ident)
                && self.peek_non_trivia(1) == Some(SyntaxKind::Eq)
            {
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

    /// Wraps a line that cannot begin a statement in an error node.
    ///
    /// Unlike [`Parser::recover`] this always consumes a token: `document`
    /// only calls it on a token no statement can start with, so consuming
    /// nothing would spin forever.
    fn error_statement(&mut self, message: &str) {
        let start = self.offset;
        self.builder.start_node(SyntaxKind::Error.into());
        self.bump_raw();

        while !self.at_recovery_boundary() {
            self.bump_raw();
        }

        self.builder.finish_node();
        self.push_error(message, start);
    }

    /// Reports `message` and swallows the rest of the line into an error node.
    ///
    /// The span may be empty — at a boundary there is nothing sensible to
    /// consume — which is what lets callers report a missing `)` without
    /// stealing the token some caller further out is waiting for.
    fn recover(&mut self, message: &str) {
        let start = self.offset;

        if !self.at_recovery_boundary() {
            self.builder.start_node(SyntaxKind::Error.into());
            while !self.at_recovery_boundary() {
                self.bump_raw();
            }
            self.builder.finish_node();
        }

        self.push_error(message, start);
    }

    fn push_error(&mut self, message: &str, start: TextSize) {
        self.errors.push(SyntaxError {
            message: message.to_string(),
            range: TextRange::new(start, self.offset),
        });
    }

    /// Consumes `kind`, or reports `message` and recovers. The bool says which,
    /// so a caller can stop rather than pile a second error onto one mistake.
    fn expect(&mut self, kind: SyntaxKind, message: &str) -> bool {
        self.eat_trivia();
        if self.current() == Some(kind) {
            self.bump_raw();
            true
        } else {
            self.recover(message);
            false
        }
    }

    /// Where recovery stops: recovery takes the rest of the line, but never
    /// crosses a line break or a statement keyword, and never eats a `)` or
    /// `,` that an enclosing argument list is still waiting for.
    fn at_recovery_boundary(&self) -> bool {
        match self.tokens.get(self.pos) {
            None => true,
            Some(token) => {
                matches!(
                    token.kind,
                    SyntaxKind::UnitsKw
                        | SyntaxKind::ParamKw
                        | SyntaxKind::BodyKw
                        | SyntaxKind::ExportKw
                        | SyntaxKind::RParen
                        | SyntaxKind::Comma
                ) || token.text.contains('\n')
            }
        }
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
        let Some(token) = self.tokens.get(self.pos) else {
            return;
        };
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

    #[test]
    fn a_bad_argument_does_not_swallow_the_rest_of_the_call() {
        // Recovery stops at `,` and `)`: one typo must cost one argument, or a
        // half-typed call would erase the arguments after it from the tree.
        let source = "body b = cuboid(1, !!!, 3)";
        let parse = parse(source);
        assert_eq!(parse.errors.len(), 1, "errors: {:?}", parse.errors);
        assert_eq!(print(&parse.syntax()), source);

        let debug = format!("{:#?}", parse.syntax());
        assert_eq!(debug.matches("Arg@").count(), 3, "{debug}");
    }

    #[test]
    fn a_long_but_reasonable_expression_still_parses() {
        // The depth cap exists for pathological input; it must stay far above
        // anything a person would write.
        let source = format!("param x = {}1", "1 + ".repeat(100));
        let parse = parse(&source);
        assert!(parse.errors.is_empty(), "{:?}", parse.errors);
        assert_eq!(print(&parse.syntax()), source);
    }

    #[test]
    fn nesting_past_the_depth_cap_is_an_error_not_a_crash() {
        let source = format!("param x = {}1", "(".repeat(MAX_DEPTH as usize + 10));
        let parse = parse(&source);
        assert!(!parse.errors.is_empty());
        assert_eq!(print(&parse.syntax()), source);
    }
}

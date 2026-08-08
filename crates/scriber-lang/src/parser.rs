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

        // The space between `=` and the expression belongs to the statement,
        // not to the expression. Eaten before the checkpoint so that a
        // diagnostic spanning this node starts at the first character the user
        // actually wrote rather than at the blank in front of it.
        //
        // Peeked first, though: if nothing ahead can begin an expression there
        // is no expression node to put the space into, and eating it anyway
        // would leave it — and, at the end of a line, the newline — inside
        // whatever node the caller is about to close. `body b = f(x =\n…` is
        // the case that found this: every enclosing node ran on to the next
        // statement. Same rule as `expect`.
        if self.peek_non_trivia(0).is_some_and(starts_an_expression) {
            self.eat_trivia();
        }
        let checkpoint = self.builder.checkpoint();
        self.unary();

        // The operator is peeked rather than consumed. Eating the trivia first
        // would pull the line's trailing newline inside this node on the pass
        // that then finds no operator, and every diagnostic spanning the node
        // would draw a caret running on to the next line.
        while let Some(op) = self.peek_non_trivia(0) {
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
            // `bump` eats the trivia in front of the operator, which lands
            // inside the wrapper where it belongs.
            self.bump();
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

        // Peeked, not eaten, for the reason given in `expr`: `-` may not be
        // there, and on that branch the trivia has nowhere to go.
        if self.peek_non_trivia(0) == Some(SyntaxKind::Minus) {
            self.eat_trivia();
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
        // The trivia in front of the expression belongs outside it, so it is
        // eaten before the node is opened — but only on the branches that open
        // one. The final arm has no node, so it leaves the trivia alone.
        match self.peek_non_trivia(0) {
            Some(SyntaxKind::Number) => {
                self.eat_trivia();
                self.builder.start_node(SyntaxKind::Literal.into());
                self.bump();
                self.builder.finish_node();
            }
            Some(SyntaxKind::LParen) => {
                self.eat_trivia();
                self.builder.start_node(SyntaxKind::ParenExpr.into());
                self.bump();
                self.expr(0);
                // No `eat_trivia` here: `expect` looks past trivia without
                // consuming it, and consumes it only on the branch that finds
                // the `)` and so has somewhere to put it.
                self.expect(SyntaxKind::RParen, "expected `)`");
                self.builder.finish_node();
            }
            Some(SyntaxKind::Ident) => {
                self.eat_trivia();
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
            // Peeked, not eaten: on the pass that ends the list there is no
            // argument node to put the trivia in, and eating it here would
            // pull the line's trailing newline inside the call.
            //
            // A statement keyword ends the list too. The call is missing its
            // `)` and the next line is a new statement; treating that keyword
            // as the start of another argument would eat the newline in front
            // of it and stretch the call over the statement below.
            match self.peek_non_trivia(0) {
                None | Some(SyntaxKind::RParen) => break,
                Some(kind) if starts_a_statement(kind) => break,
                _ => {}
            }

            // Past the break there is definitely another argument, so the
            // space in front of it can be settled — outside `Arg`, where the
            // separator belongs.
            self.eat_trivia();
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

            if self.peek_non_trivia(0) == Some(SyntaxKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }

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
        // `document` eats trivia before dispatching, so this first token is
        // never trivia and the node always ends on something the user typed.
        self.bump_raw();

        while !self.only_trivia_until_boundary() {
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

        // Stops at the last non-trivia token rather than at the boundary
        // itself: the blanks and comment at the end of a broken line are not
        // text an error node should claim, and swallowing them would leave the
        // node — and the diagnostic drawn from its range — ending past the last
        // character the user wrote.
        if !self.only_trivia_until_boundary() {
            self.builder.start_node(SyntaxKind::Error.into());
            while !self.only_trivia_until_boundary() {
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
    ///
    /// The check is a peek: eating the trivia first and only then discovering
    /// the token is missing would leave the space — and, at the end of a line,
    /// the newline — inside whatever node the caller is about to close, and
    /// every diagnostic drawn from that node's range would run on past it. The
    /// trivia is consumed only on the branch that found the token and has
    /// somewhere to put it.
    fn expect(&mut self, kind: SyntaxKind, message: &str) -> bool {
        if self.peek_non_trivia(0) == Some(kind) {
            self.bump();
            true
        } else {
            self.recover(message);
            false
        }
    }

    /// Where recovery stops: recovery takes the rest of the line, but never
    /// crosses a line break or a statement keyword, and never eats a `)` or
    /// `,` that an enclosing argument list is still waiting for.
    fn at_recovery_boundary(&self, index: usize) -> bool {
        match self.tokens.get(index) {
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

    /// Whether everything between the cursor and the next recovery boundary is
    /// trivia — so there is nothing left on this line worth claiming.
    ///
    /// This is where recovery actually stops. Stopping at the boundary instead
    /// would let an error node end on the blanks or trailing comment before it,
    /// and every node's range is a diagnostic's caret.
    fn only_trivia_until_boundary(&self) -> bool {
        let mut index = self.pos;
        loop {
            if self.at_recovery_boundary(index) {
                return true;
            }
            // `at_recovery_boundary` already answered `None`, so this cannot be
            // past the end.
            if !self.tokens[index].kind.is_trivia() {
                return false;
            }
            index += 1;
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

/// Whether a token can begin an expression.
///
/// Used to decide whether the trivia in front of the cursor has a node to land
/// in. It is not a grammar rule — the parser still recovers on anything else —
/// only the answer to "is there an expression here at all".
fn starts_an_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Number | SyntaxKind::Ident | SyntaxKind::LParen | SyntaxKind::Minus
    )
}

/// Whether a token begins a statement, and so ends whatever came before it.
fn starts_a_statement(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::UnitsKw | SyntaxKind::ParamKw | SyntaxKind::BodyKw | SyntaxKind::ExportKw
    )
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
    fn a_node_ends_at_the_last_character_the_user_wrote() {
        // Spans become diagnostics, so trailing trivia inside a node is not
        // cosmetic: an expression that swallows its line's newline renders as a
        // caret running down onto the next line, and the reader is left looking
        // for a mistake on a line that has none.
        let source = "param a = 1mm + 45deg\nparam b = 2\n";
        let debug = format!("{:#?}", parse(source).syntax());

        // `1mm + 45deg` is 10..21; 21 is the newline.
        assert!(debug.contains("BinExpr@10..21"), "{debug}");
        assert!(debug.contains("ParamDecl@0..21"), "{debug}");
        // And the trivia is still in the tree, one level out, so nothing is
        // lost — the round-trip below and in `tests/roundtrip.rs` prove it.
        assert_eq!(print(&parse(source).syntax()), source);
    }

    #[test]
    fn an_unclosed_call_ends_at_the_last_character_on_its_line() {
        // The arity error in `eval` is raised from the call's range, so a call
        // that swallows its line's newline draws a caret block over the next
        // statement — which has nothing wrong with it.
        let source = "body b = cuboid(1, 2\nbody b2 = cuboid(1,2,3)\n";
        let debug = format!("{:#?}", parse(source).syntax());

        // `cuboid(1, 2` is 9..20; 20 is the newline.
        assert!(debug.contains("CallExpr@9..20"), "{debug}");
        assert_eq!(print(&parse(source).syntax()), source);
    }

    #[test]
    fn a_statement_missing_a_part_ends_before_the_blank_line() {
        // `expect` failing must not have eaten the trivia it looked past, or
        // the statement's range covers the empty lines after it.
        let source = "param x\n\nbody y = 2\n";
        let debug = format!("{:#?}", parse(source).syntax());

        // `param x` is 0..7; 7 and 8 are the two newlines.
        assert!(debug.contains("ParamDecl@0..7"), "{debug}");
        assert_eq!(print(&parse(source).syntax()), source);
    }

    #[test]
    fn a_missing_operand_does_not_stretch_the_statement_over_the_next_one() {
        // Found by `tests/spans.rs`, not by reading. `expr` used to eat the
        // trivia in front of the expression before knowing there was one, so a
        // named argument with nothing after its `=` pulled the newline into the
        // `Arg`, and every node out to `BodyDecl` ended on line 2.
        let source = "body b = f(x =\nbody c = 2\n";
        let debug = format!("{:#?}", parse(source).syntax());

        // `body b = f(x =` is 0..14; 14 is the newline.
        assert!(debug.contains("BodyDecl@0..14"), "{debug}");
        assert!(debug.contains("Arg@11..14"), "{debug}");
        assert_eq!(print(&parse(source).syntax()), source);
    }

    #[test]
    fn an_unclosed_call_ends_at_its_last_argument_not_at_the_next_statement() {
        // The argument loop used to take the next statement's keyword for
        // another argument, eating the newline in front of it on the way.
        let source = "body b = cuboid(1,\nparam y = 2\n";
        let debug = format!("{:#?}", parse(source).syntax());

        // `body b = cuboid(1,` is 0..18; 18 is the newline.
        assert!(debug.contains("BodyDecl@0..18"), "{debug}");
        assert!(debug.contains("ArgList@15..18"), "{debug}");
        assert_eq!(print(&parse(source).syntax()), source);
    }

    #[test]
    fn recovery_stops_at_the_last_character_rather_than_at_the_boundary() {
        // Recovery takes the rest of the line — but the blanks at the end of a
        // line are not part of what went wrong, and an error node claiming them
        // draws a caret over empty space.
        let debug = format!("{:#?}", parse("!!!  ").syntax());
        assert!(debug.contains("Error@0..3"), "{debug}");

        let debug = format!("{:#?}", parse("param x = 1 +   ").syntax());
        assert!(debug.contains("ParamDecl@0..13"), "{debug}");
        assert!(debug.contains("BinExpr@10..13"), "{debug}");

        for source in ["!!!  ", "param x = 1 +   "] {
            assert_eq!(print(&parse(source).syntax()), source);
        }
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

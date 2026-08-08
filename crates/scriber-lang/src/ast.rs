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
                if node.kind() == $kind {
                    Some(Self(node))
                } else {
                    None
                }
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
///
/// Direct children only. That is what keeps `ParamDecl::name` from returning
/// an identifier that belongs to the value expression: a call's callee and its
/// named arguments live inside `CallExpr`/`ArgList`/`Arg` nodes, and a
/// recovered statement's leftover tokens live inside an `Error` node, so
/// neither is ever a direct token child of the declaration.
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

        let Stmt::Param(param) = &stmts[0] else {
            panic!("expected a param, got {stmts:?}")
        };
        assert_eq!(param.name().unwrap().text(), "width");
        assert!(matches!(param.value(), Some(Expr::Literal(_))));
    }

    #[test]
    fn reads_a_binary_expression_with_correct_nesting() {
        let doc = document("param x = 1 + 2 * 3");
        let Some(Stmt::Param(param)) = doc.statements().next() else {
            panic!()
        };
        let Some(Expr::Bin(add)) = param.value() else {
            panic!("expected a binary expr")
        };

        assert_eq!(add.op(), Some(crate::SyntaxKind::Plus));
        assert!(matches!(add.lhs(), Some(Expr::Literal(_))));
        // The multiply must be the right operand, proving precedence.
        assert!(matches!(add.rhs(), Some(Expr::Bin(_))));
    }

    #[test]
    fn reads_call_arguments_positional_and_named() {
        let doc = document("body b = cylinder(2, height = 10)");
        let Some(Stmt::Body(body)) = doc.statements().next() else {
            panic!()
        };
        let Some(Expr::Call(call)) = body.value() else {
            panic!("expected a call")
        };

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

        let Stmt::Export(first) = &stmts[0] else {
            panic!()
        };
        assert_eq!(first.path().unwrap().text(), "\"a.step\"");
        assert_eq!(first.body().unwrap().text(), "part");

        let Stmt::Export(second) = &stmts[1] else {
            panic!()
        };
        assert!(second.body().is_none());
    }

    #[test]
    fn a_declaration_name_never_comes_from_its_value() {
        // `token` searches direct children only. A well-formed declaration and
        // its value both hold `Ident` tokens, but the value's live one level
        // down inside `CallExpr`/`NameRef`, so the name cannot be confused.
        let doc = document("param w = width_of(w)\nbody b = cylinder(radius = r)");
        let stmts: Vec<Stmt> = doc.statements().collect();

        let Stmt::Param(param) = &stmts[0] else {
            panic!()
        };
        assert_eq!(param.name().unwrap().text(), "w");
        let Stmt::Body(body) = &stmts[1] else {
            panic!()
        };
        assert_eq!(body.name().unwrap().text(), "b");

        // And when the declaration's own name is missing, recovery parks the
        // rest of the line in an `Error` node — a child node, not a child
        // token — so the name is reported absent rather than borrowed from the
        // value. Returning `cylinder` here would be silently wrong.
        for source in ["param = cylinder(2)", "body = cuboid(1)"] {
            let doc = document(source);
            let name = match doc.statements().next() {
                Some(Stmt::Param(param)) => param.name(),
                Some(Stmt::Body(body)) => body.name(),
                other => panic!("expected a binding, got {other:?}"),
            };
            assert!(name.is_none(), "{source} leaked a name: {name:?}");
        }

        // Likewise an export must not adopt an identifier it never introduced.
        let doc = document("export part");
        let Some(Stmt::Export(export)) = doc.statements().next() else {
            panic!()
        };
        assert!(export.path().is_none());
        assert!(export.body().is_none(), "export borrowed a body name");
    }

    #[test]
    fn a_positional_argument_is_not_named_by_a_nested_one() {
        // The inner call's `x =` is a descendant of the outer argument, so a
        // descendant search would report the outer argument as named `x`.
        let doc = document("body b = outer(inner(x = 1))");
        let Some(Stmt::Body(body)) = doc.statements().next() else {
            panic!()
        };
        let Some(Expr::Call(outer)) = body.value() else {
            panic!()
        };

        let args: Vec<Arg> = outer.args().collect();
        assert_eq!(args.len(), 1);
        assert!(
            args[0].name().is_none(),
            "outer arg picked up a nested name"
        );

        let Some(Expr::Call(inner)) = args[0].value() else {
            panic!()
        };
        assert_eq!(inner.callee().unwrap().text(), "inner");
        let inner_args: Vec<Arg> = inner.args().collect();
        assert_eq!(inner_args[0].name().unwrap().text(), "x");
    }

    #[test]
    fn reads_units_unary_paren_and_name_references() {
        let doc = document("units mm\nparam x = -(w + 1)");
        let stmts: Vec<Stmt> = doc.statements().collect();

        let Stmt::Units(units) = &stmts[0] else {
            panic!()
        };
        assert_eq!(units.unit().unwrap().text(), "mm");
        assert_eq!(&doc.syntax().text().to_string()[units.range()], "units mm");

        let Stmt::Param(param) = &stmts[1] else {
            panic!()
        };
        let Some(Expr::Unary(neg)) = param.value() else {
            panic!("expected a unary expr")
        };
        let Some(Expr::Paren(paren)) = neg.operand() else {
            panic!("expected a paren expr")
        };
        let Some(Expr::Bin(sum)) = paren.inner() else {
            panic!("expected a binary expr")
        };

        assert_eq!(sum.op(), Some(SyntaxKind::Plus));
        let Some(Expr::Name(name)) = sum.lhs() else {
            panic!("expected a name ref")
        };
        assert_eq!(name.token().unwrap().text(), "w");
        let Some(Expr::Literal(one)) = sum.rhs() else {
            panic!("expected a literal")
        };
        assert_eq!(one.token().unwrap().text(), "1");
        assert_eq!(&doc.syntax().text().to_string()[sum.range()], "w + 1");
    }

    #[test]
    fn every_accessor_tolerates_broken_input() {
        // The parser recovers instead of bailing, so accessors are routinely
        // handed half-built statements. None of them may panic.
        let sources = [
            "",
            "param",
            "param =",
            "param x =",
            "param x = 1 +",
            "param x = (",
            "body",
            "body b = f(",
            "body b = f(,)",
            "body b = f(= 1)",
            "body b = f(1 = 2)",
            "body b = f(x =)",
            "units",
            "export",
            "export from part",
            "export \"a.step\" from",
            "!!! param x = 1",
            "param x = 1\n)\nbody b = 2",
        ];

        for source in sources {
            let doc = document(source);
            for stmt in doc.statements() {
                let _ = stmt.range();
                match stmt {
                    Stmt::Units(units) => {
                        let _ = units.unit();
                        let _ = units.syntax();
                    }
                    Stmt::Param(param) => {
                        let _ = param.name();
                        visit_expr(param.value());
                    }
                    Stmt::Body(body) => {
                        let _ = body.name();
                        visit_expr(body.value());
                    }
                    Stmt::Export(export) => {
                        let _ = export.path();
                        let _ = export.body();
                    }
                }
            }
        }
    }

    fn visit_expr(expr: Option<Expr>) {
        let Some(expr) = expr else { return };
        let _ = expr.range();
        match expr {
            Expr::Bin(bin) => {
                let _ = bin.op();
                visit_expr(bin.lhs());
                visit_expr(bin.rhs());
            }
            Expr::Unary(unary) => visit_expr(unary.operand()),
            Expr::Paren(paren) => visit_expr(paren.inner()),
            Expr::Literal(literal) => {
                let _ = literal.token();
            }
            Expr::Name(name) => {
                let _ = name.token();
            }
            Expr::Call(call) => {
                let _ = call.callee();
                for arg in call.args() {
                    let _ = arg.name();
                    visit_expr(arg.value());
                }
            }
        }
    }
}

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

syntax_kinds! {
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
}

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

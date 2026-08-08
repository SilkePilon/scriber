//! The Scriber document language.
//!
//! A document is parsed into a lossless concrete syntax tree, viewed through
//! a typed AST, and evaluated against a [`Backend`] that supplies geometry.
//! This crate never links OCCT.

pub mod syntax;

pub use syntax::{SyntaxKind, SyntaxNode, SyntaxToken, print};

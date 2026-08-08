#![forbid(unsafe_code)]

//! The Scriber document language.
//!
//! A document is parsed into a lossless concrete syntax tree, viewed through
//! a typed AST, and evaluated against a [`Backend`] that supplies geometry.
//! This crate never links OCCT.

// `unsafe` and C++ live only in scriber-occt, where the FFI boundary makes them
// unavoidable. Everything else in the workspace stays in safe Rust, so a memory
// bug can only have come from one crate. `forbid` makes that a compile error
// here rather than a convention a later change can quietly break, and unlike
// `deny` it cannot be lifted by an `allow` further down the tree.

pub mod syntax;

pub use syntax::{SyntaxKind, SyntaxNode, SyntaxToken, print};

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

    // `allow_greedy` is logos 0.16's opt-in for a `[^\n]`-class repetition. It
    // warns because `.*` would scan the whole input; running to end-of-line is
    // exactly what a comment should do, so the scan is bounded and intended.
    #[regex(r"#[^\n]*", allow_greedy = true)]
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
    // Annotated because the Error-merging branch below reads `last.kind`
    // before the first `push`, so inference has nothing to go on there yet.
    let mut out: Vec<Token> = Vec::new();
    let mut lexer = RawToken::lexer(source);

    while let Some(result) = lexer.next() {
        let text = lexer.slice().to_string();
        let kind = match result {
            Ok(raw) => classify(raw, &text),
            Err(()) => SyntaxKind::Error,
        };

        // Merge runs of Error so a garbled region is one token, not many.
        // Written as a let-chain because clippy rejects the nested form.
        if kind == SyntaxKind::Error
            && let Some(last) = out.last_mut()
            && last.kind == SyntaxKind::Error
        {
            last.text.push_str(&text);
            continue;
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
            vec![
                SyntaxKind::Comment,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident
            ]
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

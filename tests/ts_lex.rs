//! TS-M1 lexer tests (S0): token streams, literals, ASI line tracking, errors.

use aoxn::ts::lexer::{lex, Tok, Token};

fn toks(src: &str) -> Vec<Token> {
    lex(0, src).expect("lex failed")
}

fn kinds(src: &str) -> Vec<Tok> {
    toks(src).into_iter().map(|t| t.tok).collect()
}

fn ident(s: &str) -> Tok {
    Tok::Ident(s.to_string())
}

#[test]
fn ts_lex_declaration_token_stream() {
    let ks = kinds("const x: number = 42;");
    assert_eq!(
        ks,
        vec![
            Tok::Kw("const"),
            ident("x"),
            Tok::Punct(":"),
            ident("number"),
            Tok::Punct("="),
            Tok::Number("42".into()),
            Tok::Punct(";"),
            Tok::Eof,
        ]
    );
}

#[test]
fn ts_lex_keywords_vs_contextual_idents() {
    // reserved words become Kw; contextual keywords stay Ident
    let ks = kinds("let type = from;");
    assert_eq!(
        ks,
        vec![Tok::Kw("let"), ident("type"), Tok::Punct("="), ident("from"), Tok::Punct(";"), Tok::Eof]
    );
}

#[test]
fn ts_lex_number_forms() {
    let ks = kinds("0xFF 0b1010 0o17 1_000 1.5e3 .5 7.");
    assert_eq!(
        ks,
        vec![
            Tok::Number("0xFF".into()),
            Tok::Number("0b1010".into()),
            Tok::Number("0o17".into()),
            Tok::Number("1_000".into()),
            Tok::Number("1.5e3".into()),
            Tok::Number(".5".into()),
            Tok::Number("7.".into()),
            Tok::Eof,
        ]
    );
}

#[test]
fn ts_lex_string_escapes() {
    let ks = kinds(r#""a\n\t\\\"A\x42\u{263A}""#);
    assert_eq!(
        ks,
        vec![Tok::Str("a\n\t\\\"AB\u{263A}".into()), Tok::Eof]
    );
}

#[test]
fn ts_lex_string_line_continuation() {
    let ks = kinds("'ab\\\ncd'");
    assert_eq!(ks, vec![Tok::Str("abcd".into()), Tok::Eof]);
}

#[test]
fn ts_lex_template_no_substitution() {
    let ks = kinds("`hi \\`there\\``");
    assert_eq!(ks, vec![Tok::Template("hi `there`".into()), Tok::Eof]);
}

#[test]
fn ts_lex_template_substitution_rejected() {
    let e = lex(0, "`a ${b} c`").expect_err("substitution must be rejected");
    assert!(e.message.contains("template substitutions"), "{}", e.message);
}

#[test]
fn ts_lex_longest_match_punctuators() {
    let ks = kinds("a >>>= b x?.y ??= c => d ...e !== f");
    assert_eq!(
        ks,
        vec![
            ident("a"),
            Tok::Punct(">>>="),
            ident("b"),
            ident("x"),
            Tok::Punct("?."),
            ident("y"),
            Tok::Punct("??="),
            ident("c"),
            Tok::Punct("=>"),
            ident("d"),
            Tok::Punct("..."),
            ident("e"),
            Tok::Punct("!=="),
            ident("f"),
            Tok::Eof,
        ]
    );
}

#[test]
fn ts_lex_ternary_vs_optional_chaining() {
    // `?.5` is `?` then `.5`, not optional chaining
    let ks = kinds("a ? b : c ?.5");
    assert_eq!(
        ks,
        vec![
            ident("a"),
            Tok::Punct("?"),
            ident("b"),
            Tok::Punct(":"),
            ident("c"),
            Tok::Punct("?"),
            Tok::Number(".5".into()),
            Tok::Eof,
        ]
    );
}

#[test]
fn ts_lex_nl_before_for_asi() {
    let ts = toks("a // line comment\nb /* multi\nline */ c");
    assert!(!ts[0].nl_before); // a
    assert!(ts[1].nl_before); // b: newline in line comment
    assert!(ts[2].nl_before); // c: newline inside block comment
    assert!(!ts[3].nl_before); // Eof
}

#[test]
fn ts_lex_positions() {
    let ts = toks("let x\n  y");
    assert_eq!((ts[0].line, ts[0].col), (1, 1)); // let
    assert_eq!((ts[1].line, ts[1].col), (1, 5)); // x
    assert_eq!((ts[2].line, ts[2].col), (2, 3)); // y
}

#[test]
fn ts_lex_errors() {
    assert!(lex(0, "\"unterminated").is_err());
    assert!(lex(0, "'bad \\q escape'").is_err());
    assert!(lex(0, "/* never ends").is_err());
    assert!(lex(0, "let big = 1n;").is_err()); // BigInt: TS-M2 territory
    assert!(lex(0, "let e = 1e;").is_err()); // missing exponent digits
}

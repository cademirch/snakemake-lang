//! Error-reporting tests for malformed directive argument lists.

use snakemake_lang::errors::ParseErrorKind;
use snakemake_lang::{ast::*, parse};

#[test]
fn positional_after_kwarg_is_error() {
    let source = r#"
rule foo:
    input: name="a.txt", "b.txt"
    output: "out.txt"
"#;
    let errors = parse(source, "test.smk").expect_err("expected parse error");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ParseErrorKind::PythonSyntaxError
                && e.message.contains("Positional argument cannot follow keyword argument")),
        "expected positional-after-kwarg error, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn missing_comma_between_kwargs_is_error() {
    let source = r#"
rule foo:
    input: 
        a="x.txt" 
        b="y.txt"
    output: "out.txt"
"#;
    let errors = parse(source, "test.smk").expect_err("expected parse error");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ParseErrorKind::PythonSyntaxError
                && e.message.contains("Expected ','")),
        "expected missing-comma error, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn missing_comma_in_block_form_between_calls_is_error() {
    let source = r#"
rule foo:
    input:
        f("a")
        g("b")
    output: "out.txt"
"#;
    let errors = parse(source, "test.smk").expect_err("expected parse error");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ParseErrorKind::PythonSyntaxError),
        "expected python syntax error, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn missing_comma_in_block_form_between_kwargs_is_error() {
    let source = r#"
rule abc:
    input:
        a="asdas"
        b="123"
"#;
    let errors = parse(source, "test.smk").expect_err("expected parse error");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ParseErrorKind::PythonSyntaxError
                && e.message.contains("Expected ','")),
        "expected missing-comma error, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

/// Pinning test: adjacent bare string literals with no comma are silently
/// concatenated by Python (`"a" "b" == "ab"`). Ruff parses this without error,
/// so today we accept it as a single positional argument. If we ever want to
/// flag this as a Snakemake-level error, this test will need to flip.
#[test]
fn missing_comma_between_string_literals_is_silently_concatenated() {
    let source = r#"
rule foo:
    input: "a.txt" "b.txt"
    output: "out.txt"
"#;
    let ast = parse(source, "test.smk").expect("parses without error today");
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    let input = rule
        .directives
        .iter()
        .find(|d| d.keyword == DirectiveKeyword::Input)
        .expect("input directive");
    let args = match &input.value {
        DirectiveValue::Arguments(a) => a,
        other => panic!("expected Arguments, got {:?}", other),
    };
    assert_eq!(
        args.positional.len(),
        1,
        "two adjacent string literals collapse to one concatenated expression"
    );
    assert!(args.keywords.is_empty());
}

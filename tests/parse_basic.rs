use ruff_python_parser::{Mode, parse_unchecked};
use snakemake_lang::parse;

#[test]
fn ruff_parses_simple_expression() {
    let parsed = parse_unchecked("42 + 1", Mode::Expression.into());
    assert!(parsed.errors().is_empty(), "ruff should parse '42 + 1' without errors");
}

#[test]
fn ruff_parses_function_call_with_kwargs() {
    let source = r#"f("a.txt", "b.txt", ref="genome.fa")"#;
    let parsed = parse_unchecked(source, Mode::Expression.into());
    assert!(parsed.errors().is_empty(), "ruff should parse function call with kwargs");
}

#[test]
fn ruff_parses_module() {
    let source = "x = 1\ny = 2\n";
    let parsed = parse_unchecked(source, Mode::Module.into());
    assert!(parsed.errors().is_empty(), "ruff should parse module");
}

#[test]
fn parse_empty_source() {
    let ast = parse("", "Snakefile").unwrap();
    assert!(ast.body.is_empty());
}

#[test]
fn parse_python_only() {
    let ast = parse("x = 1\ny = 2\n", "Snakefile").unwrap();
    assert_eq!(ast.body.len(), 2);
    for stmt in &ast.body {
        assert!(matches!(stmt, snakemake_lang::ast::Statement::Python(_)));
    }
}

#[test]
fn parse_single_rule_detected() {
    let ast = parse("rule foo:\n    input: 'x.txt'\n", "Snakefile").unwrap();
    assert_eq!(ast.body.len(), 1);
    assert!(matches!(&ast.body[0], snakemake_lang::ast::Statement::Rule(_)));
}

use ruff_python_parser::{Mode, parse_unchecked};

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

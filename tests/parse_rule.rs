use snakemake_lang::{parse, ast::*};
use snakemake_lang::errors::ParseErrorKind;

#[test]
fn parse_simple_rule() {
    let source = "rule foo:\n    input: \"a.txt\"\n    output: \"b.txt\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.name.as_str(), "foo");
    assert!(!rule.is_checkpoint);
    assert_eq!(rule.directives.len(), 2);
    assert_eq!(rule.directives[0].keyword, DirectiveKeyword::Input);
    assert_eq!(rule.directives[1].keyword, DirectiveKeyword::Output);
}

#[test]
fn parse_rule_with_shell() {
    let source = "rule align:\n    input: \"reads.fq\"\n    output: \"aligned.bam\"\n    shell: \"bwa mem {input} > {output}\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.directives.len(), 3);
    assert_eq!(rule.directives[2].keyword, DirectiveKeyword::Shell);
    match &rule.directives[2].value {
        DirectiveValue::Arguments(args) => {
            assert_eq!(args.positional.len(), 1);
            assert!(args.keywords.is_empty());
        }
        other => panic!("expected Arguments, got {:?}", other),
    }
}

#[test]
fn parse_checkpoint() {
    let source = "checkpoint process:\n    input: \"data.csv\"\n    output: directory(\"results/\")\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert!(rule.is_checkpoint);
    assert_eq!(rule.name.as_str(), "process");
}

#[test]
fn parse_rule_with_kwargs() {
    let source = "rule foo:\n    input: reads=\"a.fq\", ref=\"genome.fa\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    match &rule.directives[0].value {
        DirectiveValue::Arguments(args) => {
            assert!(args.positional.is_empty());
            assert_eq!(args.keywords.len(), 2);
            assert_eq!(args.keywords[0].name.as_str(), "reads");
            assert_eq!(args.keywords[1].name.as_str(), "ref");
        }
        other => panic!("expected Arguments, got {:?}", other),
    }
}

#[test]
fn parse_block_form_directive() {
    let source = "rule foo:\n    input:\n        \"a.txt\",\n        \"b.txt\",\n        ref=\"genome.fa\"\n    output: \"result.txt\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.directives.len(), 2);
    match &rule.directives[0].value {
        DirectiveValue::Arguments(args) => {
            assert_eq!(args.positional.len(), 2, "expected 2 positional args");
            assert_eq!(args.keywords.len(), 1, "expected 1 keyword arg");
        }
        other => panic!("expected Arguments, got {:?}", other),
    }
}

#[test]
fn parse_run_block() {
    let source = "rule process:\n    input: \"data.csv\"\n    run:\n        import pandas as pd\n        df = pd.read_csv(input[0])\n        df.to_parquet(output[0])\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.directives.len(), 2);
    assert_eq!(rule.directives[1].keyword, DirectiveKeyword::Run);
    match &rule.directives[1].value {
        DirectiveValue::Block(stmts) => {
            assert_eq!(stmts.len(), 3, "run block should have 3 statements");
        }
        other => panic!("expected Block, got {:?}", other),
    }
}

// ============================================================
// Task 5: Block-form directives and multiline values
// ============================================================

#[test]
fn parse_multiline_parenthesized() {
    let source = "rule foo:\n    input: expand(\"reads/{sample}.fq\",\n                  sample=SAMPLES)\n    output: \"out.txt\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.directives.len(), 2);
    match &rule.directives[0].value {
        DirectiveValue::Arguments(args) => {
            assert_eq!(args.positional.len(), 1, "expand() call should be one positional arg");
        }
        other => panic!("expected Arguments, got {:?}", other),
    }
}

// ============================================================
// Task 6: Multiple rules and Python interleaving
// ============================================================

#[test]
fn parse_multi_rule_with_python() {
    let source = std::fs::read_to_string("tests/fixtures/multi_rule.smk").unwrap();
    let ast = parse(&source, "multi_rule.smk").unwrap();
    let mut rule_count = 0;
    let mut python_count = 0;
    for stmt in &ast.body {
        match stmt {
            Statement::Rule(_) => rule_count += 1,
            Statement::Python(_) => python_count += 1,
            _ => {}
        }
    }
    assert_eq!(rule_count, 2, "should find 2 rules");
    assert!(python_count >= 2, "should find at least 2 Python statements");
}

#[test]
fn parse_rules_in_sequence() {
    let source = "rule a:\n    input: \"x\"\n\nrule b:\n    input: \"y\"\n\nrule c:\n    input: \"z\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rules: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Rule(r) = s { Some(r) } else { None }
    }).collect();
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].name.as_str(), "a");
    assert_eq!(rules[1].name.as_str(), "b");
    assert_eq!(rules[2].name.as_str(), "c");
}

// ============================================================
// Task 6b: Rules inside Python control flow
// ============================================================

#[test]
fn parse_rules_inside_if_block() {
    let source = "if True:\n    rule fastqc:\n        input: \"data/{sample}.fq\"\n        shell: \"fastqc {input}\"\n\nrule always:\n    input: \"x\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rules: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Rule(r) = s { Some(r) } else { None }
    }).collect();
    assert_eq!(rules.len(), 2, "should find both rules (flat AST)");
    assert_eq!(rules[0].name.as_str(), "fastqc");
    assert_eq!(rules[1].name.as_str(), "always");
}

#[test]
fn parse_control_flow_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/control_flow.smk").unwrap();
    let ast = parse(&source, "control_flow.smk").unwrap();
    let rule_count = ast.body.iter().filter(|s| matches!(s, Statement::Rule(_))).count();
    assert!(rule_count >= 2, "should find rules inside control flow, got {rule_count}");
}

// ============================================================
// Task 7: Error recovery
// ============================================================

#[test]
fn parse_unknown_directive_recovers() {
    let source = "rule foo:\n    input: \"a.txt\"\n    bogus: \"what\"\n    output: \"b.txt\"\n";
    let result = parse(source, "test.smk");
    match result {
        Ok(ast) => {
            let rule = match &ast.body[0] {
                Statement::Rule(r) => r,
                other => panic!("expected Rule, got {:?}", other),
            };
            assert!(rule.directives.len() >= 2);
        }
        Err(errors) => {
            assert!(errors.iter().any(|e| e.message.contains("bogus") || e.message.contains("unexpected")),
                "error should mention the unknown directive");
        }
    }
}

#[test]
fn parse_missing_rule_name_reports_error() {
    let source = "rule :\n    input: \"a.txt\"\n";
    let result = parse(source, "test.smk");
    match result {
        Err(errors) => {
            assert!(errors.iter().any(|e| matches!(e.kind,
                ParseErrorKind::MissingRuleName)));
        }
        Ok(_) => {} // recovery is also fine
    }
}

// ============================================================
// Task 8: Documentation checkpoint 1 — fixture tests
// ============================================================

#[test]
fn parse_simple_rule_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/simple_rule.smk").unwrap();
    let ast = parse(&source, "simple_rule.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.name.as_str(), "align");
    assert_eq!(rule.directives.len(), 4);
}

#[test]
fn parse_all_directives_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/all_directives.smk").unwrap();
    let ast = parse(&source, "all_directives.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.name.as_str(), "full_example");
    assert!(rule.directives.len() >= 15, "expected at least 15 directives, got {}", rule.directives.len());
}

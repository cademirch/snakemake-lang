use snakemake_lang::{parse, ast::*};

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

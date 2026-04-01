use snakemake_lang::{parse, ast::*};

#[test]
fn parse_module() {
    let source = "module other_workflow:\n    snakefile: \"other/Snakefile\"\n    config: config\n";
    let ast = parse(source, "test.smk").unwrap();
    let module = match &ast.body[0] { Statement::Module(m) => m, other => panic!("expected Module, got {:?}", other) };
    assert_eq!(module.name.as_str(), "other_workflow");
    assert_eq!(module.directives.len(), 2);
}

#[test]
fn parse_use_rule_simple() {
    let source = "use rule align from qc_module\n";
    let ast = parse(source, "test.smk").unwrap();
    let ur = match &ast.body[0] { Statement::UseRule(u) => u, other => panic!("expected UseRule, got {:?}", other) };
    assert!(matches!(&ur.rules, RuleNames::Named(names) if names.len() == 1));
    assert_eq!(ur.from_module.as_str(), "qc_module");
}

#[test]
fn parse_use_rule_wildcard() {
    let source = "use rule * from other_module exclude trim as qc_*\n";
    let ast = parse(source, "test.smk").unwrap();
    let ur = match &ast.body[0] { Statement::UseRule(u) => u, other => panic!("expected UseRule, got {:?}", other) };
    assert!(matches!(&ur.rules, RuleNames::All));
    assert_eq!(ur.exclude.len(), 1);
    assert_eq!(ur.name_modifier.as_deref(), Some("qc_*"));
}

#[test]
fn parse_use_rule_with_block() {
    let source = "use rule align, sort from other_module with:\n    threads: 16\n    resources:\n        mem_mb=8192\n";
    let ast = parse(source, "test.smk").unwrap();
    let ur = match &ast.body[0] { Statement::UseRule(u) => u, other => panic!("expected UseRule, got {:?}", other) };
    assert!(matches!(&ur.rules, RuleNames::Named(names) if names.len() == 2));
    let wd = ur.with_directives.as_ref().unwrap();
    assert_eq!(wd.len(), 2);
}

#[test]
fn parse_configfile() {
    let source = "configfile: \"config.yaml\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert!(matches!(&ast.body[0], Statement::GlobalDirective(_)));
}

#[test]
fn parse_ruleorder() {
    let source = "ruleorder: align > sort > index\n";
    let ast = parse(source, "test.smk").unwrap();
    let ro = match &ast.body[0] { Statement::Ruleorder(r) => r, other => panic!("expected Ruleorder, got {:?}", other) };
    assert_eq!(ro.names.len(), 3);
    assert_eq!(ro.names[0].as_str(), "align");
}

#[test]
fn parse_localrules() {
    let source = "localrules: all, clean\n";
    let ast = parse(source, "test.smk").unwrap();
    let lr = match &ast.body[0] { Statement::Localrules(l) => l, other => panic!("expected Localrules, got {:?}", other) };
    assert_eq!(lr.names.len(), 2);
}

#[test]
fn parse_storage() {
    let source = "storage s3_data:\n    provider=\"s3\",\n    bucket=\"my-bucket\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let s = match &ast.body[0] { Statement::Storage(s) => s, other => panic!("expected Storage, got {:?}", other) };
    assert_eq!(s.tag.as_str(), "s3_data");
}

#[test]
fn parse_onsuccess_handler() {
    let source = "onsuccess:\n    print(\"Workflow finished!\")\n    shell(\"mail -s 'done' user@example.com\")\n";
    let ast = parse(source, "test.smk").unwrap();
    let h = match &ast.body[0] { Statement::Handler(h) => h, other => panic!("expected Handler, got {:?}", other) };
    assert_eq!(h.kind, HandlerKind::OnSuccess);
    assert_eq!(h.body.len(), 2);
}

#[test]
fn parse_module_use_rule_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/module_use_rule.smk").unwrap();
    let ast = parse(&source, "module_use_rule.smk").unwrap();
    let mut modules = 0; let mut use_rules = 0;
    for stmt in &ast.body { match stmt { Statement::Module(_) => modules += 1, Statement::UseRule(_) => use_rules += 1, _ => {} } }
    assert_eq!(modules, 1);
    assert_eq!(use_rules, 2);
}

#[test]
fn parse_globals_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/globals.smk").unwrap();
    let ast = parse(&source, "globals.smk").unwrap();
    let mut globals = 0; let mut ruleorders = 0; let mut localrules = 0;
    for stmt in &ast.body { match stmt { Statement::GlobalDirective(_) => globals += 1, Statement::Ruleorder(_) => ruleorders += 1, Statement::Localrules(_) => localrules += 1, _ => {} } }
    assert!(globals >= 6, "expected at least 6 global directives, got {globals}");
    assert_eq!(ruleorders, 1);
    assert_eq!(localrules, 1);
}

#[test]
fn parse_handlers_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/handlers.smk").unwrap();
    let ast = parse(&source, "handlers.smk").unwrap();
    let handlers: Vec<_> = ast.body.iter().filter_map(|s| if let Statement::Handler(h) = s { Some(h) } else { None }).collect();
    assert_eq!(handlers.len(), 3);
    assert_eq!(handlers[0].kind, HandlerKind::OnSuccess);
    assert_eq!(handlers[1].kind, HandlerKind::OnError);
    assert_eq!(handlers[2].kind, HandlerKind::OnStart);
}

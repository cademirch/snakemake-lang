use ruff_python_ast::*;
use ruff_python_parser::{Mode, parse_unchecked};
use snakemake_lang::compile;

// ============================================================
// Test helpers
// ============================================================

/// Parse compiled Python output into a list of statements,
/// asserting no syntax errors.
fn parse_compiled(python: &str) -> Vec<Stmt> {
    let parsed = parse_unchecked(python, Mode::Module.into());
    assert!(
        parsed.errors().is_empty(),
        "compiled output has syntax errors: {:?}\n\nOutput:\n{}",
        parsed.errors(),
        python
    );
    match parsed.into_syntax() {
        Mod::Module(m) => m.body,
        _ => panic!("expected module"),
    }
}

/// Extract the method name from a `workflow.method(...)` call expression.
/// Returns `None` if the expression is not a call to `workflow.XXX`.
fn workflow_method_name(expr: &Expr) -> Option<&str> {
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let value = attr.value.as_name_expr()?;
    if value.id.as_str() == "workflow" {
        Some(attr.attr.as_str())
    } else {
        None
    }
}

/// Extract the method name from a `@workflow.method` (non-call) decorator.
fn workflow_decorator_name(expr: &Expr) -> Option<&str> {
    // First try call form: @workflow.method(...)
    if let Some(name) = workflow_method_name(expr) {
        return Some(name);
    }
    // Then try non-call form: @workflow.method
    let attr = expr.as_attribute_expr()?;
    let value = attr.value.as_name_expr()?;
    if value.id.as_str() == "workflow" {
        Some(attr.attr.as_str())
    } else {
        None
    }
}

/// Find a decorated function whose decorators include @workflow.rule(name='expected_name')
/// or @workflow.checkpoint(name='expected_name').
fn find_rule_function<'a>(stmts: &'a [Stmt], rule_name: &str) -> Option<&'a StmtFunctionDef> {
    for stmt in stmts {
        if let Stmt::FunctionDef(func) = stmt {
            for dec in &func.decorator_list {
                let method = workflow_method_name(&dec.expression);
                if method == Some("rule") || method == Some("checkpoint") {
                    if decorator_has_kwarg_str(&dec.expression, "name", rule_name) {
                        return Some(func);
                    }
                }
            }
        }
    }
    None
}

/// Check whether a function has a decorator calling `@workflow.METHOD(...)`.
fn has_decorator(func: &StmtFunctionDef, method: &str) -> bool {
    func.decorator_list
        .iter()
        .any(|dec| workflow_decorator_name(&dec.expression) == Some(method))
}

/// Check if a call expression has a keyword argument with a string value.
fn decorator_has_kwarg_str(expr: &Expr, key: &str, value: &str) -> bool {
    let call = match expr.as_call_expr() {
        Some(c) => c,
        None => return false,
    };
    call.arguments.keywords.iter().any(|kw| {
        kw.arg.as_ref().is_some_and(|a| a.as_str() == key)
            && kw
                .value
                .as_string_literal_expr()
                .is_some_and(|s| s.value.to_str() == value)
    })
}

/// Find a statement that is a call to `workflow.METHOD(...)` at the top level.
fn find_workflow_call<'a>(stmts: &'a [Stmt], method: &str) -> Option<&'a ExprCall> {
    for stmt in stmts {
        if let Stmt::Expr(expr_stmt) = stmt {
            if let Some(call) = expr_stmt.value.as_call_expr() {
                if workflow_method_name(&expr_stmt.value) == Some(method) {
                    return Some(call);
                }
            }
        }
    }
    None
}

/// Find a decorated function whose decorators include `@workflow.METHOD`
/// (matching by decorator method name, not by rule name).
fn find_decorated_function<'a>(
    stmts: &'a [Stmt],
    decorator_method: &str,
) -> Option<&'a StmtFunctionDef> {
    for stmt in stmts {
        if let Stmt::FunctionDef(func) = stmt {
            if has_decorator(func, decorator_method) {
                return Some(func);
            }
        }
    }
    None
}

/// Count how many string arguments a call expression has.
fn count_string_args(call: &ExprCall) -> usize {
    call.arguments
        .args
        .iter()
        .filter(|a| a.is_string_literal_expr())
        .count()
}

// ============================================================
// Tests
// ============================================================

#[test]
fn compile_simple_rule() {
    let source = "rule align:\n    input: \"reads.fq\"\n    output: \"aligned.bam\"\n    shell: \"bwa mem {input} > {output}\"\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    let func = find_rule_function(&stmts, "align").expect("should find rule 'align'");
    assert!(has_decorator(func, "input"), "should have @workflow.input");
    assert!(
        has_decorator(func, "output"),
        "should have @workflow.output"
    );
    assert!(
        has_decorator(func, "shellcmd"),
        "should have @workflow.shellcmd"
    );
    assert!(
        has_decorator(func, "run"),
        "shell rules should have @workflow.run"
    );
    assert!(
        !has_decorator(func, "norun"),
        "shell rules should NOT have @workflow.norun"
    );

    // Function name should contain the rule name
    assert!(
        func.name.as_str().contains("align"),
        "function name should contain 'align', got '{}'",
        func.name.as_str()
    );

    // Function body should contain a shell() call
    let has_shell_call = func.body.iter().any(|s| {
        if let Stmt::Expr(expr_stmt) = s {
            if let Some(call) = expr_stmt.value.as_call_expr() {
                if let Some(name) = call.func.as_name_expr() {
                    return name.id.as_str() == "shell";
                }
            }
        }
        false
    });
    assert!(has_shell_call, "shell rule body should contain shell() call");
}

#[test]
fn compile_produces_valid_python() {
    let source = "rule foo:\n    input: \"a.txt\", \"b.txt\"\n    output: \"c.txt\"\n    threads: 4\n    shell: \"cat {input} > {output}\"\n";
    let result = compile(source, "Snakefile").unwrap();
    let parsed = parse_unchecked(&result.python, Mode::Module.into());
    assert!(
        parsed.errors().is_empty(),
        "compiled output should be valid Python, got errors: {:?}\n\nOutput:\n{}",
        parsed.errors(),
        result.python
    );
}

#[test]
fn compile_python_passthrough() {
    let source = "x = 42\nSAMPLES = [\"a\", \"b\"]\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    // Should have 2 statements, both assignments
    assert_eq!(stmts.len(), 2, "should have 2 pass-through statements");
    assert!(
        matches!(&stmts[0], Stmt::Assign(_)),
        "first stmt should be assignment"
    );
    assert!(
        matches!(&stmts[1], Stmt::Assign(_)),
        "second stmt should be assignment"
    );
}

#[test]
fn compile_run_block() {
    let source = "rule process:\n    input: \"data.csv\"\n    run:\n        import pandas as pd\n        df = pd.read_csv(input[0])\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    let func = find_rule_function(&stmts, "process").expect("should find rule 'process'");
    assert!(
        has_decorator(func, "run"),
        "should have @workflow.run decorator"
    );
    assert!(
        has_decorator(func, "input"),
        "should have @workflow.input decorator"
    );
    assert!(
        !has_decorator(func, "norun"),
        "should NOT have @workflow.norun with run block"
    );

    // The function body should not be just `pass`
    let body_len = func.body.len();
    assert!(
        body_len >= 1,
        "run block body should have at least 1 statement, got {body_len}"
    );

    // At least one statement should be an import
    let has_import = func.body.iter().any(|s| matches!(s, Stmt::Import(_)));
    assert!(
        has_import,
        "run block body should contain an import statement"
    );
}

#[test]
fn compile_checkpoint() {
    let source = "checkpoint gather_results:\n    input: \"data.csv\"\n    output: directory(\"results/\")\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    // Should find a function with @workflow.rule(name='gather_results', ..., checkpoint=True)
    let func = find_rule_function(&stmts, "gather_results")
        .expect("should find checkpoint 'gather_results'");
    // Checkpoint now uses @workflow.rule(..., checkpoint=True), not @workflow.checkpoint
    assert!(
        has_decorator(func, "rule"),
        "checkpoint should use @workflow.rule decorator"
    );
    // Verify the checkpoint=True kwarg is present
    let has_checkpoint_kwarg = func.decorator_list.iter().any(|dec| {
        if let Some(call) = dec.expression.as_call_expr() {
            call.arguments.keywords.iter().any(|kw| {
                kw.arg.as_ref().is_some_and(|a| a.as_str() == "checkpoint")
                    && kw
                        .value
                        .as_boolean_literal_expr()
                        .is_some_and(|b| b.value)
            })
        } else {
            false
        }
    });
    assert!(
        has_checkpoint_kwarg,
        "should have checkpoint=True in @workflow.rule decorator"
    );
    assert!(has_decorator(func, "input"), "should have @workflow.input");
    assert!(
        has_decorator(func, "output"),
        "should have @workflow.output"
    );
}

#[test]
fn compile_global_configfile() {
    let source = "configfile: \"config.yaml\"\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    let call =
        find_workflow_call(&stmts, "configfile").expect("should find workflow.configfile() call");
    assert!(
        count_string_args(call) >= 1,
        "configfile call should have at least one string argument"
    );
}

#[test]
fn compile_ruleorder() {
    let source = "ruleorder: align > sort > index\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    let call =
        find_workflow_call(&stmts, "ruleorder").expect("should find workflow.ruleorder() call");
    assert_eq!(
        count_string_args(call),
        3,
        "ruleorder should have 3 string arguments"
    );
}

#[test]
fn compile_handler() {
    let source = "onsuccess:\n    print(\"done!\")\n";
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    let func = find_decorated_function(&stmts, "onsuccess")
        .expect("should find @workflow.onsuccess decorated function");

    // Function body should have at least one statement
    assert!(
        !func.body.is_empty(),
        "handler body should have at least one statement"
    );

    // The function should have a `log` parameter
    let has_log_param = func
        .parameters
        .as_ref()
        .args
        .iter()
        .any(|p| p.parameter.name.as_str() == "log");
    assert!(
        has_log_param,
        "handler function should accept 'log' parameter"
    );
}

#[test]
fn compile_mixed_workflow() {
    let source = concat!(
        "configfile: \"config.yaml\"\n",
        "\n",
        "SAMPLES = [\"a\", \"b\"]\n",
        "\n",
        "rule all:\n",
        "    input: \"result.txt\"\n",
        "\n",
        "rule process:\n",
        "    input: \"data.csv\"\n",
        "    output: \"result.txt\"\n",
        "    shell: \"process {input} > {output}\"\n",
    );
    let result = compile(source, "Snakefile").unwrap();
    let stmts = parse_compiled(&result.python);

    // Should have: workflow.configfile call, assignment, two rule functions
    let configfile_call = find_workflow_call(&stmts, "configfile");
    assert!(
        configfile_call.is_some(),
        "should have workflow.configfile()"
    );

    let all_func = find_rule_function(&stmts, "all");
    assert!(all_func.is_some(), "should find rule 'all'");

    let process_func = find_rule_function(&stmts, "process");
    assert!(process_func.is_some(), "should find rule 'process'");

    // There should be an assignment statement (SAMPLES = ...)
    let has_assignment = stmts.iter().any(|s| matches!(s, Stmt::Assign(_)));
    assert!(has_assignment, "should have a pass-through assignment");
}

#[test]
fn source_map_has_entries() {
    let source = "rule foo:\n    input: \"a.txt\"\n    shell: \"echo hi\"\n";
    let result = compile(source, "Snakefile").unwrap();
    assert!(
        !result.source_map.mappings.is_empty(),
        "source map should have entries after compilation"
    );
}

#[test]
fn compile_real_workflow_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/real_workflow.smk").unwrap();
    let result = compile(&source, "real_workflow.smk").unwrap();
    let stmts = parse_compiled(&result.python);

    // All 6 rules should be present
    let rule_names = ["all", "fastqc", "trim", "align", "count", "multiqc"];
    for name in &rule_names {
        assert!(
            find_rule_function(&stmts, name).is_some(),
            "should find rule '{name}' in compiled output"
        );
    }

    // Should have configfile call
    assert!(
        find_workflow_call(&stmts, "configfile").is_some(),
        "should have workflow.configfile()"
    );

    // Should have onsuccess and onerror handlers
    assert!(
        find_decorated_function(&stmts, "onsuccess").is_some(),
        "should have @workflow.onsuccess handler"
    );
    assert!(
        find_decorated_function(&stmts, "onerror").is_some(),
        "should have @workflow.onerror handler"
    );

    // Python pass-through (SAMPLES = ..., GENOME = ...)
    let assign_count = stmts
        .iter()
        .filter(|s| matches!(s, Stmt::Assign(_)))
        .count();
    assert!(
        assign_count >= 2,
        "should have at least 2 assignment statements, got {assign_count}"
    );

    // The align rule should have threads, resources, log, conda, shell decorators
    let align = find_rule_function(&stmts, "align").unwrap();
    assert!(has_decorator(align, "threads"), "align should have threads");
    assert!(
        has_decorator(align, "resources"),
        "align should have resources"
    );
    assert!(has_decorator(align, "log"), "align should have log");
    assert!(has_decorator(align, "conda"), "align should have conda");
    assert!(
        has_decorator(align, "shellcmd"),
        "align should have shellcmd"
    );
}

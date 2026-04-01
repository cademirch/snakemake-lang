//! Virtual Python generator.
//!
//! Walks the Snakemake AST and emits valid Python that, when exec'd,
//! produces the same side effects as parser.py's output (rule registration,
//! global directives, etc.).

use ruff_text_size::Ranged;

use super::source_map::{SourceMap, SourceMapping};
use crate::ast::{
    DirectiveKeyword, DirectiveValue, GlobalKeyword, ModuleKeyword, RuleNames,
    Snakefile, SnakemakeDirective, SnakemakeGlobalDirective, SnakemakeHandler, SnakemakeLocalrules,
    SnakemakeModule, SnakemakeRule, SnakemakeRuleorder, SnakemakeStorage, SnakemakeUseRule,
    Statement,
};

/// The standard function signature for rule functions in Snakemake's runtime.
const RULE_PARAMS: &str = "input, output, params, wildcards, threads, resources, log, version, rule, conda_env, container_img, singularity_args, use_singularity, env_modules, bench_record, jobid, is_shell, bench_iteration, cleanup_scripts, shadow_dir, edit_notebook, conda_base_path, basedir, sourcecache_path, runtime_sourcecache_path";

/// Generates virtual Python from a Snakemake AST.
pub struct VirtualPythonGenerator<'a> {
    /// The original Snakemake source (for extracting expression text).
    source: &'a str,

    /// The file path (for `snakefile=` arguments in generated code).
    path: &'a str,

    /// The generated Python output.
    output: String,

    /// Source map being built.
    source_map: SourceMap,
}

impl<'a> VirtualPythonGenerator<'a> {
    pub fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            output: String::new(),
            source_map: SourceMap::new(),
        }
    }

    /// Emit text that maps back to original source.
    pub fn emit_mapped(&mut self, text: &str, original_start: usize, original_len: usize) {
        let gen_start = self.output.len();
        self.output.push_str(text);
        self.source_map.push(SourceMapping {
            generated_start: gen_start,
            generated_len: text.len(),
            original_start,
            original_len,
        });
    }

    /// Emit synthetic text with no original source mapping.
    pub fn emit(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// Consume the generator and return the output + source map.
    pub fn finish(self) -> (String, SourceMap) {
        (self.output, self.source_map)
    }

    /// 1-based line number for a byte offset in the original source.
    fn line_at(&self, offset: usize) -> usize {
        self.source[..offset].matches('\n').count() + 1
    }

    /// Extract original source text for a directive's argument range.
    fn extract_value_text(&self, directive: &SnakemakeDirective) -> &str {
        match &directive.value {
            DirectiveValue::Arguments(args) => {
                let start = args.range.start().to_u32() as usize;
                let end = args.range.end().to_u32() as usize;
                if start >= end || end > self.source.len() {
                    ""
                } else {
                    self.source[start..end].trim()
                }
            }
            DirectiveValue::Block(_) => "",
        }
    }

    /// Map a directive keyword to its workflow method name.
    fn directive_method(keyword: DirectiveKeyword) -> &'static str {
        match keyword {
            DirectiveKeyword::Input => "input",
            DirectiveKeyword::Output => "output",
            DirectiveKeyword::Params => "params",
            DirectiveKeyword::Log => "log",
            DirectiveKeyword::Benchmark => "benchmark",
            DirectiveKeyword::Shell => "shellcmd",
            DirectiveKeyword::Script => "script",
            DirectiveKeyword::Notebook => "notebook",
            DirectiveKeyword::Wrapper => "wrapper",
            DirectiveKeyword::TemplateEngine => "template_engine",
            DirectiveKeyword::Cwl => "cwl",
            DirectiveKeyword::Threads => "threads",
            DirectiveKeyword::Resources => "resources",
            DirectiveKeyword::Retries => "retries",
            DirectiveKeyword::Priority => "priority",
            DirectiveKeyword::Conda => "conda",
            DirectiveKeyword::Container => "container",
            DirectiveKeyword::Containerized => "containerized",
            DirectiveKeyword::EnvModules => "envmodules",
            DirectiveKeyword::Shadow => "shadow",
            DirectiveKeyword::Message => "message",
            DirectiveKeyword::WildcardConstraints => "wildcard_constraints",
            DirectiveKeyword::Group => "group",
            DirectiveKeyword::Name => "name",
            DirectiveKeyword::Cache => "cache",
            DirectiveKeyword::DefaultTarget => "default_target",
            DirectiveKeyword::Handover => "handover",
            DirectiveKeyword::Localrule => "localrule",
            DirectiveKeyword::Pathvars => "pathvars",
            DirectiveKeyword::Run => "run",
        }
    }

    /// Map a global keyword to its workflow method name.
    fn global_method(keyword: GlobalKeyword) -> &'static str {
        match keyword {
            GlobalKeyword::Configfile => "configfile",
            GlobalKeyword::Include => "include",
            GlobalKeyword::Workdir => "workdir",
            GlobalKeyword::Envvars => "envvars",
            GlobalKeyword::Pathvars => "pathvars",
            GlobalKeyword::Pepfile => "pepfile",
            GlobalKeyword::Pepschema => "pepschema",
            GlobalKeyword::Report => "report",
            GlobalKeyword::Scattergather => "scattergather",
            GlobalKeyword::WildcardConstraints => "wildcard_constraints",
            GlobalKeyword::Container => "container",
            GlobalKeyword::Containerized => "containerized",
            GlobalKeyword::Conda => "conda",
            GlobalKeyword::ResourceScopes => "resource_scopes",
            GlobalKeyword::InputFlags => "inputflags",
            GlobalKeyword::OutputFlags => "outputflags",
        }
    }

    /// Map a module keyword to its argument name.
    fn module_method(keyword: ModuleKeyword) -> &'static str {
        match keyword {
            ModuleKeyword::Snakefile => "snakefile",
            ModuleKeyword::MetaWrapper => "meta_wrapper",
            ModuleKeyword::Config => "config",
            ModuleKeyword::SkipValidation => "skip_validation",
            ModuleKeyword::ReplacePrefix => "replace_prefix",
            ModuleKeyword::Prefix => "prefix",
            ModuleKeyword::Name => "name",
            ModuleKeyword::Pathvars => "pathvars",
        }
    }

    // ================================================================
    // Top-level generation
    // ================================================================

    /// Generate virtual Python for an entire Snakefile.
    pub fn generate(&mut self, ast: &Snakefile) {
        for stmt in &ast.body {
            match stmt {
                Statement::Rule(rule) => self.emit_rule(rule),
                Statement::Module(module) => self.emit_module(module),
                Statement::UseRule(use_rule) => self.emit_use_rule(use_rule),
                Statement::GlobalDirective(gd) => self.emit_global_directive(gd),
                Statement::Ruleorder(ro) => self.emit_ruleorder(ro),
                Statement::Localrules(lr) => self.emit_localrules(lr),
                Statement::Storage(st) => self.emit_storage(st),
                Statement::Handler(h) => self.emit_handler(h),
                Statement::Python(py_stmt, chunk_offset) => self.emit_python_stmt(py_stmt, *chunk_offset),
            }
        }
    }

    // ================================================================
    // Rule / Checkpoint
    // ================================================================

    fn emit_rule(&mut self, rule: &SnakemakeRule) {
        let name = rule.name.as_str();
        let lineno = self.line_at(rule.range.start().to_u32() as usize);
        let kind = if rule.is_checkpoint {
            "checkpoint"
        } else {
            "rule"
        };

        // @workflow.rule(...) or @workflow.checkpoint(...)
        self.emit(&format!(
            "@workflow.{kind}(name='{name}', lineno={lineno}, snakefile='{path}')\n",
            path = self.path,
        ));

        // Emit directives as decorator calls (all except run)
        let mut has_run = false;
        let mut has_execution = false;

        for directive in &rule.directives {
            if directive.keyword == DirectiveKeyword::Run {
                has_run = true;
                has_execution = true;
                continue;
            }
            if directive.keyword.is_execution() {
                has_execution = true;
            }
            self.emit_directive_decorator(directive);
        }

        if has_run {
            // @workflow.run
            self.emit("@workflow.run\n");
        } else if !has_execution {
            // No execution directive: add @workflow.norun()
            self.emit("@workflow.norun()\n");
        } else {
            // Has a non-run execution directive (shell, script, etc.)
            // These are already emitted as decorators above.
            // Still need @workflow.norun() since the function body is pass.
            self.emit("@workflow.norun()\n");
        }

        // Function definition
        self.emit(&format!(
            "def __rule_{name}({RULE_PARAMS}, __is_snakemake_rule_func=True):\n"
        ));

        if has_run {
            // Emit the run block body
            for directive in &rule.directives {
                if directive.keyword == DirectiveKeyword::Run {
                    self.emit_run_block(directive);
                    break;
                }
            }
        } else {
            self.emit("    pass\n");
        }

        self.emit("\n");
    }

    /// Emit a single directive as a `@workflow.method(value)` decorator line.
    fn emit_directive_decorator(&mut self, directive: &SnakemakeDirective) {
        let method = Self::directive_method(directive.keyword);
        let value_text = self.extract_value_text(directive).to_owned();

        let orig_start = directive.range.start().to_u32() as usize;
        let orig_len = directive.range.end().to_u32() as usize - orig_start;

        self.emit(&format!("@workflow.{method}("));
        if !value_text.is_empty() {
            self.emit_mapped(&value_text, orig_start, orig_len);
        }
        self.emit(")\n");
    }

    /// Emit the body of a `run:` block inside a rule function.
    fn emit_run_block(&mut self, directive: &SnakemakeDirective) {
        match &directive.value {
            DirectiveValue::Block(stmts) => {
                if stmts.is_empty() {
                    self.emit("    pass\n");
                    return;
                }

                // Extract original source for the run block body.
                // The directive range covers from `run:` through the block end.
                let dir_start = directive.range.start().to_u32() as usize;
                let dir_end = directive.range.end().to_u32() as usize;

                // Find the start of the block body (after "run:\n")
                let run_line_end = self.source[dir_start..]
                    .find('\n')
                    .map(|i| dir_start + i + 1)
                    .unwrap_or(dir_end);

                if run_line_end >= dir_end {
                    self.emit("    pass\n");
                    return;
                }

                let block_source = &self.source[run_line_end..dir_end];

                // Find minimum indentation of non-empty lines in the block
                let min_indent = block_source
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.len() - l.trim_start_matches(' ').len())
                    .min()
                    .unwrap_or(0);

                // Re-indent to 4 spaces (function body indent)
                for line in block_source.lines() {
                    if line.trim().is_empty() {
                        self.emit("\n");
                    } else {
                        let stripped = if line.len() > min_indent {
                            &line[min_indent..]
                        } else {
                            line.trim_start()
                        };
                        self.emit("    ");
                        // Map the stripped content back to original source
                        let line_start_in_source =
                            run_line_end + (line.as_ptr() as usize - block_source.as_ptr() as usize);
                        let orig_content_start = line_start_in_source + (line.len() - stripped.len());
                        self.emit_mapped(stripped, orig_content_start, stripped.len());
                        self.emit("\n");
                    }
                }
            }
            DirectiveValue::Arguments(_) => {
                self.emit("    pass\n");
            }
        }
    }

    // ================================================================
    // Module
    // ================================================================

    fn emit_module(&mut self, module: &SnakemakeModule) {
        let name = module.name.as_str();

        // Build kwargs from module directives
        let mut kwargs = Vec::new();
        for dir in &module.directives {
            let method = Self::module_method(dir.keyword);
            match &dir.value {
                DirectiveValue::Arguments(args) => {
                    let start = args.range.start().to_u32() as usize;
                    let end = args.range.end().to_u32() as usize;
                    if start < end && end <= self.source.len() {
                        let text = self.source[start..end].trim();
                        kwargs.push(format!("{method}={text}"));
                    }
                }
                DirectiveValue::Block(_) => {}
            }
        }

        let orig_start = module.range.start().to_u32() as usize;
        let orig_len = module.range.end().to_u32() as usize - orig_start;

        self.emit("workflow.module(");
        self.emit_mapped(
            &format!("'{name}'"),
            orig_start,
            orig_len,
        );
        if !kwargs.is_empty() {
            self.emit(", ");
            self.emit(&kwargs.join(", "));
        }
        self.emit(")\n");
    }

    // ================================================================
    // Use Rule
    // ================================================================

    fn emit_use_rule(&mut self, use_rule: &SnakemakeUseRule) {
        let orig_start = use_rule.range.start().to_u32() as usize;
        let orig_len = use_rule.range.end().to_u32() as usize - orig_start;

        // Build rules list
        let rules_str = match &use_rule.rules {
            RuleNames::All => "'*'".to_string(),
            RuleNames::Named(names) => {
                let quoted: Vec<String> = names.iter().map(|n| format!("'{}'", n.as_str())).collect();
                format!("[{}]", quoted.join(", "))
            }
        };

        let from_module = use_rule.from_module.as_str();

        // Build exclude list
        let exclude_str = if use_rule.exclude.is_empty() {
            String::new()
        } else {
            let quoted: Vec<String> = use_rule
                .exclude
                .iter()
                .map(|n| format!("'{}'", n.as_str()))
                .collect();
            format!(", exclude=[{}]", quoted.join(", "))
        };

        // Name modifier
        let name_mod_str = match &use_rule.name_modifier {
            Some(pattern) => format!(", name_modifier='{pattern}'"),
            None => String::new(),
        };

        self.emit_mapped(
            &format!(
                "@workflow.userule(rules={rules_str}, from_module='{from_module}'{exclude_str}{name_mod_str})"
            ),
            orig_start,
            orig_len,
        );
        self.emit("\n");

        // Emit with-directives if present
        if let Some(directives) = &use_rule.with_directives {
            for directive in directives {
                if directive.keyword == DirectiveKeyword::Run {
                    continue;
                }
                self.emit_directive_decorator(directive);
            }
        }

        self.emit("def _use_rule():\n    pass\n\n");
    }

    // ================================================================
    // Global Directives
    // ================================================================

    fn emit_global_directive(&mut self, gd: &SnakemakeGlobalDirective) {
        let method = Self::global_method(gd.keyword);

        match &gd.value {
            DirectiveValue::Arguments(args) => {
                let start = args.range.start().to_u32() as usize;
                let end = args.range.end().to_u32() as usize;
                let value_text = if start < end && end <= self.source.len() {
                    self.source[start..end].trim()
                } else {
                    ""
                };

                let orig_start = gd.range.start().to_u32() as usize;
                let orig_len = gd.range.end().to_u32() as usize - orig_start;

                self.emit(&format!("workflow.{method}("));
                if !value_text.is_empty() {
                    self.emit_mapped(value_text, orig_start, orig_len);
                }
                self.emit(")\n");
            }
            DirectiveValue::Block(_) => {
                self.emit(&format!("workflow.{method}()\n"));
            }
        }
    }

    // ================================================================
    // Ruleorder / Localrules
    // ================================================================

    fn emit_ruleorder(&mut self, ro: &SnakemakeRuleorder) {
        let quoted: Vec<String> = ro.names.iter().map(|n| format!("'{}'", n.as_str())).collect();
        let orig_start = ro.range.start().to_u32() as usize;
        let orig_len = ro.range.end().to_u32() as usize - orig_start;

        self.emit("workflow.ruleorder(");
        self.emit_mapped(&quoted.join(", "), orig_start, orig_len);
        self.emit(")\n");
    }

    fn emit_localrules(&mut self, lr: &SnakemakeLocalrules) {
        let quoted: Vec<String> = lr.names.iter().map(|n| format!("'{}'", n.as_str())).collect();
        let orig_start = lr.range.start().to_u32() as usize;
        let orig_len = lr.range.end().to_u32() as usize - orig_start;

        self.emit("workflow.localrules(");
        self.emit_mapped(&quoted.join(", "), orig_start, orig_len);
        self.emit(")\n");
    }

    // ================================================================
    // Storage
    // ================================================================

    fn emit_storage(&mut self, st: &SnakemakeStorage) {
        let tag = st.tag.as_str();

        match &st.value {
            DirectiveValue::Arguments(args) => {
                let start = args.range.start().to_u32() as usize;
                let end = args.range.end().to_u32() as usize;
                let value_text = if start < end && end <= self.source.len() {
                    self.source[start..end].trim()
                } else {
                    ""
                };

                let orig_start = st.range.start().to_u32() as usize;
                let orig_len = st.range.end().to_u32() as usize - orig_start;

                self.emit(&format!("workflow.storage('{tag}', "));
                if !value_text.is_empty() {
                    self.emit_mapped(value_text, orig_start, orig_len);
                }
                self.emit(")\n");
            }
            DirectiveValue::Block(_) => {
                self.emit(&format!("workflow.storage('{tag}')\n"));
            }
        }
    }

    // ================================================================
    // Handlers
    // ================================================================

    fn emit_handler(&mut self, handler: &SnakemakeHandler) {
        let kind_str = handler.kind.as_str();
        let func_name = format!("__{kind_str}");

        let orig_start = handler.range.start().to_u32() as usize;
        let orig_len = handler.range.end().to_u32() as usize - orig_start;

        self.emit_mapped(
            &format!("@workflow.{kind_str}"),
            orig_start,
            orig_len,
        );
        self.emit("\n");

        self.emit(&format!("def {func_name}(log):\n"));

        if handler.body.is_empty() {
            self.emit("    pass\n");
        } else {
            // Extract the handler block body from original source
            let handler_start = handler.range.start().to_u32() as usize;
            let handler_end = handler.range.end().to_u32() as usize;

            // Find the start of the block body (after "onsuccess:\n" etc.)
            let header_end = self.source[handler_start..]
                .find('\n')
                .map(|i| handler_start + i + 1)
                .unwrap_or(handler_end);

            if header_end < handler_end {
                let block_source = &self.source[header_end..handler_end];

                let min_indent = block_source
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.len() - l.trim_start_matches(' ').len())
                    .min()
                    .unwrap_or(0);

                for line in block_source.lines() {
                    if line.trim().is_empty() {
                        self.emit("\n");
                    } else {
                        let stripped = if line.len() > min_indent {
                            &line[min_indent..]
                        } else {
                            line.trim_start()
                        };
                        self.emit("    ");
                        let line_start_in_source =
                            header_end + (line.as_ptr() as usize - block_source.as_ptr() as usize);
                        let orig_content_start = line_start_in_source + (line.len() - stripped.len());
                        self.emit_mapped(stripped, orig_content_start, stripped.len());
                        self.emit("\n");
                    }
                }
            } else {
                self.emit("    pass\n");
            }
        }
        self.emit("\n");
    }

    // ================================================================
    // Python pass-through
    // ================================================================

    fn emit_python_stmt(&mut self, stmt: &ruff_python_ast::Stmt, chunk_offset: ruff_text_size::TextSize) {
        let range = stmt.range();
        // ruff ranges are relative to the chunk that was parsed.
        // Add the chunk offset to get the position in the original source.
        let start = range.start().to_u32() as usize + chunk_offset.to_u32() as usize;
        let end = range.end().to_u32() as usize + chunk_offset.to_u32() as usize;

        let end = end.min(self.source.len());
        let start = start.min(end);

        let text = &self.source[start..end];
        self.emit_mapped(text, start, text.len());
        self.emit("\n");
    }
}

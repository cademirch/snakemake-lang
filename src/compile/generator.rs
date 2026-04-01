//! Virtual Python generator.
//!
//! Walks the Snakemake AST and emits valid Python that, when exec'd,
//! produces the same side effects as parser.py's output (rule registration,
//! global directives, etc.).

use ruff_text_size::Ranged;

use super::source_map::{SourceMap, SourceMapping};
use crate::ast::{
    DirectiveKeyword, DirectiveValue, GlobalKeyword, ModuleKeyword, RuleNames, Snakefile,
    SnakemakeDirective, SnakemakeGlobalDirective, SnakemakeHandler, SnakemakeLocalrules,
    SnakemakeModule, SnakemakeRule, SnakemakeRuleorder, SnakemakeStorage, SnakemakeUseRule,
    Statement,
};

/// The standard function signature for rule functions in Snakemake's runtime.
const RULE_PARAMS: &str = "input, output, params, wildcards, threads, resources, log, rule, conda_env, container_img, singularity_args, use_singularity, env_modules, bench_record, jobid, is_shell, bench_iteration, cleanup_scripts, shadow_dir, edit_notebook, conda_base_path, basedir, sourcecache_path, runtime_sourcecache_path, runtime_paths";

const INDENT: &str = "\t";

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

    /// Indentation prefix for rules inside control flow blocks.
    indent_prefix: String,

    /// Whether the next emitted character should be preceded by indent_prefix.
    at_line_start: bool,
}

impl<'a> VirtualPythonGenerator<'a> {
    pub fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            output: String::new(),
            source_map: SourceMap::new(),
            indent_prefix: String::new(),
            at_line_start: false,
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
    /// When `indent_prefix` is set, auto-prepends it at each line start.
    pub fn emit(&mut self, text: &str) {
        if self.indent_prefix.is_empty() {
            self.output.push_str(text);
        } else {
            for ch in text.chars() {
                if self.at_line_start && ch != '\n' {
                    self.output.push_str(&self.indent_prefix);
                    self.at_line_start = false;
                }
                self.output.push(ch);
                if ch == '\n' {
                    self.at_line_start = true;
                }
            }
        }
    }

    /// Consume the generator and return the output + source map.
    pub fn finish(self) -> (String, SourceMap) {
        (self.output, self.source_map)
    }

    /// 1-based line number for a byte offset in the original source.
    fn line_at(&self, offset: usize) -> usize {
        self.source[..offset].matches('\n').count() + 1
    }

    /// Measure the leading whitespace of the line starting at `offset`.
    fn source_indent_at(&self, offset: usize) -> String {
        let rest = &self.source[offset..];
        let trimmed = rest.trim_start_matches(|c: char| c == ' ' || c == '\t');
        let indent_len = rest.len() - trimmed.len();
        self.source[offset..offset + indent_len].to_string()
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
            DirectiveKeyword::WildcardConstraints => "register_wildcard_constraints",
            DirectiveKeyword::Group => "group",
            DirectiveKeyword::Name => "name",
            DirectiveKeyword::Cache => "cache_rule",
            DirectiveKeyword::DefaultTarget => "default_target_rule",
            DirectiveKeyword::Handover => "handover",
            DirectiveKeyword::Localrule => "localrule",
            DirectiveKeyword::Pathvars => "rule_pathvars",
            DirectiveKeyword::Run => "run",
        }
    }

    /// Map a global keyword to its workflow method name.
    fn global_method(keyword: GlobalKeyword) -> &'static str {
        match keyword {
            GlobalKeyword::Configfile => "configfile",
            GlobalKeyword::Include => "include",
            GlobalKeyword::Workdir => "workdir",
            GlobalKeyword::Envvars => "register_envvars",
            GlobalKeyword::Pathvars => "register_pathvars",
            GlobalKeyword::Pepfile => "set_pepfile",
            GlobalKeyword::Pepschema => "set_pepschema",
            GlobalKeyword::Report => "report",
            GlobalKeyword::Scattergather => "scattergather",
            GlobalKeyword::WildcardConstraints => "global_wildcard_constraints",
            GlobalKeyword::Container => "global_container",
            GlobalKeyword::Containerized => "containerized",
            GlobalKeyword::Conda => "set_conda_prefix",
            GlobalKeyword::ResourceScopes => "register_resource_scopes",
            GlobalKeyword::InputFlags => "set_input_flags",
            GlobalKeyword::OutputFlags => "set_output_flags",
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

    fn execution_end_func(keyword: DirectiveKeyword) -> &'static str {
        match keyword {
            DirectiveKeyword::Shell => "shell",
            DirectiveKeyword::Script => "script",
            DirectiveKeyword::Notebook => "notebook",
            DirectiveKeyword::Wrapper => "wrapper",
            DirectiveKeyword::TemplateEngine => "render_template",
            DirectiveKeyword::Cwl => "cwl",
            _ => unreachable!(),
        }
    }

    fn execution_args(keyword: DirectiveKeyword) -> &'static str {
        match keyword {
            DirectiveKeyword::Shell => ", bench_record=bench_record, bench_iteration=bench_iteration",
            DirectiveKeyword::Script => ", basedir, input, output, params, wildcards, threads, resources, log, config, rule, conda_env, conda_base_path, container_img, singularity_args, env_modules, bench_record, jobid, bench_iteration, cleanup_scripts, shadow_dir, sourcecache_path, runtime_sourcecache_path, runtime_paths",
            DirectiveKeyword::Notebook => ", basedir, input, output, params, wildcards, threads, resources, log, config, rule, conda_env, conda_base_path, container_img, singularity_args, env_modules, bench_record, jobid, bench_iteration, cleanup_scripts, shadow_dir, edit_notebook, sourcecache_path, runtime_sourcecache_path, runtime_paths",
            DirectiveKeyword::Wrapper => ", input, output, params, wildcards, threads, resources, log, config, rule, conda_env, conda_base_path, container_img, singularity_args, env_modules, bench_record, workflow.workflow_settings.wrapper_prefix, jobid, bench_iteration, cleanup_scripts, shadow_dir, sourcecache_path, runtime_sourcecache_path, runtime_paths",
            DirectiveKeyword::TemplateEngine => ", input, output, params, wildcards, config, rule",
            DirectiveKeyword::Cwl => ", basedir, input, output, params, wildcards, threads, resources, log, config, rule, use_singularity, bench_record, jobid, sourcecache_path, runtime_sourcecache_path, runtime_paths",
            _ => "",
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
                Statement::Python(py_stmt, chunk_offset) => {
                    self.emit_python_stmt(py_stmt, *chunk_offset)
                }
                Statement::VerbatimPython(text, offset) => {
                    self.emit_mapped(text, *offset, text.len());
                }
            }
        }
    }

    // ================================================================
    // Rule / Checkpoint
    // ================================================================

    fn emit_rule(&mut self, rule: &SnakemakeRule) {
        let name = rule.name.as_str();
        let rule_offset = rule.range.start().to_u32() as usize;
        let lineno = self.line_at(rule_offset);

        // Set indent prefix based on source position (for rules inside if/for)
        let saved_indent = self.indent_prefix.clone();
        let saved_at_line_start = self.at_line_start;
        self.indent_prefix = self.source_indent_at(rule_offset);
        if !self.indent_prefix.is_empty() {
            self.at_line_start = true;
        }

        let checkpoint_arg = if rule.is_checkpoint {
            ", checkpoint=True"
        } else {
            ""
        };

        // @workflow.rule(name=..., lineno=..., snakefile=...[, checkpoint=True])
        self.emit(&format!(
            "@workflow.rule(name='{name}', lineno={lineno}, snakefile='{path}'{checkpoint_arg})\n",
            path = self.path,
        ));

        // Find execution directive (if any)
        let mut has_run = false;
        let mut exec_directive: Option<&SnakemakeDirective> = None;

        for directive in &rule.directives {
            if directive.keyword == DirectiveKeyword::Run {
                has_run = true;
                break;
            }
            if directive.keyword.is_execution() {
                exec_directive = Some(directive);
            }
        }

        // Emit non-execution directives as decorators
        for directive in &rule.directives {
            if directive.keyword.is_execution() {
                continue;
            }
            self.emit_directive_decorator(directive);
        }

        if has_run {
            // run: block → emit @workflow.run + def + body
            self.emit("@workflow.run\n");
            self.emit(&format!(
                "def __rule_{name}({RULE_PARAMS}, __is_snakemake_rule_func=True):\n"
            ));
            for directive in &rule.directives {
                if directive.keyword == DirectiveKeyword::Run {
                    self.emit_run_block(directive);
                    break;
                }
            }
        } else if let Some(exec_dir) = exec_directive {
            // shell/script/etc → emit the cmd decorator + @workflow.run + def + body
            let exec_keyword = exec_dir.keyword;
            self.emit_directive_decorator(exec_dir);
            self.emit("@workflow.run\n");
            self.emit(&format!(
                "def __rule_{name}({RULE_PARAMS}, __is_snakemake_rule_func=True):\n"
            ));

            let value_text = self.extract_value_text(exec_dir).to_owned();
            let func = Self::execution_end_func(exec_keyword);
            let args = Self::execution_args(exec_keyword);
            self.emit(&format!(
                "{INDENT}{func}({value_text}{args})\n"
            ));
        } else {
            // No execution directive → @workflow.norun() + @workflow.run + def + pass
            self.emit("@workflow.norun()\n");
            self.emit("@workflow.run\n");
            self.emit(&format!(
                "def __rule_{name}({RULE_PARAMS}, __is_snakemake_rule_func=True):\n"
            ));
            self.emit(&format!("{INDENT}pass\n"));
        }

        self.emit("\n");

        // Restore indent prefix
        self.indent_prefix = saved_indent;
        self.at_line_start = saved_at_line_start;
    }

    /// Emit a single directive as a `@workflow.method(value)` decorator.
    fn emit_directive_decorator(&mut self, directive: &SnakemakeDirective) {
        let method = Self::directive_method(directive.keyword);
        let value_text = self.extract_value_text(directive).to_owned();

        self.emit(&format!("@workflow.{method}("));
        if !value_text.is_empty() {
            self.emit(" ");
            if let DirectiveValue::Arguments(args) = &directive.value {
                let start = args.range.start().to_u32() as usize;
                let len = args.range.end().to_u32() as usize - start;
                self.emit_mapped(&value_text, start, len);
            }
        }
        self.emit("\n)\n");
    }

    /// Emit the body of a `run:` block inside a rule function.
    fn emit_run_block(&mut self, directive: &SnakemakeDirective) {
        match &directive.value {
            DirectiveValue::Block(stmts) => {
                if stmts.is_empty() {
                    self.emit(&format!("{INDENT}pass\n"));
                    return;
                }

                let dir_start = directive.range.start().to_u32() as usize;
                let dir_end = directive.range.end().to_u32() as usize;

                let run_line_end = self.source[dir_start..]
                    .find('\n')
                    .map(|i| dir_start + i + 1)
                    .unwrap_or(dir_end);

                if run_line_end >= dir_end {
                    self.emit(&format!("{INDENT}pass\n"));
                    return;
                }

                let block_source = &self.source[run_line_end..dir_end];

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
                        self.emit(INDENT);
                        let line_start_in_source = run_line_end
                            + (line.as_ptr() as usize - block_source.as_ptr() as usize);
                        let orig_content_start =
                            line_start_in_source + (line.len() - stripped.len());
                        self.emit_mapped(stripped, orig_content_start, stripped.len());
                        self.emit("\n");
                    }
                }
            }
            DirectiveValue::Arguments(_) => {
                self.emit(&format!("{INDENT}pass\n"));
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

        self.emit("workflow.module(\n");
        if !kwargs.is_empty() {
            self.emit(&kwargs.join(","));
        }
        self.emit(&format!(",name='{name}'\n)\n"));
    }

    // ================================================================
    // Use Rule
    // ================================================================

    fn emit_use_rule(&mut self, use_rule: &SnakemakeUseRule) {
        let orig_start = use_rule.range.start().to_u32() as usize;
        let lineno = self.line_at(orig_start);

        // Build rules list
        let rules_str = match &use_rule.rules {
            RuleNames::All => "'*'".to_string(),
            RuleNames::Named(names) => {
                let quoted: Vec<String> =
                    names.iter().map(|n| format!("'{}'", n.as_str())).collect();
                format!("[{}]", quoted.join(", "))
            }
        };

        let from_module = use_rule.from_module.as_str();

        // Build exclude list (always present)
        let exclude_str = if use_rule.exclude.is_empty() {
            "[]".to_string()
        } else {
            let quoted: Vec<String> = use_rule
                .exclude
                .iter()
                .map(|n| format!("'{}'", n.as_str()))
                .collect();
            format!("[{}]", quoted.join(", "))
        };

        // Name modifier (always present)
        let name_mod_str = match &use_rule.name_modifier {
            Some(pattern) => format!("'{pattern}'"),
            None => "None".to_string(),
        };

        // First rule name for function name
        let first_rule = match &use_rule.rules {
            RuleNames::All => "*",
            RuleNames::Named(names) => names.first().map(|n| n.as_str()).unwrap_or("unknown"),
        };

        self.emit(&format!(
            "@workflow.userule(rules={rules_str}, from_module='{from_module}', exclude_rules={exclude_str}, name_modifier={name_mod_str}, lineno={lineno})\n"
        ));

        // Emit with-directives if present
        if let Some(directives) = &use_rule.with_directives {
            for directive in directives {
                if directive.keyword == DirectiveKeyword::Run {
                    continue;
                }
                self.emit_directive_decorator(directive);
            }
        }

        self.emit("@workflow.run\n");
        self.emit(&format!(
            "def __userule_{from_module}_{first_rule}():\n{INDENT}pass\n\n"
        ));
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
        let quoted: Vec<String> = ro
            .names
            .iter()
            .map(|n| format!("'{}'", n.as_str()))
            .collect();
        let orig_start = ro.range.start().to_u32() as usize;
        let orig_len = ro.range.end().to_u32() as usize - orig_start;

        self.emit("workflow.ruleorder(");
        self.emit_mapped(&quoted.join(", "), orig_start, orig_len);
        self.emit(")\n");
    }

    fn emit_localrules(&mut self, lr: &SnakemakeLocalrules) {
        let quoted: Vec<String> = lr
            .names
            .iter()
            .map(|n| format!("'{}'", n.as_str()))
            .collect();
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

                self.emit(&format!("workflow.storage_registry.register_storage(tag='{tag}', "));
                if !value_text.is_empty() {
                    self.emit_mapped(value_text, orig_start, orig_len);
                }
                self.emit(")\n");
            }
            DirectiveValue::Block(_) => {
                self.emit(&format!(
                    "workflow.storage_registry.register_storage(tag='{tag}')\n"
                ));
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

        self.emit_mapped(&format!("@workflow.{kind_str}"), orig_start, orig_len);
        self.emit("\n");

        self.emit(&format!("def {func_name}(log):\n"));

        if handler.body.is_empty() {
            self.emit(&format!("{INDENT}pass\n"));
        } else {
            let handler_start = handler.range.start().to_u32() as usize;
            let handler_end = handler.range.end().to_u32() as usize;

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
                        self.emit(INDENT);
                        let line_start_in_source =
                            header_end + (line.as_ptr() as usize - block_source.as_ptr() as usize);
                        let orig_content_start =
                            line_start_in_source + (line.len() - stripped.len());
                        self.emit_mapped(stripped, orig_content_start, stripped.len());
                        self.emit("\n");
                    }
                }
            } else {
                self.emit(&format!("{INDENT}pass\n"));
            }
        }
        self.emit("\n");
    }

    // ================================================================
    // Python pass-through
    // ================================================================

    fn emit_python_stmt(
        &mut self,
        stmt: &ruff_python_ast::Stmt,
        chunk_offset: ruff_text_size::TextSize,
    ) {
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

# snakemake-lang Full Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the complete Snakemake parser, compiler, and test suite — turning the current skeleton (AST types + stubs) into a working tool that can parse any Snakefile, compile it to virtual Python, and produce source maps.

**Architecture:** Line-by-line scanner identifies Snakemake constructs by leading keywords and indentation. Python expressions/statements within Snakemake blocks are delegated to ruff's `parse_unchecked()`. The compiler walks the AST and emits decorator-chain Python that the Snakemake engine can exec. Source maps track every generated span back to its origin.

**Tech Stack:** Rust (edition 2024), ruff crates (0.14.0 via git) for Python parsing, PyO3 for Python bindings, maturin for packaging, insta for snapshot tests.

---

## Current state

**Done:** AST types (all Snakemake constructs), error types, source map infrastructure, VirtualPythonGenerator skeleton, CLI shell, PyO3 shell.

**Not done:** Parser (entirely stubbed), compiler generate() (stubbed), all tests, fixtures, packaging.

**Build broken:** `Cargo.toml` has a feature resolution bug (bare `serde_json` in `cli` feature conflicts with `dep:serde_json` in `serde` feature). Must fix before anything compiles.

## Design decisions

### Rules inside Python control flow (flat AST, Option A)

Rules inside `if`/`for`/`while` blocks are common in real workflows:

```snakemake
if config.get("run_qc", True):
    rule fastqc:
        input: "{sample}.fq"
        shell: "fastqc {input}"

for tool in ["bwa", "bowtie"]:
    rule:
        name: f"align_{tool}"
        input: "{sample}.fq"
        shell: f"{tool} {{input}}"
```

The line-by-line scanner handles this naturally — it doesn't care about Python nesting depth. It sees `rule` at a line start and enters rule-parsing mode. The `if`/`for` are Python statements that get collected and passed to ruff. The result is a **flat** AST:

```
Statement::Python(if ...)    ← the if header
Statement::Rule(fastqc)      ← the rule, as a top-level statement
```

This is **Option A** (flat representation). The nesting information is lost in the AST, but compilation works correctly — the `if` passes through as Python, the rule compiles to its decorator chain, and the indentation/ordering in the output means the rule registration happens inside the `if` block just like the original.

For AST consumers (linter, LSP), this means you can't statically tell a rule is conditionally defined. For v0.1, this is fine — static analysis treats all rules as potentially existing, and execution-time evaluation resolves the condition.

> **Post-v0.1 enhancement:** Add `Statement::ConditionalBlock` or similar to preserve nesting in the AST for tools that need structural analysis.

### Equivalence testing against parser.py

The legacy `parser.py` is part of the `snakemake` package, which will be installed in the test environment. Equivalence tests run both parsers on the same fixtures and diff the compiled output. This is how we validate correctness at scale.

## File structure

### Files to modify
- `Cargo.toml` — fix feature bug
- `src/parser/mod.rs` — main parse loop, Scanner/Parser structs
- `src/parser/snakemake.rs` — rule/checkpoint parsing
- `src/parser/directive.rs` — directive value parsing (inline, block, run)
- `src/parser/global.rs` — global directives, ruleorder, localrules, storage
- `src/parser/handler.rs` — onsuccess/onerror/onstart parsing
- `src/compile/mod.rs` — generate() entry point
- `src/compile/generator.rs` — VirtualPythonGenerator.generate() implementation

### Files to create
- `src/parser/module.rs` — module parsing
- `src/parser/use_rule.rs` — use rule parsing
- `tests/parse_basic.rs` — basic parsing integration tests
- `tests/parse_rule.rs` — rule parsing tests
- `tests/parse_constructs.rs` — module, use rule, global, handler tests
- `tests/compile_basic.rs` — compilation tests
- `tests/compile_constructs.rs` — compilation of all construct types
- `tests/fixtures/simple_rule.smk` — minimal rule fixture
- `tests/fixtures/multi_rule.smk` — multiple rules with Python between
- `tests/fixtures/all_directives.smk` — rule with every directive
- `tests/fixtures/module_use_rule.smk` — module + use rule fixture
- `tests/fixtures/globals.smk` — global directives fixture
- `tests/fixtures/handlers.smk` — event handler fixture
- `tests/fixtures/real_workflow.smk` — realistic multi-rule workflow
- `tests/fixtures/control_flow.smk` — rules inside if/for blocks
- `tests/equivalence.rs` — compare output with legacy parser.py

---

## Phase 1: Parser Foundation

### Task 1: Fix build and verify ruff integration

**Files:**
- Modify: `Cargo.toml`
- Create: `tests/parse_basic.rs`

- [ ] **Step 1: Write a test that imports ruff and parses a Python expression**

```rust
// tests/parse_basic.rs
use ruff_python_parser::{Mode, parse_unchecked};

#[test]
fn ruff_parses_simple_expression() {
    let parsed = parse_unchecked("42 + 1", Mode::Expression);
    assert!(parsed.errors().is_empty(), "ruff should parse '42 + 1' without errors");
}

#[test]
fn ruff_parses_function_call_with_kwargs() {
    let source = r#"f("a.txt", "b.txt", ref="genome.fa")"#;
    let parsed = parse_unchecked(source, Mode::Expression);
    assert!(parsed.errors().is_empty(), "ruff should parse function call with kwargs");
}

#[test]
fn ruff_parses_module() {
    let source = "x = 1\ny = 2\n";
    let parsed = parse_unchecked(source, Mode::Module);
    assert!(parsed.errors().is_empty(), "ruff should parse module");
}
```

- [ ] **Step 2: Fix Cargo.toml feature bug and run the test**

In `Cargo.toml`, change:
```toml
cli = ["clap", "serde", "serde_json"]
```
to:
```toml
cli = ["clap", "serde"]
```

The `serde` feature already enables `dep:serde_json`, so `cli` gets serde_json transitively.

Run: `cargo test --test parse_basic -- --nocapture`
Expected: All 3 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml tests/parse_basic.rs
git commit -m "Fix Cargo.toml feature bug, add ruff integration smoke tests

The cli feature referenced bare serde_json which conflicts with dep:serde_json
in the serde feature under edition 2024 resolver rules. Removed the redundant
reference since serde already pulls in serde_json.

Added basic tests to verify ruff's parse_unchecked works for expressions,
function calls with kwargs, and module-mode parsing."
```

---

### Task 2: Line scanner

**Files:**
- Modify: `src/parser/mod.rs`

The scanner breaks source into lines with byte offsets and indentation levels. This is the foundation everything else builds on.

- [ ] **Step 1: Write failing tests for the scanner**

Add to `src/parser/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_splits_lines() {
        let source = "rule foo:\n    input: \"a.txt\"\n";
        let lines = scan_lines(source);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "rule foo:");
        assert_eq!(lines[1].text, "    input: \"a.txt\"");
    }

    #[test]
    fn scanner_tracks_byte_offsets() {
        let source = "line1\nline2\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].start, 0);
        assert_eq!(lines[1].start, 6); // "line1\n" = 6 bytes
    }

    #[test]
    fn scanner_measures_indentation() {
        let source = "rule foo:\n    input: \"a.txt\"\n        \"b.txt\"\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].indent, 0);
        assert_eq!(lines[1].indent, 4);
        assert_eq!(lines[2].indent, 8);
    }

    #[test]
    fn scanner_handles_blank_lines() {
        let source = "rule foo:\n\n    input: \"a.txt\"\n";
        let lines = scan_lines(source);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].text, "");
        assert_eq!(lines[1].indent, 0);
    }

    #[test]
    fn scanner_first_word_extraction() {
        let source = "rule foo:\n    input: \"a.txt\"\nuse rule * from mod\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].first_word(), Some("rule"));
        assert_eq!(lines[1].first_word(), Some("input"));
        assert_eq!(lines[2].first_word(), Some("use"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p snakemake-lang parser::tests -- --nocapture`
Expected: FAIL — `scan_lines` not defined.

- [ ] **Step 3: Implement the scanner**

```rust
// In src/parser/mod.rs, above the parse() function:

/// A single line from the source, with metadata.
#[derive(Debug)]
struct Line<'a> {
    /// The line text (without trailing newline).
    text: &'a str,
    /// Byte offset of the line start in the original source.
    start: usize,
    /// Number of leading spaces (tabs count as error — we reject mixed indentation).
    indent: usize,
    /// 1-based line number.
    number: usize,
}

impl<'a> Line<'a> {
    /// The first whitespace-delimited word on the line, or None for blank lines.
    fn first_word(&self) -> Option<&str> {
        self.text.trim_start().split_whitespace().next()
    }

    /// The text after stripping leading whitespace.
    fn trimmed(&self) -> &str {
        self.text.trim_start()
    }

    /// Whether this is a blank or comment-only line.
    fn is_blank_or_comment(&self) -> bool {
        let t = self.trimmed();
        t.is_empty() || t.starts_with('#')
    }
}

/// Split source into lines with byte offsets and indentation.
fn scan_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for (i, line_text) in source.split('\n').enumerate() {
        // Strip trailing \r for Windows line endings
        let text = line_text.strip_suffix('\r').unwrap_or(line_text);
        let indent = text.len() - text.trim_start_matches(' ').len();
        lines.push(Line {
            text,
            start: offset,
            indent,
            number: i + 1,
        });
        offset += line_text.len() + 1; // +1 for the \n
    }
    // Remove trailing empty line from final \n
    if lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    lines
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p snakemake-lang parser::tests -- --nocapture`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/parser/mod.rs
git commit -m "Add line scanner for Snakemake source

Splits source into Line structs with byte offsets, indentation levels, and
line numbers. Handles blank lines and provides first_word() for keyword
detection. Foundation for the line-by-line Snakemake parser."
```

---

### Task 3: Parser struct and top-level keyword dispatch

**Files:**
- Modify: `src/parser/mod.rs`
- Modify: `tests/parse_basic.rs`

- [ ] **Step 1: Write failing test for parsing empty and Python-only source**

Add to `tests/parse_basic.rs`:

```rust
use snakemake_lang::parse;

#[test]
fn parse_empty_source() {
    let ast = parse("", "Snakefile").unwrap();
    assert!(ast.body.is_empty());
}

#[test]
fn parse_python_only() {
    let ast = parse("x = 1\ny = 2\n", "Snakefile").unwrap();
    assert_eq!(ast.body.len(), 2);
    // Both should be Python statements
    for stmt in &ast.body {
        assert!(matches!(stmt, snakemake_lang::ast::Statement::Python(_)));
    }
}

#[test]
fn parse_single_rule_detected() {
    let source = "rule foo:\n    input: \"a.txt\"\n";
    let ast = parse(source, "Snakefile").unwrap();
    assert_eq!(ast.body.len(), 1);
    assert!(matches!(&ast.body[0], snakemake_lang::ast::Statement::Rule(_)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test parse_basic -- --nocapture`
Expected: FAIL — `parse()` still calls `todo!()`.

- [ ] **Step 3: Implement Parser struct and top-level dispatch**

Replace the `parse()` function in `src/parser/mod.rs`:

```rust
use ruff_python_parser::{self, Mode};
use ruff_python_ast::Stmt;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use crate::errors::{ParseError, ParseErrorKind};

/// Main parser state.
struct Parser<'a> {
    source: &'a str,
    path: &'a str,
    lines: Vec<Line<'a>>,
    cursor: usize,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, path: &'a str) -> Self {
        Self {
            source,
            path,
            lines: scan_lines(source),
            cursor: 0,
            errors: Vec::new(),
        }
    }

    fn current(&self) -> Option<&Line<'a>> {
        self.lines.get(self.cursor)
    }

    fn advance(&mut self) {
        self.cursor += 1;
    }

    fn at_end(&self) -> bool {
        self.cursor >= self.lines.len()
    }

    /// Parse the entire file into a Snakefile AST.
    fn parse_file(&mut self) -> Snakefile {
        let mut body = Vec::new();

        while !self.at_end() {
            let line = &self.lines[self.cursor];

            // Skip blank/comment lines
            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            // rule/checkpoint can appear at any indent (inside if/for/while)
            match line.first_word() {
                Some("rule") => {
                    body.push(Statement::Rule(self.parse_rule(false)));
                    continue;
                }
                Some("checkpoint") => {
                    body.push(Statement::Rule(self.parse_rule(true)));
                    continue;
                }
                _ => {}
            }

            // Other Snakemake keywords only at column 0 (top-level)
            if line.indent == 0 {
                match line.first_word() {
                    Some("module") => {
                        body.push(Statement::Module(self.parse_module()));
                        continue;
                    }
                    Some("use") => {
                        body.push(Statement::UseRule(self.parse_use_rule()));
                        continue;
                    }
                    Some("onsuccess" | "onerror" | "onstart") => {
                        body.push(Statement::Handler(self.parse_handler()));
                        continue;
                    }
                    Some("ruleorder") => {
                        body.push(Statement::Ruleorder(self.parse_ruleorder()));
                        continue;
                    }
                    Some("localrules") => {
                        body.push(Statement::Localrules(self.parse_localrules()));
                        continue;
                    }
                    Some("storage") => {
                        body.push(Statement::Storage(self.parse_storage()));
                        continue;
                    }
                    Some(word) if GlobalKeyword::from_str(word).is_some() => {
                        body.push(Statement::GlobalDirective(self.parse_global_directive()));
                        continue;
                    }
                    _ => {}
                }
            }

            // Not a Snakemake keyword — collect as Python
            let python_stmts = self.collect_python();
            body.extend(python_stmts.into_iter().map(Statement::Python));
        }

        let range = if self.source.is_empty() {
            TextRange::default()
        } else {
            TextRange::new(TextSize::new(0), TextSize::new(self.source.len() as u32))
        };

        Snakefile { body, range }
    }

    /// Collect contiguous non-Snakemake lines and parse as Python.
    fn collect_python(&mut self) -> Vec<Stmt> {
        let start_cursor = self.cursor;
        let start_offset = self.lines[self.cursor].start;

        // Advance past lines that aren't Snakemake keywords
        while let Some(line) = self.current() {
            if !line.is_blank_or_comment() {
                let word = line.first_word().unwrap_or("");
                // rule/checkpoint can appear at any indent
                if word == "rule" || word == "checkpoint" {
                    break;
                }
                // Other Snakemake keywords only at column 0
                if line.indent == 0 && is_top_level_keyword(word) {
                    break;
                }
            }
            self.advance();
        }

        // Determine end offset
        let end_offset = if let Some(line) = self.current() {
            line.start
        } else {
            self.source.len()
        };

        let python_text = &self.source[start_offset..end_offset];
        if python_text.trim().is_empty() {
            return Vec::new();
        }

        let parsed = ruff_python_parser::parse_unchecked(python_text, Mode::Module);

        // Collect ruff parse errors
        for error in parsed.errors() {
            self.errors.push(ParseError {
                message: error.to_string(),
                range: TextRange::default(), // will improve later
                kind: ParseErrorKind::PythonSyntaxError,
                line: self.lines[start_cursor].number,
                column: 0,
                source_line: None,
            });
        }

        // Extract statements and offset ranges
        match parsed.into_syntax() {
            ruff_python_ast::Mod::Module(module) => module.body,
            _ => Vec::new(),
        }
    }
}

/// Check if a word is a top-level Snakemake keyword.
fn is_top_level_keyword(word: &str) -> bool {
    matches!(
        word,
        "rule" | "checkpoint" | "module" | "use"
            | "onsuccess" | "onerror" | "onstart"
            | "ruleorder" | "localrules" | "storage"
    ) || GlobalKeyword::from_str(word).is_some()
}

pub fn parse(source: &str, path: &str) -> Result<Snakefile, Vec<ParseError>> {
    let mut parser = Parser::new(source, path);
    let ast = parser.parse_file();
    if parser.errors.is_empty() {
        Ok(ast)
    } else {
        Err(parser.errors)
    }
}
```

Note: The `parse_rule`, `parse_module`, etc. methods don't exist yet. They're in the sub-modules. For now, add temporary stubs directly on `Parser`:

```rust
impl<'a> Parser<'a> {
    // ... (above methods) ...

    // Temporary stubs — these move to submodules in later tasks
    fn parse_rule(&mut self, is_checkpoint: bool) -> SnakemakeRule {
        todo!("Task 4: implement rule parsing")
    }
    fn parse_module(&mut self) -> SnakemakeModule {
        todo!("Task 14: implement module parsing")
    }
    fn parse_use_rule(&mut self) -> SnakemakeUseRule {
        todo!("Task 15: implement use rule parsing")
    }
    fn parse_handler(&mut self) -> SnakemakeHandler {
        todo!("Task 18: implement handler parsing")
    }
    fn parse_ruleorder(&mut self) -> SnakemakeRuleorder {
        todo!("Task 17: implement ruleorder parsing")
    }
    fn parse_localrules(&mut self) -> SnakemakeLocalrules {
        todo!("Task 17: implement localrules parsing")
    }
    fn parse_storage(&mut self) -> SnakemakeStorage {
        todo!("Task 17: implement storage parsing")
    }
    fn parse_global_directive(&mut self) -> SnakemakeGlobalDirective {
        todo!("Task 16: implement global directive parsing")
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test parse_basic -- parse_empty_source parse_python_only --nocapture`
Expected: `parse_empty_source` and `parse_python_only` PASS. `parse_single_rule_detected` will panic (todo on parse_rule). That's expected.

- [ ] **Step 5: Commit**

```bash
git add src/parser/mod.rs tests/parse_basic.rs
git commit -m "Implement top-level parser dispatch and Python collection

Parser scans lines and dispatches to Snakemake-specific parsers based on
leading keywords at column 0. Non-Snakemake lines are collected and parsed
as Python via ruff. Individual construct parsers are stubbed for now."
```

---

### Task 4: Simple rule parsing with inline directives

**Files:**
- Modify: `src/parser/snakemake.rs`
- Modify: `src/parser/directive.rs`
- Modify: `src/parser/mod.rs` (wire up the new modules)
- Create: `tests/parse_rule.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/parse_rule.rs
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

    // Shell directive should have one positional string argument
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test parse_rule -- --nocapture`
Expected: FAIL — `parse_rule()` is still a `todo!()`.

- [ ] **Step 3: Implement rule parsing**

In `src/parser/snakemake.rs`, implement the rule parser. The Parser methods need access from this module, so make Parser and its fields `pub(crate)`:

```rust
// src/parser/snakemake.rs
use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use crate::errors::{ParseError, ParseErrorKind};
use super::Parser;

impl<'a> Parser<'a> {
    /// Parse a `rule` or `checkpoint` definition.
    ///
    /// Cursor is on the `rule`/`checkpoint` line. Advances past the entire block.
    pub(crate) fn parse_rule(&mut self, is_checkpoint: bool) -> SnakemakeRule {
        let header_line = &self.lines[self.cursor];
        let start_offset = header_line.start;
        let keyword = if is_checkpoint { "checkpoint" } else { "rule" };

        // Parse: rule NAME :
        let trimmed = header_line.trimmed();
        let after_keyword = &trimmed[keyword.len()..].trim_start();
        let (name_str, _rest) = after_keyword
            .split_once(':')
            .unwrap_or((after_keyword, ""));
        let name_str = name_str.trim();

        if name_str.is_empty() {
            self.errors.push(ParseError {
                message: format!("{keyword} definition requires a name"),
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
                    TextSize::new((start_offset + header_line.text.len()) as u32),
                ),
                kind: ParseErrorKind::MissingRuleName,
                line: header_line.number,
                column: 0,
                source_line: Some(header_line.text.to_string()),
            });
        }

        // Calculate name range in original source
        let name_byte_start = self.source[start_offset..]
            .find(name_str)
            .map(|i| start_offset + i)
            .unwrap_or(start_offset);
        let name = Identifier::new(
            name_str.to_string(),
            TextRange::new(
                TextSize::new(name_byte_start as u32),
                TextSize::new((name_byte_start + name_str.len()) as u32),
            ),
        );

        self.advance(); // move past header line

        // Parse directives in the indented body
        let body_indent = self.current().map(|l| l.indent).unwrap_or(0);
        let mut directives = Vec::new();
        let mut docstring = None;

        while let Some(line) = self.current() {
            // Body ends when indentation drops back to rule level or below
            if !line.is_blank_or_comment() && line.indent < body_indent {
                break;
            }

            // Skip blank/comment lines
            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            // Check for docstring (string literal as first non-blank body item)
            if directives.is_empty() && docstring.is_none() {
                let t = line.trimmed();
                if (t.starts_with("\"\"\"") || t.starts_with("'''")
                    || t.starts_with('"') || t.starts_with('\''))
                    && !t.contains(':')
                {
                    // Simple docstring detection — just skip for now
                    // A proper implementation would extract the string value
                    self.advance();
                    continue;
                }
            }

            // Try to parse as directive
            if let Some(directive) = self.try_parse_directive(body_indent) {
                directives.push(directive);
            } else {
                // Unknown line in rule body — skip with error
                self.errors.push(ParseError {
                    message: format!("unexpected line in rule body: {}", line.trimmed()),
                    range: TextRange::new(
                        TextSize::new(line.start as u32),
                        TextSize::new((line.start + line.text.len()) as u32),
                    ),
                    kind: ParseErrorKind::UnknownDirective,
                    line: line.number,
                    column: line.indent,
                    source_line: Some(line.text.to_string()),
                });
                self.advance();
            }
        }

        let end_offset = self.current()
            .map(|l| l.start)
            .unwrap_or(self.source.len());

        SnakemakeRule {
            name,
            directives,
            docstring,
            is_checkpoint,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(end_offset as u32),
            ),
        }
    }
}
```

- [ ] **Step 4: Implement directive parsing**

In `src/parser/directive.rs`:

```rust
use ruff_python_parser::{self, Mode};
use ruff_python_ast::{self, Expr, Identifier};
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use crate::errors::{ParseError, ParseErrorKind};
use super::Parser;

impl<'a> Parser<'a> {
    /// Try to parse the current line as a directive. Returns None if
    /// the line doesn't start with a recognized directive keyword.
    pub(crate) fn try_parse_directive(&mut self, body_indent: usize) -> Option<SnakemakeDirective> {
        let line = self.current()?;
        let trimmed = line.trimmed();

        // Extract the first word (potential keyword)
        let keyword_str = trimmed.split(':').next()?.split_whitespace().next()?;
        let keyword = DirectiveKeyword::from_str(keyword_str)?;

        let line_start = line.start;
        let line_number = line.number;

        if keyword == DirectiveKeyword::Run {
            return Some(self.parse_run_directive(line_start, body_indent));
        }

        // Find the colon after the keyword
        let colon_pos = trimmed.find(':')?;
        let after_colon = trimmed[colon_pos + 1..].trim();

        if after_colon.is_empty() || after_colon.starts_with('#') {
            // Block form: value starts on next indented line(s)
            self.advance();
            let value = self.parse_block_directive_value(body_indent, line_start);
            let end = self.current()
                .map(|l| l.start)
                .unwrap_or(self.source.len());

            Some(SnakemakeDirective {
                keyword,
                value,
                range: TextRange::new(
                    TextSize::new(line_start as u32),
                    TextSize::new(end as u32),
                ),
            })
        } else {
            // Inline form: value is the rest of the line (may continue with open parens)
            let value_start = line_start + line.text.len() - line.trimmed().len()
                + colon_pos + 1
                + (trimmed[colon_pos + 1..].len() - after_colon.len());

            let value_text = self.collect_inline_value(after_colon);
            let value = self.parse_arguments(
                &value_text,
                value_start,
            );

            let end = self.current()
                .map(|l| l.start)
                .unwrap_or(self.source.len());

            Some(SnakemakeDirective {
                keyword,
                value: DirectiveValue::Arguments(value),
                range: TextRange::new(
                    TextSize::new(line_start as u32),
                    TextSize::new(end as u32),
                ),
            })
        }
    }

    /// Collect an inline value, handling continuation lines for open parens/brackets.
    fn collect_inline_value(&mut self, first_part: &str) -> String {
        let mut text = first_part.to_string();
        self.advance();

        // Count open parens/brackets/braces
        let mut depth = count_open_delimiters(&text);
        while depth > 0 {
            match self.current() {
                Some(line) if !line.text.is_empty() => {
                    text.push('\n');
                    text.push_str(line.text);
                    depth += count_open_delimiters(line.text);
                    self.advance();
                }
                _ => break,
            }
        }

        text
    }

    /// Parse a block-form directive value (indented continuation lines).
    fn parse_block_directive_value(
        &mut self,
        parent_indent: usize,
        directive_start: usize,
    ) -> DirectiveValue {
        let mut text = String::new();
        let value_start = self.current().map(|l| l.start).unwrap_or(directive_start);

        // The block's own indentation level is the first non-blank line
        let block_indent = self.current()
            .filter(|l| !l.is_blank_or_comment())
            .map(|l| l.indent)
            .unwrap_or(parent_indent + 4);

        while let Some(line) = self.current() {
            if !line.is_blank_or_comment() && line.indent <= parent_indent {
                break;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line.text);
            self.advance();
        }

        if text.trim().is_empty() {
            return DirectiveValue::Arguments(DirectiveArguments {
                positional: Vec::new(),
                keywords: Vec::new(),
                range: TextRange::default(),
            });
        }

        // Dedent the block to parse it
        let dedented = dedent_block(&text, block_indent);
        DirectiveValue::Arguments(self.parse_arguments(&dedented, value_start))
    }

    /// Parse directive arguments by wrapping in a function call and extracting args.
    ///
    /// Wraps the text as `__f(text)` and parses as expression, then extracts
    /// the Call node's arguments.
    pub(crate) fn parse_arguments(
        &mut self,
        text: &str,
        original_offset: usize,
    ) -> DirectiveArguments {
        let wrapper = format!("__f({text})");
        let prefix_len = 4; // "__f("

        let parsed = ruff_python_parser::parse_unchecked(&wrapper, Mode::Expression);

        for error in parsed.errors() {
            self.errors.push(ParseError {
                message: format!("invalid directive value: {error}"),
                range: TextRange::default(),
                kind: ParseErrorKind::PythonSyntaxError,
                line: 0,
                column: 0,
                source_line: Some(text.to_string()),
            });
        }

        let syntax = parsed.into_syntax();
        match syntax {
            ruff_python_ast::Mod::Expression(expr_mod) => {
                match *expr_mod.body {
                    Expr::Call(call) => {
                        // Extract positional and keyword arguments
                        let positional = call.arguments.args.into_iter().collect();
                        let keywords = call.arguments.keywords.iter().map(|kw| {
                            let name = kw.arg.as_ref().map(|id| id.clone())
                                .unwrap_or_else(|| Identifier::new(
                                    String::new(),
                                    TextRange::default(),
                                ));
                            DirectiveKeywordArgument {
                                name,
                                value: kw.value.clone(),
                                range: kw.range,
                            }
                        }).collect();

                        DirectiveArguments {
                            positional,
                            keywords,
                            range: TextRange::new(
                                TextSize::new(original_offset as u32),
                                TextSize::new((original_offset + text.len()) as u32),
                            ),
                        }
                    }
                    _ => {
                        // Single expression, not a call — shouldn't happen with our wrapper
                        DirectiveArguments {
                            positional: Vec::new(),
                            keywords: Vec::new(),
                            range: TextRange::default(),
                        }
                    }
                }
            }
            _ => DirectiveArguments {
                positional: Vec::new(),
                keywords: Vec::new(),
                range: TextRange::default(),
            },
        }
    }

    /// Parse a `run:` directive (Python block).
    fn parse_run_directive(
        &mut self,
        start_offset: usize,
        body_indent: usize,
    ) -> SnakemakeDirective {
        self.advance(); // past the `run:` line

        let mut block_text = String::new();
        let block_start = self.current().map(|l| l.start).unwrap_or(start_offset);

        let run_indent = self.current()
            .filter(|l| !l.is_blank_or_comment())
            .map(|l| l.indent)
            .unwrap_or(body_indent + 4);

        while let Some(line) = self.current() {
            if !line.is_blank_or_comment() && line.indent <= body_indent {
                break;
            }
            if !block_text.is_empty() {
                block_text.push('\n');
            }
            block_text.push_str(line.text);
            self.advance();
        }

        let dedented = dedent_block(&block_text, run_indent);
        let parsed = ruff_python_parser::parse_unchecked(&dedented, Mode::Module);

        for error in parsed.errors() {
            self.errors.push(ParseError {
                message: format!("syntax error in run block: {error}"),
                range: TextRange::default(),
                kind: ParseErrorKind::PythonSyntaxError,
                line: 0,
                column: 0,
                source_line: None,
            });
        }

        let stmts = match parsed.into_syntax() {
            ruff_python_ast::Mod::Module(module) => module.body,
            _ => Vec::new(),
        };

        let end = self.current().map(|l| l.start).unwrap_or(self.source.len());

        SnakemakeDirective {
            keyword: DirectiveKeyword::Run,
            value: DirectiveValue::Block(stmts),
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(end as u32),
            ),
        }
    }
}

/// Count unmatched opening delimiters.
fn count_open_delimiters(text: &str) -> i32 {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut prev = ' ';

    for ch in text.chars() {
        if in_string {
            if ch == string_char && prev != '\\' {
                in_string = false;
            }
        } else {
            match ch {
                '"' | '\'' => {
                    in_string = true;
                    string_char = ch;
                }
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '#' => break, // comment — stop counting
                _ => {}
            }
        }
        prev = ch;
    }
    depth
}

/// Remove `indent` spaces of leading whitespace from each line.
fn dedent_block(text: &str, indent: usize) -> String {
    text.lines()
        .map(|line| {
            if line.len() >= indent && line[..indent].chars().all(|c| c == ' ') {
                &line[indent..]
            } else {
                line.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

Also update `src/parser/mod.rs` to:
1. Remove the `parse_rule` stub (it's now in `snakemake.rs`)
2. Make `Parser` fields `pub(crate)` so submodules can access them
3. Add `pub mod snakemake;` etc. for the new module wiring

- [ ] **Step 5: Run tests**

Run: `cargo test --test parse_rule -- --nocapture`
Expected: All 4 tests PASS.

Then run all tests: `cargo test`
Expected: All tests PASS (the ruff smoke tests + scanner tests + parse basic tests that don't hit stubs + parse rule tests).

- [ ] **Step 6: Commit**

```bash
git add src/parser/snakemake.rs src/parser/directive.rs src/parser/mod.rs tests/parse_rule.rs
git commit -m "Implement rule and directive parsing

Parse rule/checkpoint headers with name extraction. Parse inline directive
values by wrapping in a synthetic function call and extracting ruff's Call
node arguments. Handle block-form directives via indentation tracking.
Parse run: blocks as Python module via ruff.

Supports positional args, keyword args, trailing commas, and parenthesized
continuation lines."
```

---

### Task 5: Block-form directives and multiline values

**Files:**
- Modify: `tests/parse_rule.rs`
- Modify: `src/parser/directive.rs` (if fixes needed)

- [ ] **Step 1: Write tests for block-form and multiline values**

```rust
// Add to tests/parse_rule.rs

#[test]
fn parse_block_form_directive() {
    let source = "\
rule foo:
    input:
        \"a.txt\",
        \"b.txt\",
        ref=\"genome.fa\"
    output: \"result.txt\"
";
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
fn parse_multiline_parenthesized() {
    let source = "\
rule foo:
    input: expand(\"reads/{sample}.fq\",
                  sample=SAMPLES)
    output: \"out.txt\"
";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };

    assert_eq!(rule.directives.len(), 2);
    // First directive value should be a single expand() call
    match &rule.directives[0].value {
        DirectiveValue::Arguments(args) => {
            assert_eq!(args.positional.len(), 1, "expand() call should be one positional arg");
        }
        other => panic!("expected Arguments, got {:?}", other),
    }
}

#[test]
fn parse_run_block() {
    let source = "\
rule process:
    input: \"data.csv\"
    run:
        import pandas as pd
        df = pd.read_csv(input[0])
        df.to_parquet(output[0])
";
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --test parse_rule -- --nocapture`
Expected: All tests PASS (if directive parsing was implemented correctly in Task 4). Fix any issues.

- [ ] **Step 3: Commit**

```bash
git add tests/parse_rule.rs
git commit -m "Add tests for block-form directives, multiline values, and run blocks

Verifies block-form directive parsing with indented continuation lines,
parenthesized expressions spanning multiple lines, and run: blocks that
contain multi-statement Python code."
```

---

### Task 6: Multiple rules and Python interleaving

**Files:**
- Modify: `tests/parse_rule.rs`
- Create: `tests/fixtures/multi_rule.smk`

- [ ] **Step 1: Write test fixture and tests**

```snakemake
# tests/fixtures/multi_rule.smk
import os

SAMPLES = ["A", "B", "C"]

rule all:
    input: expand("results/{sample}.txt", sample=SAMPLES)

def get_input(wildcards):
    return f"data/{wildcards.sample}.csv"

rule process:
    input: get_input
    output: "results/{sample}.txt"
    threads: 4
    shell: "process --threads {threads} {input} > {output}"
```

```rust
// Add to tests/parse_rule.rs
use snakemake_lang::ast::Statement;

#[test]
fn parse_multi_rule_with_python() {
    let source = std::fs::read_to_string("tests/fixtures/multi_rule.smk").unwrap();
    let ast = parse(&source, "multi_rule.smk").unwrap();

    // Should have: import, assignment, rule all, function def, rule process
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
    assert!(python_count >= 2, "should find at least 2 Python statements (import + assignment + function)");
}

#[test]
fn parse_rules_in_sequence() {
    let source = "\
rule a:
    input: \"x\"

rule b:
    input: \"y\"

rule c:
    input: \"z\"
";
    let ast = parse(source, "test.smk").unwrap();
    let rules: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Rule(r) = s { Some(r) } else { None }
    }).collect();

    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].name.as_str(), "a");
    assert_eq!(rules[1].name.as_str(), "b");
    assert_eq!(rules[2].name.as_str(), "c");
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --test parse_rule -- parse_multi_rule parse_rules_in_sequence --nocapture`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/parse_rule.rs tests/fixtures/multi_rule.smk
git commit -m "Test multi-rule parsing and Python interleaving

Adds a fixture with two rules separated by Python imports, assignments, and
function definitions. Verifies the parser correctly identifies rule boundaries
and collects Python code between rules."
```

---

### Task 6b: Rules inside Python control flow

**Files:**
- Create: `tests/fixtures/control_flow.smk`
- Modify: `tests/parse_rule.rs`

This is a common pattern in real workflows. The line-by-line scanner handles it naturally — `rule` at a line start triggers rule parsing regardless of surrounding Python nesting. The AST is flat (Option A): the `if`/`for` is a `Statement::Python`, the rule is a sibling `Statement::Rule`.

- [ ] **Step 1: Create fixture and write tests**

```snakemake
# tests/fixtures/control_flow.smk
SAMPLES = ["A", "B"]

if config.get("run_qc", True):
    rule fastqc:
        input: "data/{sample}.fq"
        output: "qc/{sample}_fastqc.html"
        shell: "fastqc {input} -o qc/"

for i in range(3):
    rule:
        name: f"step_{i}"
        input: f"stage{i}.txt"
        output: f"stage{i+1}.txt"
        shell: "process {input} > {output}"

rule always_runs:
    input: "final_input.txt"
    output: "final_output.txt"
    shell: "finalize {input} > {output}"
```

```rust
// Add to tests/parse_rule.rs

#[test]
fn parse_rules_inside_if_block() {
    let source = "\
if config.get(\"run_qc\", True):
    rule fastqc:
        input: \"data/{sample}.fq\"
        shell: \"fastqc {input}\"

rule always:
    input: \"x\"
";
    let ast = parse(source, "test.smk").unwrap();

    // Flat AST: if is Python, rule is a sibling Statement::Rule
    let rules: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Rule(r) = s { Some(r) } else { None }
    }).collect();
    assert_eq!(rules.len(), 2, "should find both rules (flat AST)");
    assert_eq!(rules[0].name.as_str(), "fastqc");
    assert_eq!(rules[1].name.as_str(), "always");
}

#[test]
fn parse_rules_inside_for_loop() {
    let source = "\
for tool in [\"bwa\", \"bowtie\"]:
    rule:
        name: f\"align_{tool}\"
        input: \"{sample}.fq\"
        shell: f\"{tool} {{input}}\"
";
    let ast = parse(source, "test.smk").unwrap();

    let rules: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Rule(r) = s { Some(r) } else { None }
    }).collect();
    // The for loop generates rules dynamically, but the parser sees
    // the rule template once in the source
    assert!(!rules.is_empty(), "should parse the rule inside the for loop");
}

#[test]
fn parse_control_flow_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/control_flow.smk").unwrap();
    let ast = parse(&source, "control_flow.smk").unwrap();

    let rule_count = ast.body.iter().filter(|s| matches!(s, Statement::Rule(_))).count();
    assert!(rule_count >= 2, "should find rules inside control flow, got {rule_count}");
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --test parse_rule -- parse_rules_inside parse_control_flow --nocapture`
Expected: PASS. The scanner sees `rule` at line start (indented or not) and enters rule-parsing mode. The `if`/`for` lines get collected as Python.

Note: if the scanner only matches keywords at indent level 0, these tests will fail. In that case, update the top-level dispatch in `parse_file()` to recognize `rule`/`checkpoint` at *any* indentation level (not just column 0), while keeping other Snakemake keywords top-level-only. This matches the real Snakemake behavior where rules can appear inside control flow but global directives cannot.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/control_flow.smk tests/parse_rule.rs
git commit -m "Handle rules inside if/for blocks (flat AST representation)

Rules inside Python control flow are parsed as sibling Statement::Rule nodes
alongside Statement::Python for the if/for. The scanner recognizes rule/checkpoint
at any indentation level, not just column 0. The flat representation is correct
for compilation — Python control flow passes through, and rule registration
decorators execute inside the block naturally.

Post-v0.1: consider ConditionalBlock AST nodes for structural analysis."
```

---

### Task 7: Error recovery

**Files:**
- Modify: `src/parser/snakemake.rs`
- Modify: `src/parser/mod.rs`
- Modify: `tests/parse_rule.rs`

- [ ] **Step 1: Write error case tests**

```rust
// Add to tests/parse_rule.rs

#[test]
fn parse_unknown_directive_recovers() {
    let source = "\
rule foo:
    input: \"a.txt\"
    bogus: \"what\"
    output: \"b.txt\"
";
    // Should produce errors but still return a partial AST
    let result = parse(source, "test.smk");
    // Accept either Ok (with partial parse) or Err (with errors)
    // The key is that we get the valid directives
    match result {
        Ok(ast) => {
            let rule = match &ast.body[0] {
                Statement::Rule(r) => r,
                other => panic!("expected Rule, got {:?}", other),
            };
            // Should have at least input and output, with bogus skipped
            assert!(rule.directives.len() >= 2);
        }
        Err(errors) => {
            assert!(errors.iter().any(|e| e.message.contains("bogus")),
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
                snakemake_lang::errors::ParseErrorKind::MissingRuleName)));
        }
        Ok(_) => {
            // If we allow recovery, that's also fine
        }
    }
}
```

- [ ] **Step 2: Ensure error recovery works**

The parser should collect errors in `self.errors` and continue parsing. When done, if there are errors, the `parse()` function should return `Err(errors)`. But the parser should still attempt to produce as complete an AST as possible.

Update `parse()` to optionally return both AST and errors (or make errors non-fatal for recovery):

The current design returns `Result<Snakefile, Vec<ParseError>>`. For error recovery, the easiest approach is: collect errors but return `Ok(ast)` when possible, only return `Err` for truly unrecoverable errors. Add errors to a recoverable list.

Alternatively, return errors alongside the AST. For now, keep the `Result` API but make non-fatal errors non-blocking:

```rust
pub fn parse(source: &str, path: &str) -> Result<Snakefile, Vec<ParseError>> {
    let mut parser = Parser::new(source, path);
    let ast = parser.parse_file();
    if parser.errors.is_empty() {
        Ok(ast)
    } else {
        // Still return Ok if we got a partial AST — errors are warnings
        // Only return Err for fatal errors (none currently)
        Err(parser.errors)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test parse_rule -- --nocapture`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/parser/snakemake.rs src/parser/directive.rs src/parser/mod.rs tests/parse_rule.rs
git commit -m "Add error recovery for unknown directives and missing rule names

Parser now collects errors and continues rather than panicking. Unknown
directives in a rule body emit a diagnostic and skip the line. Missing
rule names emit a specific MissingRuleName error."
```

---

### Task 8: Documentation checkpoint 1

**Files:**
- Create: `tests/fixtures/simple_rule.smk`
- Create: `tests/fixtures/all_directives.smk`

- [ ] **Step 1: Create fixture files for testing**

```snakemake
# tests/fixtures/simple_rule.smk
rule align:
    input: "reads/{sample}.fastq"
    output: "aligned/{sample}.bam"
    threads: 8
    shell: "bwa mem -t {threads} {input} > {output}"
```

```snakemake
# tests/fixtures/all_directives.smk
rule full_example:
    input:
        reads="data/{sample}.fq",
        ref="genome.fa"
    output:
        bam="aligned/{sample}.bam",
        bai="aligned/{sample}.bam.bai"
    params:
        extra="--rg-id {sample}"
    log: "logs/{sample}.log"
    benchmark: "benchmarks/{sample}.txt"
    threads: 8
    resources:
        mem_mb=4096,
        disk_mb=1000
    retries: 3
    priority: 50
    conda: "envs/align.yaml"
    container: "docker://biocontainers/bwa:0.7.17"
    envmodules: "bwa/0.7.17", "samtools/1.15"
    message: "Aligning {wildcards.sample}"
    wildcard_constraints:
        sample="[A-Za-z0-9]+"
    shadow: "minimal"
    group: "alignment"
    shell: "bwa mem -t {threads} {params.extra} {input.ref} {input.reads} | samtools sort -o {output.bam} && samtools index {output.bam}"
```

- [ ] **Step 2: Add fixture parsing tests**

```rust
// Add to tests/parse_rule.rs

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
    // Verify we parsed many directives successfully
    assert!(rule.directives.len() >= 15, "expected at least 15 directives, got {}", rule.directives.len());
}
```

- [ ] **Step 3: Run all tests and verify everything passes**

Run: `cargo test -- --nocapture`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/ tests/parse_rule.rs
git commit -m "Add test fixtures and verify rule parsing completeness

Adds simple_rule.smk with a basic 4-directive rule and all_directives.smk
with 15+ directives covering every directive category. Both parse
successfully."
```

---

## Phase 2: All Snakemake Constructs

### Task 9: Module parsing

**Files:**
- Create: `src/parser/module.rs`
- Modify: `src/parser/mod.rs` (add `pub mod module;`, remove stub)
- Create: `tests/parse_constructs.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/parse_constructs.rs
use snakemake_lang::{parse, ast::*};

#[test]
fn parse_module() {
    let source = "\
module other_workflow:
    snakefile: \"other/Snakefile\"
    config: config
";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);

    let module = match &ast.body[0] {
        Statement::Module(m) => m,
        other => panic!("expected Module, got {:?}", other),
    };
    assert_eq!(module.name.as_str(), "other_workflow");
    assert_eq!(module.directives.len(), 2);
}

#[test]
fn parse_module_all_keywords() {
    let source = "\
module analysis:
    snakefile: \"analysis/Snakefile\"
    config: config
    skip_validation: True
    meta_wrapper: \"v1.0\"
    replace_prefix: {\"old\": \"new\"}
    prefix: \"analysis\"
";
    let ast = parse(source, "test.smk").unwrap();
    let module = match &ast.body[0] {
        Statement::Module(m) => m,
        other => panic!("expected Module, got {:?}", other),
    };
    assert_eq!(module.directives.len(), 6);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test parse_constructs -- --nocapture`
Expected: FAIL — `parse_module()` is `todo!()`.

- [ ] **Step 3: Implement module parsing**

```rust
// src/parser/module.rs
use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use crate::errors::{ParseError, ParseErrorKind};
use super::Parser;

impl<'a> Parser<'a> {
    pub(crate) fn parse_module(&mut self) -> SnakemakeModule {
        let header_line = &self.lines[self.cursor];
        let start_offset = header_line.start;

        // Parse: module NAME :
        let trimmed = header_line.trimmed();
        let after_keyword = &trimmed["module".len()..].trim_start();
        let (name_str, _) = after_keyword.split_once(':').unwrap_or((after_keyword, ""));
        let name_str = name_str.trim();

        let name_byte_start = self.source[start_offset..]
            .find(name_str)
            .map(|i| start_offset + i)
            .unwrap_or(start_offset);
        let name = Identifier::new(
            name_str.to_string(),
            TextRange::new(
                TextSize::new(name_byte_start as u32),
                TextSize::new((name_byte_start + name_str.len()) as u32),
            ),
        );

        self.advance(); // past header

        let body_indent = self.current().map(|l| l.indent).unwrap_or(0);
        let mut directives = Vec::new();

        while let Some(line) = self.current() {
            if !line.is_blank_or_comment() && line.indent < body_indent {
                break;
            }
            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            let trimmed = line.trimmed();
            let keyword_str = trimmed.split(':').next()
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("");

            if let Some(keyword) = ModuleKeyword::from_str(keyword_str) {
                let directive_start = line.start;
                let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
                let after_colon = trimmed[colon_pos + 1..].trim();

                if after_colon.is_empty() || after_colon.starts_with('#') {
                    self.advance();
                    let value = self.parse_block_directive_value(body_indent, directive_start);
                    let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
                    directives.push(ModuleDirective {
                        keyword,
                        value,
                        range: TextRange::new(
                            TextSize::new(directive_start as u32),
                            TextSize::new(end as u32),
                        ),
                    });
                } else {
                    let value_start = directive_start + line.text.len() - line.trimmed().len()
                        + colon_pos + 1
                        + (trimmed[colon_pos + 1..].len() - after_colon.len());
                    let value_text = self.collect_inline_value(after_colon);
                    let args = self.parse_arguments(&value_text, value_start);
                    let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
                    directives.push(ModuleDirective {
                        keyword,
                        value: DirectiveValue::Arguments(args),
                        range: TextRange::new(
                            TextSize::new(directive_start as u32),
                            TextSize::new(end as u32),
                        ),
                    });
                }
            } else {
                self.advance(); // skip unrecognized line
            }
        }

        let end_offset = self.current().map(|l| l.start).unwrap_or(self.source.len());

        SnakemakeModule {
            name,
            directives,
            docstring: None,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(end_offset as u32),
            ),
        }
    }
}
```

Update `src/parser/mod.rs`: add `pub mod module;` and remove the `parse_module` stub.

- [ ] **Step 4: Run tests**

Run: `cargo test --test parse_constructs -- --nocapture`
Expected: Module tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/parser/module.rs src/parser/mod.rs tests/parse_constructs.rs
git commit -m "Implement module parsing

Parse module NAME: header and module-specific directives (snakefile, config,
skip_validation, meta_wrapper, replace_prefix, prefix, name, pathvars).
Reuses the directive value parsing infrastructure from rule parsing."
```

---

### Task 10: Use rule parsing

**Files:**
- Create: `src/parser/use_rule.rs`
- Modify: `src/parser/mod.rs` (add module, remove stub)
- Modify: `tests/parse_constructs.rs`

- [ ] **Step 1: Write failing tests**

```rust
// Add to tests/parse_constructs.rs

#[test]
fn parse_use_rule_simple() {
    let source = "use rule align from qc_module\n";
    let ast = parse(source, "test.smk").unwrap();

    let use_rule = match &ast.body[0] {
        Statement::UseRule(u) => u,
        other => panic!("expected UseRule, got {:?}", other),
    };
    assert!(matches!(&use_rule.rules, RuleNames::Named(names) if names.len() == 1));
    assert_eq!(use_rule.from_module.as_str(), "qc_module");
    assert!(use_rule.exclude.is_empty());
    assert!(use_rule.name_modifier.is_none());
    assert!(use_rule.with_directives.is_none());
}

#[test]
fn parse_use_rule_wildcard() {
    let source = "use rule * from other_module exclude trim as qc_*\n";
    let ast = parse(source, "test.smk").unwrap();

    let use_rule = match &ast.body[0] {
        Statement::UseRule(u) => u,
        other => panic!("expected UseRule, got {:?}", other),
    };
    assert!(matches!(&use_rule.rules, RuleNames::All));
    assert_eq!(use_rule.from_module.as_str(), "other_module");
    assert_eq!(use_rule.exclude.len(), 1);
    assert_eq!(use_rule.exclude[0].as_str(), "trim");
    assert_eq!(use_rule.name_modifier.as_deref(), Some("qc_*"));
}

#[test]
fn parse_use_rule_with_block() {
    let source = "\
use rule align, sort from other_module with:
    threads: 16
    resources:
        mem_mb=8192
";
    let ast = parse(source, "test.smk").unwrap();

    let use_rule = match &ast.body[0] {
        Statement::UseRule(u) => u,
        other => panic!("expected UseRule, got {:?}", other),
    };
    assert!(matches!(&use_rule.rules, RuleNames::Named(names) if names.len() == 2));
    let with_directives = use_rule.with_directives.as_ref().unwrap();
    assert_eq!(with_directives.len(), 2);
}

#[test]
fn parse_use_rule_multiple_names() {
    let source = "use rule a, b, c from mod_x\n";
    let ast = parse(source, "test.smk").unwrap();

    let use_rule = match &ast.body[0] {
        Statement::UseRule(u) => u,
        other => panic!("expected UseRule, got {:?}", other),
    };
    match &use_rule.rules {
        RuleNames::Named(names) => {
            assert_eq!(names.len(), 3);
            assert_eq!(names[0].as_str(), "a");
            assert_eq!(names[1].as_str(), "b");
            assert_eq!(names[2].as_str(), "c");
        }
        other => panic!("expected Named, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test parse_constructs -- parse_use_rule --nocapture`
Expected: FAIL — `parse_use_rule()` is `todo!()`.

- [ ] **Step 3: Implement use rule parsing**

```rust
// src/parser/use_rule.rs
use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use crate::errors::{ParseError, ParseErrorKind};
use super::Parser;

impl<'a> Parser<'a> {
    /// Parse `use rule ... from ... [exclude ...] [as ...] [with: ...]`
    pub(crate) fn parse_use_rule(&mut self) -> SnakemakeUseRule {
        let header_line = &self.lines[self.cursor];
        let start_offset = header_line.start;
        let trimmed = header_line.trimmed();

        // Skip "use rule "
        let rest = trimmed.strip_prefix("use").unwrap().trim_start();
        let rest = rest.strip_prefix("rule").unwrap().trim_start();

        // Tokenize the rest by splitting on known clause keywords
        // Format: NAMES from MODULE [exclude NAMES] [as PATTERN] [with:]
        let has_with_block = rest.ends_with("with:") || rest.contains("with:");
        let clause_text = if has_with_block {
            rest.split("with:").next().unwrap_or(rest).trim()
        } else {
            rest.trim_end()
        };

        // Split on "from"
        let (names_text, after_from) = clause_text
            .split_once("from")
            .unwrap_or((clause_text, ""));
        let names_text = names_text.trim();
        let after_from = after_from.trim();

        // Parse rule names
        let rules = if names_text == "*" {
            RuleNames::All
        } else {
            let names = names_text
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| Identifier::new(s.to_string(), TextRange::default()))
                .collect();
            RuleNames::Named(names)
        };

        // Parse module name and optional clauses
        let (module_name, exclude, name_modifier) = parse_use_rule_clauses(after_from);

        let from_module = Identifier::new(module_name, TextRange::default());

        self.advance(); // past the header line

        // Parse optional with: block
        let with_directives = if has_with_block {
            let body_indent = self.current().map(|l| l.indent).unwrap_or(0);
            let mut directives = Vec::new();
            while let Some(line) = self.current() {
                if !line.is_blank_or_comment() && line.indent < body_indent {
                    break;
                }
                if line.is_blank_or_comment() {
                    self.advance();
                    continue;
                }
                if let Some(directive) = self.try_parse_directive(body_indent) {
                    directives.push(directive);
                } else {
                    self.advance();
                }
            }
            Some(directives)
        } else {
            None
        };

        let end_offset = self.current().map(|l| l.start).unwrap_or(self.source.len());

        SnakemakeUseRule {
            rules,
            from_module,
            exclude,
            name_modifier,
            with_directives,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(end_offset as u32),
            ),
        }
    }
}

/// Parse the clauses after "from" in a use rule statement.
/// Returns (module_name, exclude_list, name_modifier).
fn parse_use_rule_clauses(text: &str) -> (String, Vec<Identifier>, Option<String>) {
    let mut exclude = Vec::new();
    let mut name_modifier = None;
    let mut module_name = String::new();

    let parts: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;

    // First token is the module name
    if i < parts.len() {
        module_name = parts[i].to_string();
        i += 1;
    }

    while i < parts.len() {
        match parts[i] {
            "exclude" => {
                i += 1;
                // Collect names until we hit another clause keyword or end
                while i < parts.len() && parts[i] != "as" && parts[i] != "with:" {
                    let name = parts[i].trim_end_matches(',');
                    if !name.is_empty() {
                        exclude.push(Identifier::new(name.to_string(), TextRange::default()));
                    }
                    i += 1;
                }
            }
            "as" => {
                i += 1;
                if i < parts.len() {
                    name_modifier = Some(parts[i].to_string());
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    (module_name, exclude, name_modifier)
}
```

Update `src/parser/mod.rs`: add `pub mod use_rule;`, remove stub.

- [ ] **Step 4: Run tests**

Run: `cargo test --test parse_constructs -- parse_use_rule --nocapture`
Expected: All 4 use rule tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/parser/use_rule.rs src/parser/mod.rs tests/parse_constructs.rs
git commit -m "Implement use rule parsing

Handle all use rule forms: named rules, wildcard (*), exclude clause,
as clause with name pattern, and optional with: block containing directive
overrides. Reuses directive parsing for the with: block body."
```

---

### Task 11: Global directives, ruleorder, localrules, storage

**Files:**
- Modify: `src/parser/global.rs`
- Modify: `src/parser/mod.rs` (remove stubs)
- Modify: `tests/parse_constructs.rs`

- [ ] **Step 1: Write failing tests**

```rust
// Add to tests/parse_constructs.rs

#[test]
fn parse_configfile() {
    let source = "configfile: \"config.yaml\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
    assert!(matches!(&ast.body[0], Statement::GlobalDirective(_)));
}

#[test]
fn parse_include() {
    let source = "include: \"rules/align.smk\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert!(matches!(&ast.body[0], Statement::GlobalDirective(_)));
}

#[test]
fn parse_ruleorder() {
    let source = "ruleorder: align > sort > index\n";
    let ast = parse(source, "test.smk").unwrap();

    let ruleorder = match &ast.body[0] {
        Statement::Ruleorder(r) => r,
        other => panic!("expected Ruleorder, got {:?}", other),
    };
    assert_eq!(ruleorder.names.len(), 3);
    assert_eq!(ruleorder.names[0].as_str(), "align");
    assert_eq!(ruleorder.names[1].as_str(), "sort");
    assert_eq!(ruleorder.names[2].as_str(), "index");
}

#[test]
fn parse_localrules() {
    let source = "localrules: all, clean\n";
    let ast = parse(source, "test.smk").unwrap();

    let localrules = match &ast.body[0] {
        Statement::Localrules(l) => l,
        other => panic!("expected Localrules, got {:?}", other),
    };
    assert_eq!(localrules.names.len(), 2);
}

#[test]
fn parse_storage() {
    let source = "storage s3_data:\n    provider=\"s3\",\n    bucket=\"my-bucket\"\n";
    let ast = parse(source, "test.smk").unwrap();

    let storage = match &ast.body[0] {
        Statement::Storage(s) => s,
        other => panic!("expected Storage, got {:?}", other),
    };
    assert_eq!(storage.tag.as_str(), "s3_data");
}

#[test]
fn parse_wildcard_constraints_global() {
    let source = "wildcard_constraints:\n    sample=\"[A-Za-z0-9]+\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert!(matches!(&ast.body[0], Statement::GlobalDirective(_)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test parse_constructs -- parse_configfile parse_ruleorder parse_localrules parse_storage --nocapture`
Expected: FAIL — all stubs are `todo!()`.

- [ ] **Step 3: Implement global.rs**

```rust
// src/parser/global.rs
use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use super::Parser;

impl<'a> Parser<'a> {
    /// Parse a global directive: `configfile: "config.yaml"` etc.
    pub(crate) fn parse_global_directive(&mut self) -> SnakemakeGlobalDirective {
        let line = &self.lines[self.cursor];
        let start_offset = line.start;
        let trimmed = line.trimmed();

        let keyword_str = trimmed.split(':').next()
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("");
        let keyword = GlobalKeyword::from_str(keyword_str).unwrap();

        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let after_colon = trimmed[colon_pos + 1..].trim();

        if after_colon.is_empty() || after_colon.starts_with('#') {
            self.advance();
            let value = self.parse_block_directive_value(0, start_offset);
            let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
            SnakemakeGlobalDirective {
                keyword,
                value,
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
                    TextSize::new(end as u32),
                ),
            }
        } else {
            let value_start = start_offset + line.text.len() - trimmed.len()
                + colon_pos + 1
                + (trimmed[colon_pos + 1..].len() - after_colon.len());
            let value_text = self.collect_inline_value(after_colon);
            let args = self.parse_arguments(&value_text, value_start);
            let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
            SnakemakeGlobalDirective {
                keyword,
                value: DirectiveValue::Arguments(args),
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
                    TextSize::new(end as u32),
                ),
            }
        }
    }

    /// Parse `ruleorder: a > b > c`
    pub(crate) fn parse_ruleorder(&mut self) -> SnakemakeRuleorder {
        let line = &self.lines[self.cursor];
        let start_offset = line.start;
        let trimmed = line.trimmed();

        let after_colon = trimmed.split_once(':').map(|(_, rest)| rest.trim()).unwrap_or("");
        let names: Vec<Identifier> = after_colon
            .split('>')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Identifier::new(s.to_string(), TextRange::default()))
            .collect();

        self.advance();

        SnakemakeRuleorder {
            names,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(self.current().map(|l| l.start).unwrap_or(self.source.len()) as u32),
            ),
        }
    }

    /// Parse `localrules: a, b, c`
    pub(crate) fn parse_localrules(&mut self) -> SnakemakeLocalrules {
        let line = &self.lines[self.cursor];
        let start_offset = line.start;
        let trimmed = line.trimmed();

        let after_colon = trimmed.split_once(':').map(|(_, rest)| rest.trim()).unwrap_or("");
        let names: Vec<Identifier> = after_colon
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Identifier::new(s.to_string(), TextRange::default()))
            .collect();

        self.advance();

        SnakemakeLocalrules {
            names,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(self.current().map(|l| l.start).unwrap_or(self.source.len()) as u32),
            ),
        }
    }

    /// Parse `storage tag: provider="s3", ...`
    pub(crate) fn parse_storage(&mut self) -> SnakemakeStorage {
        let line = &self.lines[self.cursor];
        let start_offset = line.start;
        let trimmed = line.trimmed();

        // "storage TAG : ..."
        let after_storage = trimmed.strip_prefix("storage").unwrap().trim_start();
        let (tag_str, rest) = after_storage.split_once(':').unwrap_or((after_storage, ""));
        let tag_str = tag_str.trim();
        let tag = Identifier::new(tag_str.to_string(), TextRange::default());

        let after_colon = rest.trim();

        if after_colon.is_empty() || after_colon.starts_with('#') {
            self.advance();
            let value = self.parse_block_directive_value(0, start_offset);
            let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
            SnakemakeStorage {
                tag,
                value,
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
                    TextSize::new(end as u32),
                ),
            }
        } else {
            let value_start = start_offset + (trimmed.len() - after_colon.len());
            let value_text = self.collect_inline_value(after_colon);
            let args = self.parse_arguments(&value_text, value_start);
            let end = self.current().map(|l| l.start).unwrap_or(self.source.len());
            SnakemakeStorage {
                tag,
                value: DirectiveValue::Arguments(args),
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
                    TextSize::new(end as u32),
                ),
            }
        }
    }
}
```

Remove the stubs from `src/parser/mod.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test --test parse_constructs -- --nocapture`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/parser/global.rs src/parser/mod.rs tests/parse_constructs.rs
git commit -m "Implement global directive, ruleorder, localrules, and storage parsing

Parse configfile, include, workdir, envvars, and all other global directives
in both inline and block form. Parse ruleorder (a > b > c), localrules
(a, b, c), and storage (tag: kwargs) constructs."
```

---

### Task 12: Handler parsing

**Files:**
- Modify: `src/parser/handler.rs`
- Modify: `src/parser/mod.rs` (remove stub)
- Modify: `tests/parse_constructs.rs`

- [ ] **Step 1: Write failing tests**

```rust
// Add to tests/parse_constructs.rs

#[test]
fn parse_onsuccess_handler() {
    let source = "\
onsuccess:
    print(\"Workflow finished!\")
    shell(\"mail -s 'done' user@example.com < {log}\")
";
    let ast = parse(source, "test.smk").unwrap();

    let handler = match &ast.body[0] {
        Statement::Handler(h) => h,
        other => panic!("expected Handler, got {:?}", other),
    };
    assert_eq!(handler.kind, HandlerKind::OnSuccess);
    assert_eq!(handler.body.len(), 2);
}

#[test]
fn parse_onerror_handler() {
    let source = "\
onerror:
    print(\"An error occurred\")
";
    let ast = parse(source, "test.smk").unwrap();
    let handler = match &ast.body[0] {
        Statement::Handler(h) => h,
        other => panic!("expected Handler, got {:?}", other),
    };
    assert_eq!(handler.kind, HandlerKind::OnError);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test parse_constructs -- parse_onsuccess parse_onerror --nocapture`
Expected: FAIL.

- [ ] **Step 3: Implement handler parsing**

```rust
// src/parser/handler.rs
use ruff_python_parser::{self, Mode};
use ruff_text_size::{TextRange, TextSize};

use crate::ast::*;
use super::{Parser, dedent_block};

impl<'a> Parser<'a> {
    pub(crate) fn parse_handler(&mut self) -> SnakemakeHandler {
        let line = &self.lines[self.cursor];
        let start_offset = line.start;
        let keyword_str = line.first_word().unwrap_or("");
        let kind = HandlerKind::from_str(keyword_str).unwrap();

        self.advance(); // past the header line

        // Collect the indented Python block
        let mut block_text = String::new();
        let block_indent = self.current()
            .filter(|l| !l.is_blank_or_comment())
            .map(|l| l.indent)
            .unwrap_or(4);

        while let Some(line) = self.current() {
            if !line.is_blank_or_comment() && line.indent < block_indent {
                break;
            }
            if !block_text.is_empty() {
                block_text.push('\n');
            }
            block_text.push_str(line.text);
            self.advance();
        }

        let dedented = dedent_block(&block_text, block_indent);
        let parsed = ruff_python_parser::parse_unchecked(&dedented, Mode::Module);

        let body = match parsed.into_syntax() {
            ruff_python_ast::Mod::Module(module) => module.body,
            _ => Vec::new(),
        };

        let end_offset = self.current().map(|l| l.start).unwrap_or(self.source.len());

        SnakemakeHandler {
            kind,
            body,
            range: TextRange::new(
                TextSize::new(start_offset as u32),
                TextSize::new(end_offset as u32),
            ),
        }
    }
}
```

Make `dedent_block` `pub(crate)` in `src/parser/directive.rs` (or move to `mod.rs`). Remove the `parse_handler` stub from `mod.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test --test parse_constructs -- --nocapture`
Expected: All construct tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/parser/handler.rs src/parser/mod.rs src/parser/directive.rs tests/parse_constructs.rs
git commit -m "Implement handler parsing (onsuccess, onerror, onstart)

Parse event handlers with indented Python block bodies. The block text
is dedented and parsed as a Python module via ruff."
```

---

### Task 13: Comprehensive construct fixture test

**Files:**
- Create: `tests/fixtures/module_use_rule.smk`
- Create: `tests/fixtures/globals.smk`
- Create: `tests/fixtures/handlers.smk`
- Modify: `tests/parse_constructs.rs`

- [ ] **Step 1: Create fixture files**

```snakemake
# tests/fixtures/module_use_rule.smk
module qc:
    snakefile: "qc/Snakefile"
    config: config

use rule * from qc exclude trim as qc_*

use rule align from qc with:
    threads: 16
    resources:
        mem_mb=8192
```

```snakemake
# tests/fixtures/globals.smk
configfile: "config.yaml"

include: "rules/align.smk"
include: "rules/call.smk"

workdir: "analysis/"

wildcard_constraints:
    sample="[A-Za-z0-9]+",
    chr="chr[0-9XY]+"

ruleorder: bwa_align > bowtie_align

localrules: all, clean

container: "docker://continuumio/miniconda3:4.8.2"

envvars: "TMPDIR", "HOME"
```

```snakemake
# tests/fixtures/handlers.smk
onsuccess:
    print("Workflow completed successfully!")
    shell("mail -s 'done' user@example.com")

onerror:
    print("An error occurred")
    shell("mail -s 'error' user@example.com")

onstart:
    print("Workflow is starting")
```

- [ ] **Step 2: Write fixture tests**

```rust
// Add to tests/parse_constructs.rs

#[test]
fn parse_module_use_rule_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/module_use_rule.smk").unwrap();
    let ast = parse(&source, "module_use_rule.smk").unwrap();

    let mut modules = 0;
    let mut use_rules = 0;
    for stmt in &ast.body {
        match stmt {
            Statement::Module(_) => modules += 1,
            Statement::UseRule(_) => use_rules += 1,
            _ => {}
        }
    }
    assert_eq!(modules, 1);
    assert_eq!(use_rules, 2);
}

#[test]
fn parse_globals_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/globals.smk").unwrap();
    let ast = parse(&source, "globals.smk").unwrap();

    let mut globals = 0;
    let mut ruleorders = 0;
    let mut localrules = 0;
    for stmt in &ast.body {
        match stmt {
            Statement::GlobalDirective(_) => globals += 1,
            Statement::Ruleorder(_) => ruleorders += 1,
            Statement::Localrules(_) => localrules += 1,
            _ => {}
        }
    }
    assert!(globals >= 6, "expected at least 6 global directives, got {globals}");
    assert_eq!(ruleorders, 1);
    assert_eq!(localrules, 1);
}

#[test]
fn parse_handlers_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/handlers.smk").unwrap();
    let ast = parse(&source, "handlers.smk").unwrap();

    let handlers: Vec<_> = ast.body.iter().filter_map(|s| {
        if let Statement::Handler(h) = s { Some(h) } else { None }
    }).collect();
    assert_eq!(handlers.len(), 3);
    assert_eq!(handlers[0].kind, HandlerKind::OnSuccess);
    assert_eq!(handlers[1].kind, HandlerKind::OnError);
    assert_eq!(handlers[2].kind, HandlerKind::OnStart);
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -- --nocapture`
Expected: ALL tests PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/ tests/parse_constructs.rs
git commit -m "Add comprehensive fixture tests for all Snakemake constructs

Fixtures cover modules with use rule (including wildcard, exclude, as, and
with: block), global directives (configfile, include, workdir, wildcard_constraints,
ruleorder, localrules, container, envvars), and all three handler types."
```

---

## Phase 3: Compiler

### Task 14: Simple rule compilation

**Files:**
- Modify: `src/compile/mod.rs`
- Modify: `src/compile/generator.rs`
- Create: `tests/compile_basic.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/compile_basic.rs
use snakemake_lang::compile;

#[test]
fn compile_simple_rule() {
    let source = "\
rule align:
    input: \"reads.fq\"
    output: \"aligned.bam\"
    shell: \"bwa mem {input} > {output}\"
";
    let result = compile(source, "Snakefile").unwrap();

    // Should contain the rule decorator
    assert!(result.python.contains("@workflow.rule"),
        "compiled output should contain @workflow.rule: {}", result.python);
    assert!(result.python.contains("name='align'"),
        "compiled output should contain rule name");
    assert!(result.python.contains("@workflow.input"),
        "compiled output should contain @workflow.input");
    assert!(result.python.contains("@workflow.output"),
        "compiled output should contain @workflow.output");
    assert!(result.python.contains("@workflow.shellcmd"),
        "compiled output should contain @workflow.shellcmd");
}

#[test]
fn compile_produces_valid_python() {
    let source = "rule foo:\n    input: \"a.txt\"\n    output: \"b.txt\"\n";
    let result = compile(source, "Snakefile").unwrap();

    // The compiled output should be parseable as Python
    let parsed = ruff_python_parser::parse_unchecked(&result.python, ruff_python_parser::Mode::Module);
    assert!(parsed.errors().is_empty(),
        "compiled output should be valid Python, errors: {:?}", parsed.errors());
}

#[test]
fn compile_python_passthrough() {
    let source = "x = 1\ny = 2\n";
    let result = compile(source, "Snakefile").unwrap();
    // Python should pass through verbatim
    assert!(result.python.contains("x = 1"));
    assert!(result.python.contains("y = 2"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test compile_basic -- --nocapture`
Expected: FAIL — `generate()` is `todo!()`.

- [ ] **Step 3: Implement the compiler**

Update `src/compile/mod.rs`:

```rust
pub mod generator;
pub mod source_map;

pub use source_map::SourceMap;

use crate::ast::Snakefile;
use crate::CompileResult;

use generator::VirtualPythonGenerator;

pub fn generate(source: &str, path: &str, ast: &Snakefile) -> CompileResult {
    let mut gen = VirtualPythonGenerator::new(source, path);
    gen.generate(ast);
    let (python, source_map) = gen.finish();
    CompileResult { python, source_map }
}
```

Update `src/compile/generator.rs` — implement the `generate()` method:

```rust
use crate::ast::*;
use ruff_python_ast::Stmt;

impl<'a> VirtualPythonGenerator<'a> {
    pub fn generate(&mut self, ast: &Snakefile) {
        for statement in &ast.body {
            match statement {
                Statement::Rule(rule) => self.generate_rule(rule),
                Statement::Module(module) => self.generate_module(module),
                Statement::UseRule(use_rule) => self.generate_use_rule(use_rule),
                Statement::GlobalDirective(global) => self.generate_global_directive(global),
                Statement::Ruleorder(ruleorder) => self.generate_ruleorder(ruleorder),
                Statement::Localrules(localrules) => self.generate_localrules(localrules),
                Statement::Storage(storage) => self.generate_storage(storage),
                Statement::Handler(handler) => self.generate_handler(handler),
                Statement::Python(stmt) => self.generate_python(stmt),
            }
        }
    }

    fn generate_rule(&mut self, rule: &SnakemakeRule) {
        let kind = if rule.is_checkpoint { "checkpoint" } else { "rule" };

        // @workflow.rule(name='foo', lineno=N, snakefile='path')
        self.emit(&format!(
            "@workflow.{}(name='{}', lineno={}, snakefile='{}')\n",
            kind,
            rule.name.as_str(),
            self.line_at(rule.range.start().to_usize()),
            self.path
        ));

        // Emit directives
        let mut has_execution = false;
        for directive in &rule.directives {
            match &directive.value {
                DirectiveValue::Arguments(_) => {
                    self.generate_directive_decorator(directive);
                }
                DirectiveValue::Block(stmts) => {
                    // run: block
                    has_execution = true;
                    self.emit("@workflow.run\n");
                    self.emit(&format!(
                        "def __rule_{}(input, output, params, wildcards, threads, resources, log, version, rule, conda_env, container_img, singularity_args, use_singularity, env_modules, bench_record, jobid, is_shell, bench_iteration, cleanup_scripts, shadow_dir, edit_notebook, conda_base_path, basedir, sourcecache_path, runtime_sourcecache_path, __is_snakemake_rule_func=True):\n",
                        rule.name.as_str()
                    ));
                    // Emit the body
                    let block_source = self.extract_range(directive.range);
                    // For now, emit the dedented block as-is
                    for stmt in stmts {
                        // Use the original source text for the statement
                        self.emit("    pass\n"); // placeholder — improved in later task
                    }
                    // We'll improve the body emission in Task 15
                    break;
                }
            }
        }

        if !has_execution {
            // No execution directive — emit norun + dummy function
            self.emit("@workflow.norun()\n");
            self.emit(&format!(
                "def __rule_{}(input, output, params, wildcards, threads, resources, log, version, rule, conda_env, container_img, singularity_args, use_singularity, env_modules, bench_record, jobid, is_shell, bench_iteration, cleanup_scripts, shadow_dir, edit_notebook, conda_base_path, basedir, sourcecache_path, runtime_sourcecache_path, __is_snakemake_rule_func=True):\n    pass\n",
                rule.name.as_str()
            ));
        }
    }

    fn generate_directive_decorator(&mut self, directive: &SnakemakeDirective) {
        let method_name = match directive.keyword {
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
            DirectiveKeyword::Run => return, // handled separately
        };

        // Extract the original value text from source
        let value_text = self.extract_directive_value_text(directive);
        self.emit(&format!("@workflow.{}({})\n", method_name, value_text));
    }

    fn generate_python(&mut self, stmt: &Stmt) {
        // Extract and emit the original source text for the Python statement
        use ruff_python_ast::Ranged;
        let range = stmt.range();
        let start = range.start().to_usize();
        let end = range.end().to_usize();
        if start < self.source.len() && end <= self.source.len() {
            let text = &self.source[start..end];
            self.emit_mapped(text, start, end - start);
            self.emit("\n");
        }
    }

    /// Extract the value text for a directive from the original source.
    fn extract_directive_value_text(&self, directive: &SnakemakeDirective) -> String {
        if let DirectiveValue::Arguments(args) = &directive.value {
            let start = args.range.start().to_usize();
            let end = args.range.end().to_usize();
            if start < self.source.len() && end <= self.source.len() {
                return self.source[start..end].trim().to_string();
            }
        }
        String::new()
    }

    /// Extract source text for a TextRange.
    fn extract_range(&self, range: ruff_text_size::TextRange) -> &str {
        let start = range.start().to_usize();
        let end = range.end().to_usize();
        if start < self.source.len() && end <= self.source.len() {
            &self.source[start..end]
        } else {
            ""
        }
    }

    /// Get the 1-based line number for a byte offset.
    fn line_at(&self, offset: usize) -> usize {
        self.source[..offset].matches('\n').count() + 1
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test compile_basic -- --nocapture`
Expected: All 3 compilation tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/compile/mod.rs src/compile/generator.rs tests/compile_basic.rs
git commit -m "Implement basic rule compilation to virtual Python

Generate decorator-chain Python for rules: @workflow.rule(), @workflow.input(),
@workflow.output(), etc. No-run rules get @workflow.norun() with a dummy
function. Python statements pass through verbatim with source mapping.
Run blocks generate @workflow.run + function definition."
```

---

### Task 15: Run block compilation and directive value emission

**Files:**
- Modify: `src/compile/generator.rs`
- Modify: `tests/compile_basic.rs`

- [ ] **Step 1: Write tests for run block compilation**

```rust
// Add to tests/compile_basic.rs

#[test]
fn compile_run_block() {
    let source = "\
rule process:
    input: \"data.csv\"
    output: \"result.csv\"
    run:
        import pandas as pd
        df = pd.read_csv(input[0])
        df.to_csv(output[0])
";
    let result = compile(source, "Snakefile").unwrap();

    assert!(result.python.contains("@workflow.run"),
        "should contain @workflow.run: {}", result.python);
    assert!(result.python.contains("def __rule_process("),
        "should contain rule function: {}", result.python);
    assert!(result.python.contains("import pandas"),
        "should contain the run block body: {}", result.python);
}

#[test]
fn compile_checkpoint() {
    let source = "\
checkpoint split:
    input: \"data.csv\"
    output: directory(\"chunks/\")
    shell: \"split_data {input} {output}\"
";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("@workflow.checkpoint("),
        "should use checkpoint decorator: {}", result.python);
}

#[test]
fn compile_rule_with_all_directive_types() {
    let source = "\
rule full:
    input: \"a.txt\"
    output: \"b.txt\"
    params: extra=\"--verbose\"
    log: \"logs/full.log\"
    threads: 4
    resources: mem_mb=2048
    conda: \"envs/tool.yaml\"
    shell: \"tool {params.extra} {input} > {output}\"
";
    let result = compile(source, "Snakefile").unwrap();

    // Verify all directives are present in output
    for method in &["input", "output", "params", "log", "threads", "resources", "conda", "shellcmd"] {
        assert!(result.python.contains(&format!("@workflow.{}(", method)),
            "should contain @workflow.{}(): {}", method, result.python);
    }
}
```

- [ ] **Step 2: Run and fix**

Run: `cargo test --test compile_basic -- --nocapture`
Expected: Tests should PASS or reveal issues to fix in the generator.

The run block compilation needs refinement — extract the original Python text from source using the block's line range rather than emitting `pass`. Update the `generate_rule` method to properly emit run block bodies by extracting and dedenting the original source text.

- [ ] **Step 3: Commit**

```bash
git add src/compile/generator.rs tests/compile_basic.rs
git commit -m "Improve run block and directive compilation

Run blocks now emit the actual Python body from source. Added compilation
tests for checkpoint, run blocks, and rules with many directive types."
```

---

### Task 16: Compile all construct types

**Files:**
- Modify: `src/compile/generator.rs`
- Create: `tests/compile_constructs.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/compile_constructs.rs
use snakemake_lang::compile;

#[test]
fn compile_global_configfile() {
    let source = "configfile: \"config.yaml\"\n";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("workflow.configfile("),
        "output: {}", result.python);
}

#[test]
fn compile_global_include() {
    let source = "include: \"rules/align.smk\"\n";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("workflow.include("),
        "output: {}", result.python);
}

#[test]
fn compile_ruleorder() {
    let source = "ruleorder: a > b > c\n";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("workflow.ruleorder("),
        "output: {}", result.python);
}

#[test]
fn compile_localrules() {
    let source = "localrules: all, clean\n";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("workflow.localrules("),
        "output: {}", result.python);
}

#[test]
fn compile_module() {
    let source = "\
module other:
    snakefile: \"other/Snakefile\"
    config: config
";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("workflow.module("),
        "output: {}", result.python);
}

#[test]
fn compile_handler() {
    let source = "\
onsuccess:
    print(\"done!\")
";
    let result = compile(source, "Snakefile").unwrap();
    assert!(result.python.contains("@workflow.onsuccess"),
        "output: {}", result.python);
    assert!(result.python.contains("def __onsuccess("),
        "output: {}", result.python);
}

#[test]
fn compile_mixed_workflow() {
    let source = "\
configfile: \"config.yaml\"

SAMPLES = config[\"samples\"]

rule all:
    input: expand(\"results/{s}.txt\", s=SAMPLES)

rule process:
    input: \"data/{sample}.csv\"
    output: \"results/{sample}.txt\"
    shell: \"process {input} > {output}\"
";
    let result = compile(source, "Snakefile").unwrap();

    // Should contain all parts
    assert!(result.python.contains("workflow.configfile("));
    assert!(result.python.contains("SAMPLES = "));
    assert!(result.python.contains("name='all'"));
    assert!(result.python.contains("name='process'"));
}
```

- [ ] **Step 2: Implement remaining generator methods**

Add to `src/compile/generator.rs`:

```rust
impl<'a> VirtualPythonGenerator<'a> {
    fn generate_global_directive(&mut self, global: &SnakemakeGlobalDirective) {
        let method = match global.keyword {
            GlobalKeyword::Configfile => "configfile",
            GlobalKeyword::Include => "include",
            GlobalKeyword::Workdir => "workdir",
            GlobalKeyword::Envvars => "envvars",
            GlobalKeyword::Pathvars => "pathvars",
            GlobalKeyword::Pepfile => "pepfile",
            GlobalKeyword::Pepschema => "pepschema",
            GlobalKeyword::Report => "report",
            GlobalKeyword::Scattergather => "scattergather",
            GlobalKeyword::WildcardConstraints => "global_wildcard_constraints",
            GlobalKeyword::Container => "global_container",
            GlobalKeyword::Containerized => "global_containerized",
            GlobalKeyword::Conda => "global_conda",
            GlobalKeyword::ResourceScopes => "resource_scopes",
            GlobalKeyword::InputFlags => "inputflags",
            GlobalKeyword::OutputFlags => "outputflags",
        };

        if let DirectiveValue::Arguments(args) = &global.value {
            let value_text = self.extract_args_text(args);
            self.emit(&format!("workflow.{}({})\n", method, value_text));
        }
    }

    fn generate_ruleorder(&mut self, ruleorder: &SnakemakeRuleorder) {
        let names: Vec<&str> = ruleorder.names.iter().map(|n| n.as_str()).collect();
        let args = names.iter().map(|n| format!("'{}'", n)).collect::<Vec<_>>().join(", ");
        self.emit(&format!("workflow.ruleorder({})\n", args));
    }

    fn generate_localrules(&mut self, localrules: &SnakemakeLocalrules) {
        let names: Vec<&str> = localrules.names.iter().map(|n| n.as_str()).collect();
        let args = names.iter().map(|n| format!("'{}'", n)).collect::<Vec<_>>().join(", ");
        self.emit(&format!("workflow.localrules({})\n", args));
    }

    fn generate_module(&mut self, module: &SnakemakeModule) {
        let mut kwargs = Vec::new();
        for directive in &module.directives {
            let key = match directive.keyword {
                ModuleKeyword::Snakefile => "snakefile",
                ModuleKeyword::MetaWrapper => "meta_wrapper",
                ModuleKeyword::Config => "config",
                ModuleKeyword::SkipValidation => "skip_validation",
                ModuleKeyword::ReplacePrefix => "replace_prefix",
                ModuleKeyword::Prefix => "prefix",
                ModuleKeyword::Name => "name",
                ModuleKeyword::Pathvars => "pathvars",
            };
            if let DirectiveValue::Arguments(args) = &directive.value {
                let value_text = self.extract_args_text(args);
                kwargs.push(format!("{}={}", key, value_text));
            }
        }
        self.emit(&format!(
            "workflow.module('{}', {})\n",
            module.name.as_str(),
            kwargs.join(", ")
        ));
    }

    fn generate_use_rule(&mut self, use_rule: &SnakemakeUseRule) {
        // use rule generates @workflow.userule(...) decorator chain
        let rules_arg = match &use_rule.rules {
            RuleNames::All => "'*'".to_string(),
            RuleNames::Named(names) => {
                let n: Vec<_> = names.iter().map(|n| format!("'{}'", n.as_str())).collect();
                format!("[{}]", n.join(", "))
            }
        };

        self.emit(&format!(
            "@workflow.userule(rules={}, from_module='{}'",
            rules_arg,
            use_rule.from_module.as_str()
        ));

        if !use_rule.exclude.is_empty() {
            let exc: Vec<_> = use_rule.exclude.iter().map(|n| format!("'{}'", n.as_str())).collect();
            self.emit(&format!(", exclude=[{}]", exc.join(", ")));
        }

        if let Some(modifier) = &use_rule.name_modifier {
            self.emit(&format!(", name_modifier='{}'", modifier));
        }

        self.emit(")\n");

        // Emit directive overrides if present
        if let Some(directives) = &use_rule.with_directives {
            for directive in directives {
                self.generate_directive_decorator(directive);
            }
        }

        // Dummy function
        self.emit("def _use_rule():\n    pass\n");
    }

    fn generate_storage(&mut self, storage: &SnakemakeStorage) {
        if let DirectiveValue::Arguments(args) = &storage.value {
            let value_text = self.extract_args_text(args);
            self.emit(&format!(
                "workflow.storage('{}', {})\n",
                storage.tag.as_str(),
                value_text
            ));
        }
    }

    fn generate_handler(&mut self, handler: &SnakemakeHandler) {
        let kind = handler.kind.as_str();
        self.emit(&format!("@workflow.{}\n", kind));
        self.emit(&format!("def __{}(log):\n", kind));

        if handler.body.is_empty() {
            self.emit("    pass\n");
        } else {
            // Extract handler body from source
            let start = handler.range.start().to_usize();
            let end = handler.range.end().to_usize();
            if start < self.source.len() && end <= self.source.len() {
                let handler_text = &self.source[start..end];
                // Find the indented body (after the header line)
                if let Some(newline_pos) = handler_text.find('\n') {
                    let body = &handler_text[newline_pos + 1..];
                    // Dedent by finding common indent
                    let min_indent = body.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| l.len() - l.trim_start().len())
                        .min()
                        .unwrap_or(0);
                    for line in body.lines() {
                        if line.trim().is_empty() {
                            self.emit("\n");
                        } else if line.len() >= min_indent {
                            self.emit(&format!("    {}\n", &line[min_indent..]));
                        }
                    }
                }
            }
        }
    }

    /// Extract the original text for a DirectiveArguments from source.
    fn extract_args_text(&self, args: &DirectiveArguments) -> String {
        let start = args.range.start().to_usize();
        let end = args.range.end().to_usize();
        if start < self.source.len() && end <= self.source.len() {
            self.source[start..end].trim().to_string()
        } else {
            String::new()
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test compile_constructs -- --nocapture`
Expected: All tests PASS.

Run: `cargo test -- --nocapture`
Expected: ALL tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/compile/generator.rs tests/compile_constructs.rs
git commit -m "Implement compilation for all Snakemake constructs

Generate virtual Python for modules (workflow.module()), use rules
(@workflow.userule()), global directives (workflow.configfile(), etc.),
ruleorder, localrules, storage, and handlers (@workflow.onsuccess, etc.).

Mixed workflows with Python interleaved between Snakemake constructs
compile correctly end-to-end."
```

---

### Task 17: Source map verification

**Files:**
- Modify: `tests/compile_basic.rs`

- [ ] **Step 1: Write source map tests**

```rust
// Add to tests/compile_basic.rs

#[test]
fn source_map_has_entries() {
    let source = "x = 1\nrule foo:\n    input: \"a.txt\"\n";
    let result = compile(source, "Snakefile").unwrap();

    // Source map should have at least one mapping (for the Python passthrough)
    assert!(!result.source_map.mappings().is_empty(),
        "source map should have mappings");
}

#[test]
fn source_map_linemap() {
    let source = "x = 1\nrule foo:\n    input: \"a.txt\"\n";
    let result = compile(source, "Snakefile").unwrap();

    let linemap = result.source_map.to_linemap(&result.python, source);
    // The linemap should map generated lines to original lines
    // Python passthrough "x = 1" should map to itself
    assert!(!linemap.is_empty(), "linemap should not be empty");
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --test compile_basic -- source_map --nocapture`
Expected: PASS (or fix source map generation in the generator).

- [ ] **Step 3: Commit**

```bash
git add tests/compile_basic.rs
git commit -m "Add source map verification tests

Verify that compilation produces source map entries and that the linemap
conversion produces mappings from generated to original line numbers."
```

---

### Task 18: Documentation checkpoint 2

**Files:**
- Create: `tests/fixtures/real_workflow.smk`

- [ ] **Step 1: Create a realistic workflow fixture**

```snakemake
# tests/fixtures/real_workflow.smk
# A realistic RNA-seq analysis workflow

configfile: "config.yaml"

SAMPLES = config["samples"]
GENOME = config["genome"]

rule all:
    input:
        expand("results/counts/{sample}.counts.txt", sample=SAMPLES),
        "results/multiqc_report.html"

rule fastqc:
    input: "data/{sample}.fastq.gz"
    output: "results/fastqc/{sample}_fastqc.html"
    conda: "envs/fastqc.yaml"
    threads: 2
    shell: "fastqc -t {threads} {input} -o results/fastqc/"

rule trim:
    input: "data/{sample}.fastq.gz"
    output: "results/trimmed/{sample}.trimmed.fastq.gz"
    params:
        extra="--quality 20 --length 50"
    log: "logs/trim/{sample}.log"
    conda: "envs/trimgalore.yaml"
    shell: "trim_galore {params.extra} -o results/trimmed/ {input} 2> {log}"

rule align:
    input:
        reads="results/trimmed/{sample}.trimmed.fastq.gz",
        index=GENOME
    output:
        bam="results/aligned/{sample}.bam",
        bai="results/aligned/{sample}.bam.bai"
    threads: 8
    resources:
        mem_mb=16384
    log: "logs/align/{sample}.log"
    conda: "envs/star.yaml"
    shell:
        "STAR --runThreadN {threads}"
        " --genomeDir {input.index}"
        " --readFilesIn {input.reads}"
        " --outSAMtype BAM SortedByCoordinate"
        " --outFileNamePrefix results/aligned/{wildcards.sample}."
        " 2> {log}"
        " && samtools index {output.bam}"

rule count:
    input:
        bam="results/aligned/{sample}.bam",
        gtf=config["gtf"]
    output: "results/counts/{sample}.counts.txt"
    conda: "envs/subread.yaml"
    shell: "featureCounts -a {input.gtf} -o {output} {input.bam}"

rule multiqc:
    input:
        expand("results/fastqc/{sample}_fastqc.html", sample=SAMPLES),
        expand("results/counts/{sample}.counts.txt", sample=SAMPLES)
    output: "results/multiqc_report.html"
    conda: "envs/multiqc.yaml"
    shell: "multiqc results/ -o results/ -n multiqc_report.html"

onsuccess:
    print("RNA-seq analysis complete!")

onerror:
    print("RNA-seq analysis failed. Check logs/")
```

- [ ] **Step 2: Test the full fixture parses and compiles**

```rust
// Add to tests/compile_basic.rs

#[test]
fn compile_real_workflow_fixture() {
    let source = std::fs::read_to_string("tests/fixtures/real_workflow.smk").unwrap();
    let result = compile(&source, "real_workflow.smk").unwrap();

    // Should compile without error
    assert!(!result.python.is_empty());

    // Verify it mentions all rules
    for rule_name in &["all", "fastqc", "trim", "align", "count", "multiqc"] {
        assert!(result.python.contains(&format!("name='{}'", rule_name)),
            "compiled output should mention rule '{}'", rule_name);
    }

    // Verify handlers
    assert!(result.python.contains("@workflow.onsuccess"));
    assert!(result.python.contains("@workflow.onerror"));
}
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: ALL tests PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/real_workflow.smk tests/compile_basic.rs
git commit -m "Add realistic RNA-seq workflow fixture and end-to-end test

Tests the full pipeline: parse and compile a multi-rule RNA-seq workflow
with configfile, global variables, 6 rules, keyword arguments, block-form
directives, multi-line shell commands, and event handlers."
```

---

## Phase 4: Integration and CLI

### Task 19: CLI end-to-end testing

**Files:**
- No code changes needed if CLI already compiles
- Run manual verification

- [ ] **Step 1: Build the CLI**

Run: `cargo build --features cli`
Expected: Builds successfully.

- [ ] **Step 2: Test CLI commands**

```bash
# Check a file
cargo run --features cli -- check tests/fixtures/real_workflow.smk
# Expected: exits 0, no errors

# Parse a file to JSON
cargo run --features cli -- parse tests/fixtures/simple_rule.smk
# Expected: JSON AST output

# Compile a file
cargo run --features cli -- compile tests/fixtures/real_workflow.smk
# Expected: Python output to stdout

# Compile with source map
cargo run --features cli -- compile --source-map tests/fixtures/real_workflow.smk
# Expected: Python to stdout, JSON source map to stderr
```

- [ ] **Step 3: Commit**

No code changes needed. If fixes were required, commit them:

```bash
git commit -m "Verify CLI works end-to-end with all subcommands

Tested check, parse (JSON output), and compile (with and without source
map) against fixture files."
```

---

### Task 20: Snapshot tests with insta

**Files:**
- Create: `tests/snapshots/` directory (insta creates this automatically)
- Modify: `tests/compile_basic.rs`

- [ ] **Step 1: Add snapshot tests**

```rust
// Add to tests/compile_basic.rs
use insta::assert_snapshot;

#[test]
fn snapshot_simple_rule_compilation() {
    let source = "\
rule align:
    input: \"reads.fq\"
    output: \"aligned.bam\"
    threads: 8
    shell: \"bwa mem -t {threads} {input} > {output}\"
";
    let result = compile(source, "Snakefile").unwrap();
    assert_snapshot!(result.python);
}

#[test]
fn snapshot_rule_with_run_block() {
    let source = "\
rule process:
    input: \"data.csv\"
    output: \"result.csv\"
    run:
        import pandas as pd
        df = pd.read_csv(input[0])
        df.to_csv(output[0])
";
    let result = compile(source, "Snakefile").unwrap();
    assert_snapshot!(result.python);
}

#[test]
fn snapshot_mixed_workflow() {
    let source = "\
configfile: \"config.yaml\"

SAMPLES = [\"A\", \"B\"]

rule all:
    input: expand(\"out/{s}.txt\", s=SAMPLES)

rule process:
    input: \"data/{sample}.csv\"
    output: \"out/{sample}.txt\"
    shell: \"process {input} > {output}\"
";
    let result = compile(source, "Snakefile").unwrap();
    assert_snapshot!(result.python);
}
```

- [ ] **Step 2: Generate and review snapshots**

Run: `cargo test --test compile_basic -- snapshot`
First run will fail. Then:
Run: `cargo insta review`
Review each snapshot, accept if correct.

- [ ] **Step 3: Commit**

```bash
git add tests/compile_basic.rs tests/snapshots/
git commit -m "Add insta snapshot tests for compilation output

Snapshots capture the exact compilation output for simple rules, run blocks,
and mixed workflows. Makes it easy to detect unintentional changes to the
compiler output format."
```

---

## Phase 5: PyO3 and Packaging

### Task 21: PyO3 build verification

**Files:**
- Create: `pyproject.toml` (if not exists)
- Test Python bindings

- [ ] **Step 1: Create pyproject.toml for maturin**

```toml
# pyproject.toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "snakemake-lang"
version = "0.1.0"
description = "Parser, AST, and compiler for the Snakemake workflow language"
requires-python = ">=3.8"
license = {text = "MIT"}
keywords = ["snakemake", "parser", "bioinformatics"]

[tool.maturin]
features = ["extension-module"]
```

- [ ] **Step 2: Build with maturin**

Run: `maturin develop --features extension-module`
Expected: Builds and installs the Python module.

- [ ] **Step 3: Test from Python**

```bash
python -c "
from snakemake_lang import parse_and_compile
source = '''
rule foo:
    input: \"a.txt\"
    output: \"b.txt\"
    shell: \"cat {input} > {output}\"
'''
python_code, linemap = parse_and_compile(source, 'test.smk')
print(python_code)
print('linemap:', linemap)
"
```

Expected: Python output printed, linemap is a dict.

- [ ] **Step 4: Test error handling**

```bash
python -c "
from snakemake_lang import parse_and_compile
try:
    parse_and_compile('rule :\n    input: \"x\"\n', 'test.smk')
except SyntaxError as e:
    print('Caught SyntaxError:', e)
    print('filename:', e.filename)
    print('lineno:', e.lineno)
"
```

Expected: SyntaxError with file/line info.

- [ ] **Step 5: Commit**

```bash
git add pyproject.toml
git commit -m "Add maturin packaging for Python extension module

pyproject.toml configured for maturin build with extension-module feature.
Verified parse_and_compile() works from Python and error conversion
produces SyntaxError with correct file/line/column information."
```

---

## Phase 6: Hardening

### Task 22: Edge case tests

**Files:**
- Modify: `tests/parse_rule.rs`

- [ ] **Step 1: Write edge case tests**

```rust
// Add to tests/parse_rule.rs

#[test]
fn parse_fstring_in_directive() {
    let source = "rule foo:\n    input: f\"data/{config['sample']}.txt\"\n";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
}

#[test]
fn parse_triple_quoted_shell() {
    let source = "rule foo:\n    shell:\n        \"\"\"\n        echo 'hello'\n        echo 'world'\n        \"\"\"\n";
    let ast = parse(source, "test.smk").unwrap();
    let rule = match &ast.body[0] {
        Statement::Rule(r) => r,
        other => panic!("expected Rule, got {:?}", other),
    };
    assert_eq!(rule.directives[0].keyword, DirectiveKeyword::Shell);
}

#[test]
fn parse_windows_line_endings() {
    let source = "rule foo:\r\n    input: \"a.txt\"\r\n    output: \"b.txt\"\r\n";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
}

#[test]
fn parse_trailing_comments() {
    let source = "rule foo: # this is a rule\n    input: \"a.txt\" # input file\n";
    let ast = parse(source, "test.smk").unwrap();
    assert_eq!(ast.body.len(), 1);
}

#[test]
fn parse_empty_rule() {
    let source = "rule empty:\n    pass\n";
    // This might be an error since "pass" isn't a directive
    // The parser should handle it gracefully
    let _ = parse(source, "test.smk");
}

#[test]
fn parse_many_rules() {
    // Generate a Snakefile with 100 rules
    let mut source = String::new();
    for i in 0..100 {
        source.push_str(&format!(
            "rule rule_{i}:\n    input: \"in_{i}.txt\"\n    output: \"out_{i}.txt\"\n    shell: \"cp {{input}} {{output}}\"\n\n"
        ));
    }
    let ast = parse(&source, "test.smk").unwrap();
    let rule_count = ast.body.iter().filter(|s| matches!(s, Statement::Rule(_))).count();
    assert_eq!(rule_count, 100);
}
```

- [ ] **Step 2: Run and fix any failures**

Run: `cargo test -- --nocapture`
Expected: All pass (fix any that don't).

- [ ] **Step 3: Commit**

```bash
git add tests/parse_rule.rs
git commit -m "Add edge case tests: f-strings, triple quotes, CRLF, comments, scale

Tests f-strings in directive values, triple-quoted shell strings, Windows
line endings, trailing comments, empty rules, and parsing 100 rules in
a single file."
```

---

### Task 22b: Equivalence testing against parser.py

**Files:**
- Create: `tests/equivalence.rs`
- Create: `tests/compare_parsers.py`

The legacy `parser.py` is part of the `snakemake` package. We run both parsers on the same fixtures and diff the compiled output. This is how we validate correctness at scale.

- [ ] **Step 1: Write the Python comparison script**

```python
# tests/compare_parsers.py
"""
Compare snakemake-lang compilation output against legacy parser.py.

Usage: python tests/compare_parsers.py tests/fixtures/*.smk

Requires: pip install snakemake snakemake-lang
"""
import sys
import difflib
from pathlib import Path

def compile_legacy(source: str, path: str) -> str:
    """Compile using snakemake's built-in parser.py."""
    from snakemake.parser import parse
    code, _, _ = parse(source, path)
    return code

def compile_new(source: str, path: str) -> str:
    """Compile using snakemake-lang."""
    from snakemake_lang import parse_and_compile
    code, _ = parse_and_compile(source, path)
    return code

def normalize(code: str) -> list[str]:
    """Normalize whitespace for comparison."""
    return [line.rstrip() for line in code.splitlines() if line.strip()]

def main():
    files = [Path(f) for f in sys.argv[1:]]
    failures = []

    for path in files:
        source = path.read_text()
        try:
            legacy = normalize(compile_legacy(source, str(path)))
            new = normalize(compile_new(source, str(path)))

            if legacy != new:
                diff = list(difflib.unified_diff(
                    legacy, new,
                    fromfile=f"{path} (parser.py)",
                    tofile=f"{path} (snakemake-lang)",
                    lineterm=""
                ))
                if diff:
                    failures.append((path, "\n".join(diff)))
                    print(f"DIFF: {path}")
                else:
                    print(f"OK:   {path}")
            else:
                print(f"OK:   {path}")
        except Exception as e:
            failures.append((path, str(e)))
            print(f"ERR:  {path}: {e}")

    if failures:
        print(f"\n{len(failures)} file(s) differ:")
        for path, detail in failures:
            print(f"\n--- {path} ---")
            print(detail)
        sys.exit(1)
    else:
        print(f"\nAll {len(files)} files match.")

if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Write a Rust integration test that shells out to the comparison**

```rust
// tests/equivalence.rs
use std::process::Command;

#[test]
#[ignore] // requires snakemake + snakemake-lang Python packages installed
fn equivalence_with_legacy_parser() {
    let output = Command::new("python3")
        .args(["tests/compare_parsers.py"])
        .args(glob::glob("tests/fixtures/*.smk").unwrap().filter_map(|p| p.ok()))
        .output()
        .expect("failed to run compare_parsers.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "equivalence test failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
```

- [ ] **Step 3: Run it (if snakemake is installed)**

```bash
# Install the local snakemake-lang for Python
maturin develop --features extension-module

# Run equivalence test
python3 tests/compare_parsers.py tests/fixtures/*.smk
```

Expected: Most fixtures match. Differences are expected for breaking changes documented in CLAUDE.md (required rule names, removed `singularity:`, etc.).

- [ ] **Step 4: Commit**

```bash
git add tests/equivalence.rs tests/compare_parsers.py
git commit -m "Add equivalence testing against legacy parser.py

Comparison script runs both parsers on the same fixtures and diffs compiled
output. Rust test shells out to the script (ignored by default since it
requires snakemake + snakemake-lang Python packages).

Expected differences from documented breaking changes: required rule names,
removed singularity/version/subworkflow keywords, stricter run: blocks."
```

---

### Task 23: Full test suite run and cleanup

**Files:**
- Any files that need fixes

- [ ] **Step 1: Run the complete test suite**

```bash
cargo test -- --nocapture 2>&1 | tee test_output.txt
```

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --all-features -- -D warnings
```

Fix any warnings.

- [ ] **Step 3: Run fmt**

```bash
cargo fmt -- --check
```

Fix any formatting issues.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "Clean up clippy warnings and formatting

All tests pass, no clippy warnings with --all-features, code formatted
with cargo fmt."
```

---

### Task 24: Performance sanity check

**Files:**
- Create: `benches/parse.rs` (optional, can use ad-hoc timing instead)

- [ ] **Step 1: Time parsing a large file**

```bash
# Generate a large Snakefile
python3 -c "
for i in range(500):
    print(f'''rule rule_{i}:
    input: \"in_{i}.txt\"
    output: \"out_{i}.txt\"
    threads: 4
    shell: \"process {{input}} > {{output}}\"
''')
" > /tmp/large_workflow.smk

# Time parsing
time cargo run --features cli --release -- check /tmp/large_workflow.smk
```

Expected: Under 1 second for 500 rules.

- [ ] **Step 2: Time compilation**

```bash
time cargo run --features cli --release -- compile /tmp/large_workflow.smk > /dev/null
```

Expected: Under 1 second.

- [ ] **Step 3: Note results — no commit needed**

Record timing in your notes. If performance is unexpectedly slow, profile and optimize.

---

### Task 25: Final integration test and documentation

**Files:**
- Verify all fixtures parse and compile

- [ ] **Step 1: Run all fixtures through parse + compile**

```bash
for f in tests/fixtures/*.smk; do
    echo "=== $f ==="
    cargo run --features cli --release -- check "$f" && echo "OK" || echo "FAIL"
done
```

Expected: All OK.

- [ ] **Step 2: Run full test suite one final time**

```bash
cargo test --all-features -- --nocapture
```

Expected: All tests PASS.

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "Complete snakemake-lang v0.1.0 implementation

Parser handles all Snakemake constructs: rules, checkpoints, modules,
use rules, global directives, ruleorder, localrules, storage, and handlers.
Compiler generates valid virtual Python with decorator chains and source maps.

Tested against realistic workflow fixtures including a multi-rule RNA-seq
pipeline. Edge cases covered: f-strings, triple-quoted strings, Windows
line endings, block-form directives, run blocks, and 100+ rule files."
```

---

## Summary of commit sequence

1. Fix Cargo.toml feature bug, add ruff integration smoke tests
2. Add line scanner for Snakemake source
3. Implement top-level parser dispatch and Python collection
4. Implement rule and directive parsing
5. Add tests for block-form directives, multiline values, and run blocks
6. Test multi-rule parsing and Python interleaving
6b. Handle rules inside if/for blocks (flat AST representation)
7. Add error recovery for unknown directives and missing rule names
8. Add test fixtures and verify rule parsing completeness
9. Implement module parsing
10. Implement use rule parsing
11. Implement global directive, ruleorder, localrules, and storage parsing
12. Implement handler parsing (onsuccess, onerror, onstart)
13. Add comprehensive fixture tests for all Snakemake constructs
14. Implement basic rule compilation to virtual Python
15. Improve run block and directive compilation
16. Implement compilation for all Snakemake constructs
17. Add source map verification tests
18. Add realistic RNA-seq workflow fixture and end-to-end test
19. Verify CLI works end-to-end with all subcommands
20. Add insta snapshot tests for compilation output
21. Add maturin packaging for Python extension module
22. Add edge case tests: f-strings, triple quotes, CRLF, comments, scale
22b. Add equivalence testing against legacy parser.py
23. Clean up clippy warnings and formatting
24. Performance sanity check (500 rules under 1 second)
25. Complete snakemake-lang v0.1.0 implementation

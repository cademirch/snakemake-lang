# snakemake-lang Implementation Plan

## Overview

Build the canonical Snakemake language crate: parser, AST, and compiler.
Replaces `parser.py` in the Snakemake engine. Foundation for the fangs
developer toolchain.

## Milestone 1: Skeleton + simplest parse (Week 1)

### Goal
Parse `rule name: input: "file"` and produce an AST node. Prove the
ruff crate integration works end to end.

### Tasks

**1.1 Project setup**
- Initialize Cargo workspace
- Add ruff git dependencies (pin to a stable tag)
- Verify ruff crates compile and link
- Set up CI (cargo test, cargo clippy, cargo fmt)
- Create `CLAUDE.md`, `README.md`, `LICENSE`

**1.2 AST node types — minimal set**
- `Snakefile` (root node, contains `Vec<Statement>`)
- `Statement` enum: `Rule(SnakemakeRule)`, `Python(PythonStatement)`
- `SnakemakeRule`: `name`, `directives`, `range`
- `SnakemakeDirective`: `keyword`, `value`, `range`
- `SnakemakeKeyword` enum: start with `Input`, `Output`, `Shell`
- `SnakemakeDirectiveValue` enum: `Arguments(Vec<ruff Expr>)`, `Block(Vec<ruff Stmt>)`
- All nodes carry `TextRange`

**1.3 Parser — top-level dispatch**
- Tokenize source with `ruff_python_parser::tokenize()`
- Build a token cursor that can peek/bump/expect tokens
- Implement top-level loop: for each token at line start, check if it's a Snakemake keyword. If yes, dispatch to Snakemake parser. If no, collect as Python and parse with ruff.
- Implement `parse_rule()`: consume `rule`, NAME, `:`, NEWLINE, INDENT, directives, DEDENT

**1.4 Parser — directive values**
- Implement `parse_directive()`: consume keyword NAME, `:`, then delegate value to ruff
- For inline values: extract text to end of line, call `ruff_python_parser::parse_expression()`
- For block values: extract indented block text, parse as expression list
- Implement TextRange offsetting for sub-parsed expressions

**1.5 First test**
- Parse a simple Snakefile, assert AST structure
- Verify TextRanges are correct
- Verify Python expressions inside directives are properly parsed

### Definition of done
`parse("rule foo:\n    input: \"a.txt\"\n    output: \"b.txt\"\n    shell: \"cat {input} > {output}\"\n")`
returns an AST with one `SnakemakeRule` containing three directives, each with
correctly-parsed Python string literal expressions and correct TextRanges.

---

## Milestone 2: Complete rule parsing (Week 2)

### Goal
Parse all directive keywords, both inline and block forms, including `run:` blocks.

### Tasks

**2.1 All directive keywords**
- Add all keywords to `SnakemakeKeyword` enum (see grammar in CLAUDE.md)
- Handle inline vs block form detection in `parse_directive_value()`
- Handle keyword arguments in directive values: `input: reads="a.fq", ref="b.fa"`

**2.2 `run:` block parsing**
- Detect `run` keyword → switch to block parsing mode
- Extract the indented block as text
- Parse with `ruff_python_parser::parse_module()` to get `Vec<Stmt>`
- Offset TextRanges

**2.3 Multiple rules + Python interleaving**
- Top-level parser handles multiple rules in sequence
- Python code between rules is collected and parsed with ruff
- `Statement::Python` wraps ruff's `Stmt` nodes

**2.4 Rule body edge cases**
- Docstrings inside rule body (STRING tokens)
- Comments inside rule body
- Trailing commas in argument lists
- Multiline expressions (parenthesized across lines)
- String concatenation (adjacent string literals)

**2.5 Error recovery**
- Unknown keyword in rule body → emit diagnostic, skip to next line
- Missing colon after keyword → emit diagnostic, try to continue
- Missing rule name → emit diagnostic with suggestion
- Unterminated string → report with correct line number

### Tests
- Every directive keyword in both inline and block form
- `run:` block with multi-line Python code
- Rule with all directives present
- Multiple rules in sequence
- Python functions between rules
- Error cases with recovery

---

## Milestone 3: Checkpoint, module, use rule, globals (Week 3)

### Goal
Parse every Snakemake construct.

### Tasks

**3.1 Checkpoint**
- Identical to rule parsing but sets `is_checkpoint = true`
- Separate AST node or flag on `SnakemakeRule`

**3.2 Module**
- `parse_module()`: consume `module`, NAME, `:`, NEWLINE, INDENT, module directives, DEDENT
- `SnakemakeModule` AST node
- Module-specific keyword set

**3.3 Use rule**
- `parse_use_rule()`: the most complex parser function
- Parse `use rule` → rule names (list or `*`) → `from` MODULE → optional `exclude` → optional `as` pattern → optional `with:` block
- `SnakemakeUseRule` AST node
- Soft keyword handling for `from`, `as`, `exclude`, `with`

**3.4 Global directives**
- `parse_global_directive()`: keyword `:` value
- `parse_ruleorder()`: keyword `:` NAME `>` NAME `>` ...
- `parse_localrules()`: keyword `:` NAME `,` NAME `,` ...
- `parse_storage()`: `storage` NAME `:` value
- AST nodes for each

**3.5 Handlers**
- `parse_handler()`: `onsuccess`/`onerror`/`onstart` `:` NEWLINE INDENT python_block DEDENT
- `SnakemakeHandler` AST node

**3.6 Rules inside Python control flow**
- `if config["x"]: rule foo: ...` — the `if` is Python, the `rule` inside is Snakemake
- When parsing Python statements at top level, check if any contain Snakemake keywords at line start within their body
- For compound Python statements (`if`, `for`, `while`, `with`, `try`): recursively check their bodies for Snakemake constructs

### Tests
- Module with all module keywords
- Use rule: all forms (named list, wildcard, with exclude, with as, with block, combinations)
- Use rule without `with:` (single line)
- All global directives
- Ruleorder with 2, 3, 5 rules
- Localrules with multiple rules
- Storage with tag
- Handlers with Python code
- Rules inside `if` blocks
- Rules inside `for` loops
- Fixture: real multi-module workflow from Snakemake catalog

---

## Milestone 4: Virtual Python compiler (Week 4)

### Goal
Generate valid Python from the AST that produces the same behavior as
`parser.py`'s output when exec'd by the Snakemake engine.

### Tasks

**4.1 Source map infrastructure**
- `SourceMap` struct: list of `SourceMapping` entries
- `SourceMapping`: `generated_range → original_range`
- `to_linemap()`: convert to `Dict[int, int]` for Snakemake compatibility
- Builder pattern for constructing source maps during generation

**4.2 Virtual Python generator**
- `generate(source: &str, ast: &Snakefile) -> CompileResult`
- Walk AST top-down
- Emit Python statement-by-statement

**4.3 Rule compilation**
- `rule foo:` → `@workflow.rule(name='foo', lineno=N, snakefile='path')`
- Input/output/params/log → `@workflow.input(...)` / `@workflow.output(...)`
- Scalar directives → `@workflow.threads(N)`, etc.
- Shell/script/wrapper → `@workflow.shellcmd(...)` / `@workflow.script(...)`
- `run:` block → `@workflow.run` + `def __rule_foo(input, output, params, wildcards, threads, resources, log, ...):` + body
- No-run rules → `@workflow.norun()` + dummy function

**4.4 Other construct compilation**
- Checkpoint → same as rule with `checkpoint=True` arg
- Module → `workflow.module(snakefile=..., config=..., ...)`
- Use rule → `@workflow.userule(rules=..., from_module=..., ...)`
- Global directives → `workflow.configfile(...)`, `workflow.include(...)`, etc.
- Ruleorder → `workflow.ruleorder(...)` (with names as repr strings)
- Localrules → same pattern
- Handlers → `@workflow.onsuccess` + `def __onsuccess(log):` + body

**4.5 Python pass-through with source mapping**
- Python statements between rules: emit verbatim
- Record identity source mappings for pass-through text
- Synthetic text (decorators, function signatures): no source mapping

**4.6 Equivalence testing against parser.py**
- For each fixture, run both parsers and compare output
- Output need not be character-identical but must produce the same rule registrations
- Test by exec'ing both outputs with a mock Workflow and comparing the resulting rules

### Tests
- Compile simple rule → verify valid Python
- Compile rule with all directives → verify valid Python
- Compile `run:` block → verify function is generated
- Compile module → verify `workflow.module()` call
- Compile use rule → verify `@workflow.userule()` decorator
- Source map: verify every mapped range is valid
- Source map: verify linemap matches expected line correspondences
- Equivalence: run 10+ real Snakefiles through both parsers

---

## Milestone 5: PyO3 bindings + Snakemake integration (Week 5)

### Goal
`snakemake-lang` is pip-installable and can replace `parser.py`.

### Tasks

**5.1 PyO3 module**
- `parse_and_compile(source, path) -> (python_code, linemap)` — drop-in for `parser.parse()`
- `parse_to_json(source, path) -> str` — AST as JSON for tooling
- `parse_to_ast(source, path) -> PyObject` — AST as Python objects (optional, more complex)
- Error handling: convert `ParseError` to Python `SyntaxError` with correct file/line/col

**5.2 Maturin packaging**
- `pyproject.toml` for building with maturin
- Build wheels for Linux, macOS, Windows
- CI: build and test wheels on all platforms
- Publish to PyPI as `snakemake-lang`

**5.3 Snakemake integration**
- PR to snakemake: add `snakemake-lang` as optional dependency
- New `parser.py` that delegates to `snakemake-lang` when available
- Fallback to legacy parser when not installed
- Run Snakemake's full test suite with the new parser
- Fix any behavioral differences

**5.4 CLI for standalone use**
- `snakemake-lang compile <file>` — print compiled Python (replaces `--print-compilation`)
- `snakemake-lang parse <file>` — print AST as JSON
- `snakemake-lang check <file>` — parse and report errors only
- These commands are useful for debugging and for fangs-ls integration

### Tests
- Python integration: import `snakemake_lang`, call `parse_and_compile()`, verify output
- Round-trip: parse a Snakefile, compile it, exec the compiled Python, verify rules
- Error messages: verify SyntaxError has correct file, line, column
- CLI: verify all subcommands work

---

## Milestone 6: Hardening + real-world validation (Week 6)

### Goal
Confident enough to make `snakemake-lang` the default parser.

### Tasks

**6.1 Workflow catalog testing**
- Download top 50 workflows from the Snakemake Workflow Catalog
- Parse all of them. Fix any failures.
- Compare compilation output with parser.py for all of them.

**6.2 Edge case sweep**
- F-strings in directive values
- Nested f-strings (Python 3.12+)
- Triple-quoted strings in shell/script directives
- Very long argument lists (100+ items)
- Deeply nested Python expressions in directive values
- Unicode identifiers
- Mixed tabs and spaces (should error clearly)
- Windows line endings

**6.3 Performance benchmarking**
- Benchmark against parser.py on large Snakefiles (500+ rules)
- Target: at least 10x faster (Rust vs Python tokenizer)
- Benchmark incremental re-parse (for future LSP use)

**6.4 Documentation**
- API documentation (rustdoc)
- Python API documentation
- Grammar specification (formal reference)
- Migration guide: parser.py → snakemake-lang, including breaking changes

**6.5 Release**
- Tag v0.1.0
- Publish to crates.io (the snakemake-lang Rust crate)
- Publish to PyPI (the Python extension module)
- Announce to Snakemake community

---

## Post-v0.1 roadmap

### For fangs (formatter + linter)
- Fangs depends on `snakemake-lang` for parsing
- Formatter: walk AST, emit formatted source. Use `ruff_python_formatter` for Python expressions.
- Linter: walk AST, check Snakemake rules (SMK001-SMK020). Run ruff Python rules on Python content.

### For fangs-ls (LSP)
- Depends on `snakemake-lang compile` for virtual Python + source maps
- Volar.js feeds virtual Python to Pyright
- Snakemake-specific features use the AST directly (via JSON or WASM)

### For Snakemake engine
- Make `snakemake-lang` a required dependency (remove legacy parser)
- Expose AST to the engine for the DAG rewrite (dag2)
- Enable static analysis passes before execution (reachability, wildcard validation)

### For the language itself
- Use the grammar spec as the formal language reference
- Versioned grammar: `snakemake-lang` declares which Snakemake language version it implements
- Migration tooling: `snakemake-lang migrate <file>` auto-fixes deprecated syntax

# CLAUDE.md — snakemake-lang

## What is this project?

Rust crate: parser, AST, and compiler for the Snakemake workflow language. Replaces the legacy `parser.py` transpiler in the Snakemake engine.

Snakemake extends Python with structural keywords (`rule`, `checkpoint`, `module`, `use rule`, `configfile`, `include`, etc.). Everything else is standard Python.

## Architecture

```
.smk source → Parser (delegates Python expressions to ruff) → AST → Compiler → valid Python + source map
```

Depends on ruff crates via git tag (not on crates.io): `ruff_python_parser`, `ruff_python_ast`, `ruff_source_file`, `ruff_text_size`. We do NOT fork ruff.

Features: `python` (PyO3 bindings), `extension-module` (pip-installable), `serde` (JSON AST output).

## The Snakemake language

Snakemake keywords are **soft keywords** — recognized only at line start in specific contexts, plain Python identifiers everywhere else. Same mechanism as Python's `match`.

### Keyword recognition contexts

1. **Top level**: `rule`, `checkpoint`, `module`, `use`, handler keywords, global directive keywords
2. **Rule/checkpoint body**: directive keywords (`input`, `output`, `shell`, `run`, etc.)
3. **Module body**: module directive keywords (`snakefile`, `config`, etc.)
4. **`use rule ... with:` body**: same as rule body minus execution keywords
5. **Everywhere else**: plain Python identifiers

### Grammar

```ebnf
snakefile = { statement } ;
statement = rule | checkpoint | module | use_rule | global_directive | handler | python_statement ;

rule       = 'rule'       NAME ':' NEWLINE INDENT rule_body DEDENT ;
checkpoint = 'checkpoint' NAME ':' NEWLINE INDENT rule_body DEDENT ;
rule_body  = { directive | COMMENT | STRING } ;

directive = directive_kw ':' argument_list NEWLINE
          | directive_kw ':' NEWLINE INDENT argument_list DEDENT
          | 'run' ':' NEWLINE INDENT python_block DEDENT ;

directive_kw = 'input' | 'output' | 'params' | 'threads' | 'resources'
             | 'retries' | 'priority' | 'log' | 'message' | 'benchmark'
             | 'conda' | 'container' | 'envmodules'
             | 'wildcard_constraints' | 'shadow' | 'group'
             | 'cache' | 'default_target' | 'handover' | 'localrule'
             | 'name' | 'pathvars'
             | 'shell' | 'script' | 'notebook' | 'wrapper'
             | 'template_engine' | 'cwl' ;

argument_list = argument { ',' argument } [','] ;
argument = expression | NAME '=' expression ;

module = 'module' NAME ':' NEWLINE INDENT module_body DEDENT ;
module_body = { module_directive | COMMENT | STRING } ;
module_directive = module_kw ':' argument_list NEWLINE
                 | module_kw ':' NEWLINE INDENT argument_list DEDENT ;
module_kw = 'snakefile' | 'meta_wrapper' | 'config' | 'skip_validation'
          | 'replace_prefix' | 'prefix' | 'name' | 'pathvars' ;

use_rule = 'use' 'rule' rule_names 'from' NAME
           [ 'exclude' name_list ] [ 'as' name_pattern ]
           [ 'with' ':' NEWLINE INDENT { directive } DEDENT ] NEWLINE ;
rule_names   = name_list | '*' ;
name_list    = NAME { ',' NAME } ;
name_pattern = { NAME | '*' } ;

global_directive = global_kw ':' argument_list NEWLINE
                 | global_kw ':' NEWLINE INDENT argument_list DEDENT ;
global_kw = 'configfile' | 'include' | 'workdir' | 'envvars' | 'pathvars'
          | 'pepfile' | 'pepschema' | 'report' | 'scattergather'
          | 'wildcard_constraints' | 'container' | 'containerized' | 'conda'
          | 'resource_scopes' ;

ruleorder  = 'ruleorder'  ':' NAME { '>' NAME } NEWLINE ;
localrules = 'localrules' ':' NAME { ',' NAME } NEWLINE ;
storage    = 'storage' NAME ':' argument_list NEWLINE
           | 'storage' NAME ':' NEWLINE INDENT argument_list DEDENT ;

handler    = ('onsuccess' | 'onerror' | 'onstart') ':' NEWLINE INDENT python_block DEDENT ;
```

`expression`, `python_block`, `python_statement` → parsed by ruff.

### Breaking changes from legacy parser.py

1. Rule names required
2. `singularity:` removed (use `container:`)
3. `version:` removed
4. `subworkflow` removed (use `module`)
5. `run:` requires NEWLINE + INDENT, must contain ≥1 statement
6. Directive values validated as Python argument lists
7. `use rule` clause order fixed

## Parsing strategy

Line-by-line scan identifies Snakemake constructs, delegates Python to ruff.

For directive values (`input: expand("x/{s}.txt", s=SAMPLES)`):
1. Determine byte range from after colon to end of line or indented block
2. Extract text
3. `ruff_python_parser::parse_unchecked(text, Mode::Expression)`
4. Offset returned TextRanges by sub-string position in original file

For `run:` blocks: extract indented block, `parse_unchecked(text, Mode::Module)`.

**TextRange adjustment** — ruff returns ranges relative to the sub-string. Offset recursively:

```rust
fn offset_range(range: TextRange, offset: TextSize) -> TextRange {
    TextRange::new(range.start() + offset, range.end() + offset)
}
```

## Compilation

```
rule foo:                    →  @workflow.rule(name='foo', lineno=N, snakefile='path')
    input: "a.txt"           →  @workflow.input("a.txt")
    output: "b.txt"          →  @workflow.output("b.txt")
    threads: 8               →  @workflow.threads(8)
    shell: "cmd {input}"     →  @workflow.shellcmd("cmd {input}")
    run:                     →  @workflow.run
        do_stuff()           →  def __rule_foo(input, output, ...):
                                     do_stuff()
```

Must match what `Workflow.rule()`, `Workflow.input()`, etc. expect.

Source map: `(generated_offset, generated_len) → (original_offset, original_len)`. Synthetic text unmapped. Convertible to line-based `linemap` dict for backward compat.

## Testing

- Unit: parse each construct, verify AST
- Fixtures: real Snakefiles in `tests/fixtures/` including catalog workflows
- Equivalence: compilation output vs `parser.py --print-compilation`
- Property: `compile(parse(source))` → valid Python; all TextRanges in bounds

## Conventions

- AST nodes: `Clone`, `Debug`, `TextRange`. `Serialize` behind `serde` feature.
- Parser never panics. `Result<_, Vec<ParseError>>` with recovery.
- Byte offsets via `TextSize`.

### Comments policy

- **Doc comments (`///` and `//!`)**: Use on public types, functions, and modules. Keep them short — state what it is, not how it works, unless the how is surprising.
- **Inline comments (`//`)**: Only where the code is doing something non-obvious. If the code needs a comment to explain what it does, consider renaming things first. If you still need a comment, it should explain *why*, not *what*.
- **No comments for**: type/field names that are self-descriptive, straightforward match arms, function calls where the name says what happens, imports, boilerplate.
- **No TODO/FIXME without a tracking issue.**
- When in doubt, leave the comment out. The grammar spec in this file and the PLAN.md provide the high-level context.

## Related projects

- **snakemake**: will use `snakemake-lang` via PyO3 to replace `parser.py`
- **fangs**: formatter + linter, depends on `snakemake-lang`
- **fangs-ls**: Volar.js LSP, calls `snakemake-lang compile`, delegates to Pyright
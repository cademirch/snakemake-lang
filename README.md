# snakemake-lang

Parser, AST, and compiler for the Snakemake workflow language. Rust crate
with optional CLI, JSON output, and PyO3 bindings.

## Features

| Feature            | Enables                                        |
| ------------------ | ---------------------------------------------- |
| `cli`              | `snakemake-lang` binary (implies `serde`)      |
| `serde`            | `Serialize` impls for AST + errors             |
| `python`           | PyO3 bindings                                  |
| `extension-module` | `python` + `pyo3/extension-module` for wheels  |

Default features: none. The library alone has no optional deps.

## Build

```sh
# Library only
cargo build

# CLI binary
cargo build --features cli --bin snakemake-lang

# Release
cargo build --release --features cli --bin snakemake-lang
```

The CLI binary lands at `target/{debug,release}/snakemake-lang`.

## CLI usage

```
snakemake-lang compile <PATH> [--source-map]   # .smk -> virtual Python
snakemake-lang parse   <PATH>                  # .smk -> AST as JSON
snakemake-lang check   <PATHS>...              # parse-only, report errors
```

### Try it on a fixture

```sh
cargo run --features cli --bin snakemake-lang -- \
    compile tests/fixtures/simple_rule.smk
```

Expected output begins with:

```python
@workflow.rule(name='align', lineno=1, snakefile='tests/fixtures/simple_rule.smk')
@workflow.input( "reads/{sample}.fastq"
)
...
```

AST as JSON:

```sh
cargo run --features cli --bin snakemake-lang -- \
    parse tests/fixtures/simple_rule.smk
```

Output (truncated — `Expr`/`Stmt` are debug-formatted strings):

```json
{
  "body": [
    {
      "Rule": {
        "name": "align",
        "directives": [
          {
            "keyword": "Input",
            "value": {
              "Arguments": {
                "positional": [
                  "StringLiteral(ExprStringLiteral { ... value: \"reads/{sample}.fastq\" ... })"
                ],
                "keywords": [],
                "range": { "start": 23, "end": 45 }
              }
            },
            "range": { "start": 12, "end": 45 }
          },
          { "keyword": "Output", "value": { "Arguments": { "positional": ["..."], "keywords": [], "range": { "start": 58, "end": 80 } } }, "range": { "start": 46, "end": 80 } },
          { "keyword": "Threads", "value": { "Arguments": { "positional": ["NumberLiteral(... Int(8) ...)"], "keywords": [], "range": { "start": 94, "end": 95 } } }, "range": { "start": 81, "end": 95 } },
          { "keyword": "Shell", "value": { "Arguments": { "positional": ["..."], "keywords": [], "range": { "start": 107, "end": 148 } } }, "range": { "start": 96, "end": 148 } }
        ],
        "docstring": null,
        "is_checkpoint": false,
        "range": { "start": 0, "end": 148 }
      }
    }
  ],
  "range": { "start": 0, "end": 149 }
}
```

Source map alongside compiled Python:

```sh
cargo run --features cli --bin snakemake-lang -- \
    compile --source-map tests/fixtures/simple_rule.smk
```

## Tests

```sh
cargo test                  # unit + fixture tests
cargo test --features cli   # include CLI-gated paths
```

Fixtures live in `tests/fixtures/`. Equivalence against the legacy
`parser.py --print-compilation` is driven by `tests/compare_parsers.py`.

## Library

```rust
use snakemake_lang::{parse, compile};

let src = std::fs::read_to_string("Snakefile")?;
let ast = parse(&src, "Snakefile")?;
let out = compile(&src, "Snakefile")?;
println!("{}", out.python);
```

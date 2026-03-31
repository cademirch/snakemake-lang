//! Snakemake parser.
//!
//! Hand-written recursive descent parser that extends ruff's Python parser
//! with Snakemake structural keywords.
//!
//! ## How we use ruff's parser
//!
//! ruff's parser API (as of ~0.14.x):
//!
//! ```rust,ignore
//! use ruff_python_parser::{Mode, parse_unchecked};
//!
//! // Parse a complete Python module
//! let parsed = parse_unchecked(source, Mode::Module);
//! let module = parsed.into_syntax().expect("Module");
//! // module.body is Vec<Stmt>
//!
//! // Parse a single expression
//! let parsed = parse_unchecked(expression_text, Mode::Expression);
//! let expr_mod = parsed.into_syntax().expect("Expression");
//! // expr_mod gives you the Expr
//! ```
//!
//! We use Mode::Module to parse `run:` blocks and top-level Python code,
//! and Mode::Expression to parse directive values (the expressions after
//! `input:`, `output:`, etc.).
//!
//! ## Our parsing strategy
//!
//! We do NOT use ruff's tokenizer directly. Instead:
//!
//! 1. We do our own line-by-line scan to identify Snakemake constructs
//!    (rules, directives, global keywords) by looking at line-start tokens.
//!
//! 2. For Python content (expressions in directives, run blocks, top-level
//!    Python between rules), we extract the text span and hand it to
//!    ruff's `parse_unchecked()`.
//!
//! 3. We offset the TextRanges returned by ruff to be relative to the
//!    original file, not the extracted sub-string.
//!
//! This approach avoids fighting ruff's tokenizer/parser with Snakemake
//! keywords it doesn't understand. We handle the Snakemake structure
//! ourselves and delegate Python to ruff.

pub mod directive;
pub mod global;
pub mod handler;
pub mod snakemake;

use crate::ast::Snakefile;
use crate::errors::ParseError;

/// Parse Snakemake source into an AST.
///
/// This is the main entry point. It scans the source line by line,
/// identifies Snakemake constructs at line starts, and delegates
/// Python content to ruff's parser.
pub fn parse(source: &str, path: &str) -> Result<Snakefile, Vec<ParseError>> {
    // TODO: implement in Milestone 1
    //
    // High-level algorithm:
    //
    // 1. Scan source line by line, tracking indentation
    // 2. At each line start, check if the first token is a Snakemake keyword:
    //    - "rule" / "checkpoint" → parse_rule()
    //    - "module" → parse_module()
    //    - "use" → parse_use_rule()
    //    - "onsuccess" / "onerror" / "onstart" → parse_handler()
    //    - global directive keyword → parse_global_directive()
    //    - "ruleorder" → parse_ruleorder()
    //    - "localrules" → parse_localrules()
    //    - "storage" → parse_storage()
    // 3. Otherwise, collect contiguous Python lines and parse with:
    //    ruff_python_parser::parse_unchecked(python_text, Mode::Module)
    // 4. For directive values inside rules/modules:
    //    Extract the value text, parse with ruff as expression(s)
    // 5. For run: blocks:
    //    Extract the indented block text, parse with ruff as Module
    //
    // The key challenge: determining where Snakemake blocks end.
    // We track indentation levels. A rule body starts at the indent
    // after `rule name:`. A directive value starts at the indent after
    // `input:`. Blocks end when indentation returns to or below the
    // block's base level.
    //
    // All TextRanges on returned AST nodes are byte offsets in the
    // original source string.

    todo!("Milestone 1: implement top-level parser")
}

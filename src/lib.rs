//! # snakemake-lang
//!
//! Parser, AST, and compiler for the Snakemake workflow language.
//!
//! Snakemake extends Python with structural keywords for defining
//! data analysis workflows. This crate provides the canonical
//! implementation of the Snakemake language parser.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use snakemake_lang::{parse, compile};
//!
//! let source = r#"
//! rule align:
//!     input: "reads/{sample}.fastq"
//!     output: "aligned/{sample}.bam"
//!     shell: "bwa mem {input} > {output}"
//! "#;
//!
//! // Parse to AST
//! let ast = parse(source, "Snakefile").unwrap();
//!
//! // Compile to virtual Python + source map
//! let result = compile(source, "Snakefile").unwrap();
//! println!("{}", result.python);
//! ```

pub mod ast;
pub mod compile;
pub mod errors;
pub mod parser;

#[cfg(feature = "python")]
mod python;

use errors::ParseError;

/// Parse Snakemake source into an AST.
pub fn parse(source: &str, path: &str) -> Result<ast::Snakefile, Vec<ParseError>> {
    parser::parse(source, path)
}

/// Compile Snakemake source to virtual Python + source map.
pub fn compile(source: &str, path: &str) -> Result<CompileResult, Vec<ParseError>> {
    let ast = parser::parse(source, path)?;
    Ok(compile::generate(source, path, &ast))
}

/// Result of compiling a Snakefile to virtual Python.
pub struct CompileResult {
    /// Valid Python source code that, when exec'd, registers the
    /// workflow's rules and configuration with the Snakemake engine.
    pub python: String,

    /// Source map from generated Python positions to original Snakemake
    /// source positions. Used by the LSP for position mapping.
    pub source_map: compile::SourceMap,
}

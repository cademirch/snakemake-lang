//! Virtual Python compiler.
//!
//! Generates valid Python from a Snakemake AST, producing the same
//! decorator-chain output that parser.py's transpiler generates.
//! Includes a correct source map for position mapping.

pub mod generator;
pub mod source_map;

pub use source_map::SourceMap;

use crate::ast::Snakefile;
use crate::CompileResult;

/// Generate virtual Python + source map from a Snakemake AST.
pub fn generate(source: &str, path: &str, ast: &Snakefile) -> CompileResult {
    let mut codegen = generator::VirtualPythonGenerator::new(source, path);
    codegen.generate(ast);
    let (python, source_map) = codegen.finish();
    CompileResult { python, source_map }
}

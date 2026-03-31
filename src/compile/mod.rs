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
    // TODO: implement in Milestone 4
    // 1. Create VirtualPythonGenerator
    // 2. Walk AST, emit Python for each node
    // 3. Build source map as we go
    // 4. Return CompileResult
    todo!("Milestone 4: implement virtual Python generator")
}

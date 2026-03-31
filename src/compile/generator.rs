//! Virtual Python generator.
//!
//! Walks the Snakemake AST and emits valid Python that, when exec'd,
//! produces the same side effects as parser.py's output (rule registration,
//! global directives, etc.).

use super::source_map::{SourceMap, SourceMapping};
use crate::ast::Snakefile;

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

    /// Generate virtual Python for an entire Snakefile.
    pub fn generate(&mut self, ast: &Snakefile) {
        // TODO: implement in Milestone 4
        // For each statement:
        //   - Snakemake constructs → emit decorator chain
        //   - Python statements → emit verbatim with identity mapping
        todo!("Milestone 4")
    }
}

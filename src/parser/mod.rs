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

// ============================================================
// Line scanner
// ============================================================

/// A single line from a Snakemake source file.
#[derive(Debug, Clone)]
pub(crate) struct Line<'src> {
    /// The raw text of the line, not including the trailing newline.
    pub text: &'src str,
    /// Byte offset of the first character of this line in the source.
    pub start: usize,
    /// Number of leading space characters (indentation level).
    pub indent: usize,
    /// 1-based line number.
    pub number: usize,
}

impl<'src> Line<'src> {
    /// Returns the first whitespace-delimited word on the line, if any.
    pub fn first_word(&self) -> Option<&str> {
        self.trimmed().split_ascii_whitespace().next()
    }

    /// Returns the line text with leading and trailing whitespace removed.
    pub fn trimmed(&self) -> &str {
        self.text.trim()
    }

    /// Returns true if the line is empty or contains only a comment.
    pub fn is_blank_or_comment(&self) -> bool {
        let t = self.trimmed();
        t.is_empty() || t.starts_with('#')
    }
}

/// Splits `source` into a `Vec<Line>`, tracking byte offsets and indentation.
///
/// Splits on `'\n'`, strips `'\r'` for Windows line endings, and removes the
/// trailing empty entry that results from a final `'\n'`.
pub(crate) fn scan_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut byte_offset = 0usize;

    for (number, raw) in source.split('\n').enumerate() {
        let text = raw.strip_suffix('\r').unwrap_or(raw);
        let start = byte_offset;

        // Count leading spaces for indentation.
        let indent = text.len() - text.trim_start_matches(' ').len();

        lines.push(Line {
            text,
            start,
            indent,
            number: number + 1,
        });

        // Advance past this line plus the '\n' separator.
        byte_offset += raw.len() + 1;
    }

    // Remove the trailing empty line produced by a final '\n'.
    if lines.last().map_or(false, |l| l.text.is_empty()) {
        lines.pop();
    }

    lines
}

// ============================================================
// Public entry point (stub — will be replaced in Task 3)
// ============================================================

/// Parse Snakemake source into an AST.
///
/// This is the main entry point. It scans the source line by line,
/// identifies Snakemake constructs at line starts, and delegates
/// Python content to ruff's parser.
pub fn parse(source: &str, path: &str) -> Result<Snakefile, Vec<ParseError>> {
    // TODO: implement in Task 3
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

    todo!("Task 3: implement top-level parser dispatch")
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_splits_lines() {
        let source = "line one\nline two\nline three\n";
        let lines = scan_lines(source);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "line one");
        assert_eq!(lines[1].text, "line two");
        assert_eq!(lines[2].text, "line three");
    }

    #[test]
    fn scanner_tracks_byte_offsets() {
        let source = "abc\ndef\nghi\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].start, 0);
        assert_eq!(lines[1].start, 4);  // "abc\n" = 4 bytes
        assert_eq!(lines[2].start, 8);  // "def\n"  = 4 bytes
    }

    #[test]
    fn scanner_measures_indentation() {
        let source = "top\n    indented\n        double\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].indent, 0);
        assert_eq!(lines[1].indent, 4);
        assert_eq!(lines[2].indent, 8);
    }

    #[test]
    fn scanner_handles_blank_lines() {
        let source = "a\n\nb\n";
        let lines = scan_lines(source);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].text, "");
        assert!(lines[1].is_blank_or_comment());
    }

    #[test]
    fn scanner_first_word_extraction() {
        let source = "rule foo:\n    input: 'x'\n# comment\n\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].first_word(), Some("rule"));
        assert_eq!(lines[1].first_word(), Some("input:"));
        assert_eq!(lines[2].first_word(), Some("#"));
        assert_eq!(lines[3].first_word(), None);
    }

    #[test]
    fn scanner_line_numbers_are_one_based() {
        let source = "a\nb\nc\n";
        let lines = scan_lines(source);
        assert_eq!(lines[0].number, 1);
        assert_eq!(lines[1].number, 2);
        assert_eq!(lines[2].number, 3);
    }

    #[test]
    fn scanner_strips_carriage_return() {
        let source = "line one\r\nline two\r\n";
        let lines = scan_lines(source);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line one");
        assert_eq!(lines[1].text, "line two");
    }

    #[test]
    fn scanner_empty_source() {
        let lines = scan_lines("");
        assert!(lines.is_empty());
    }

    #[test]
    fn scanner_no_trailing_newline() {
        let source = "line one\nline two";
        let lines = scan_lines(source);
        // No trailing empty entry when there's no trailing '\n'.
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line one");
        assert_eq!(lines[1].text, "line two");
    }
}

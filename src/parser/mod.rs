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

use ruff_python_ast::Mod;
use ruff_python_parser::{Mode, parse_unchecked};
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{GlobalKeyword, Snakefile, Statement};
use crate::errors::{ParseError, ParseErrorKind};

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
// Parser struct
// ============================================================

/// Top-level parser for a Snakemake file.
pub(crate) struct Parser<'src> {
    pub(crate) source: &'src str,
    pub(crate) path: &'src str,
    pub(crate) lines: Vec<Line<'src>>,
    pub(crate) cursor: usize,
    pub(crate) errors: Vec<ParseError>,
}

impl<'src> Parser<'src> {
    /// Creates a new parser for the given source text and file path.
    pub(crate) fn new(source: &'src str, path: &'src str) -> Self {
        let lines = scan_lines(source);
        Self {
            source,
            path,
            lines,
            cursor: 0,
            errors: Vec::new(),
        }
    }

    /// Returns the current line without advancing the cursor.
    pub(crate) fn current(&self) -> Option<&Line<'src>> {
        self.lines.get(self.cursor)
    }

    /// Advances the cursor past the current line and returns it.
    pub(crate) fn advance(&mut self) -> Option<&Line<'src>> {
        let line = self.lines.get(self.cursor);
        self.cursor += 1;
        line
    }

    /// Returns true when all lines have been consumed.
    pub(crate) fn at_end(&self) -> bool {
        self.cursor >= self.lines.len()
    }

    // ----------------------------------------------------------
    // Top-level dispatch
    // ----------------------------------------------------------

    /// Parses the whole file and returns the top-level `Snakefile` AST node.
    pub(crate) fn parse_file(&mut self) -> Snakefile {
        let mut body = Vec::new();

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            let word = match line.first_word() {
                Some(w) => w,
                None => {
                    self.advance();
                    continue;
                }
            };
            let indent = line.indent;

            // rule/checkpoint match at any indentation level.
            if word == "rule" || word == "checkpoint" {
                let is_checkpoint = word == "checkpoint";
                body.push(self.parse_rule(is_checkpoint));
                continue;
            }

            // All other Snakemake keywords are only recognized at column 0.
            if indent == 0 {
                match word {
                    "module" => {
                        body.push(self.parse_module());
                        continue;
                    }
                    "use" => {
                        body.push(self.parse_use_rule());
                        continue;
                    }
                    "onsuccess" | "onerror" | "onstart" => {
                        body.push(self.parse_handler());
                        continue;
                    }
                    "ruleorder" => {
                        body.push(self.parse_ruleorder());
                        continue;
                    }
                    "localrules" => {
                        body.push(self.parse_localrules());
                        continue;
                    }
                    "storage" => {
                        body.push(self.parse_storage());
                        continue;
                    }
                    kw if GlobalKeyword::from_str(kw).is_some() => {
                        body.push(self.parse_global_directive());
                        continue;
                    }
                    _ => {}
                }
            }

            // Everything else is Python.
            body.extend(self.collect_python());
        }

        let range = if self.source.is_empty() {
            TextRange::default()
        } else {
            TextRange::new(TextSize::new(0), TextSize::new(self.source.len() as u32))
        };

        Snakefile { body, range }
    }

    /// Collects contiguous non-Snakemake lines and parses them as Python.
    ///
    /// Breaks when it encounters `rule`/`checkpoint` at any indent level, or
    /// another Snakemake keyword at indent == 0.
    pub(crate) fn collect_python(&mut self) -> Vec<Statement> {
        let start_cursor = self.cursor;
        let mut end_byte = 0usize;
        let mut start_byte: Option<usize> = None;

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            if line.is_blank_or_comment() {
                // Blank/comment lines belong to the surrounding Python block.
                end_byte = line.start + line.text.len() + 1; // include the '\n'
                if start_byte.is_none() {
                    start_byte = Some(line.start);
                }
                self.advance();
                continue;
            }

            let word = line.first_word().unwrap_or("");
            let indent = line.indent;

            // rule/checkpoint break at any indent.
            if word == "rule" || word == "checkpoint" {
                break;
            }

            // Other Snakemake keywords break only at column 0.
            if indent == 0 && is_top_level_keyword(word) {
                break;
            }

            if start_byte.is_none() {
                start_byte = Some(line.start);
            }
            end_byte = line.start + line.text.len() + 1;
            self.advance();
        }

        let start_byte = match start_byte {
            Some(b) => b,
            None => return Vec::new(),
        };

        // Clamp end_byte to source length in case the last line has no trailing '\n'.
        let end_byte = end_byte.min(self.source.len());
        let python_text = &self.source[start_byte..end_byte];

        if python_text.trim().is_empty() {
            return Vec::new();
        }

        let parsed = parse_unchecked(python_text, Mode::Module.into());
        let offset = TextSize::new(start_byte as u32);

        // Collect any ruff syntax errors, offsetting their ranges.
        for err in parsed.errors() {
            self.errors.push(ParseError {
                message: err.to_string(),
                range: offset_range(err.location, offset),
                kind: ParseErrorKind::PythonSyntaxError,
                line: self.lines.get(start_cursor).map_or(1, |l| l.number),
                column: 0,
                source_line: None,
            });
        }

        let module = match parsed.into_syntax() {
            Mod::Module(m) => m,
            Mod::Expression(_) => return Vec::new(),
        };

        module
            .body
            .into_iter()
            .map(|stmt| Statement::Python(offset_stmt(stmt, offset)))
            .collect()
    }

    // ----------------------------------------------------------
    // Stubs — implemented in later milestones
    // ----------------------------------------------------------

    pub(crate) fn parse_rule(&mut self, _is_checkpoint: bool) -> Statement {
        todo!("parse_rule: implement in Milestone 2")
    }

    pub(crate) fn parse_module(&mut self) -> Statement {
        todo!("parse_module: implement in Milestone 3")
    }

    pub(crate) fn parse_use_rule(&mut self) -> Statement {
        todo!("parse_use_rule: implement in Milestone 4")
    }

    pub(crate) fn parse_handler(&mut self) -> Statement {
        todo!("parse_handler: implement in Milestone 5")
    }

    pub(crate) fn parse_ruleorder(&mut self) -> Statement {
        todo!("parse_ruleorder: implement in Milestone 6")
    }

    pub(crate) fn parse_localrules(&mut self) -> Statement {
        todo!("parse_localrules: implement in Milestone 6")
    }

    pub(crate) fn parse_storage(&mut self) -> Statement {
        todo!("parse_storage: implement in Milestone 6")
    }

    pub(crate) fn parse_global_directive(&mut self) -> Statement {
        todo!("parse_global_directive: implement in Milestone 6")
    }
}

// ============================================================
// Helpers
// ============================================================

/// Returns true if `word` is a Snakemake keyword that is only recognized at
/// column 0 (i.e., not `rule`/`checkpoint`, which are handled separately).
fn is_top_level_keyword(word: &str) -> bool {
    matches!(
        word,
        "module"
            | "use"
            | "onsuccess"
            | "onerror"
            | "onstart"
            | "ruleorder"
            | "localrules"
            | "storage"
    ) || GlobalKeyword::from_str(word).is_some()
}

/// Offsets a `TextRange` by `offset` bytes.
pub(crate) fn offset_range(range: TextRange, offset: TextSize) -> TextRange {
    TextRange::new(range.start() + offset, range.end() + offset)
}

/// Returns `stmt` as-is; range offsetting is deferred to compiler infrastructure.
///
/// ruff returns `TextRange` values relative to the sub-string we hand it.
/// Full recursive offsetting will be wired up once the source map is in place.
fn offset_stmt(stmt: ruff_python_ast::Stmt, offset: TextSize) -> ruff_python_ast::Stmt {
    // TODO: apply offset recursively once source map infrastructure is ready.
    let _ = offset;
    stmt
}

// ============================================================
// Public entry point
// ============================================================

/// Parse Snakemake source into an AST.
///
/// This is the main entry point. It scans the source line by line,
/// identifies Snakemake constructs at line starts, and delegates
/// Python content to ruff's parser.
pub fn parse(source: &str, path: &str) -> Result<Snakefile, Vec<ParseError>> {
    let mut parser = Parser::new(source, path);
    let snakefile = parser.parse_file();
    if parser.errors.is_empty() {
        Ok(snakefile)
    } else {
        Err(parser.errors)
    }
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

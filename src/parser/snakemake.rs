//! Rule and checkpoint parsing.

use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{SnakemakeDirective, SnakemakeRule, Statement};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;

impl<'src> Parser<'src> {
    /// Parse a `rule` or `checkpoint` definition.
    ///
    /// Expects the cursor to be on the `rule NAME:` or `checkpoint NAME:` line.
    pub(crate) fn parse_rule(&mut self, is_checkpoint: bool) -> Statement {
        let header_line = &self.lines[self.cursor];
        let rule_start = header_line.start;
        let rule_indent = header_line.indent;
        let line_number = header_line.number;

        // Extract name from header: "rule NAME:" or "checkpoint NAME:"
        let trimmed = header_line.trimmed();
        let keyword = if is_checkpoint { "checkpoint" } else { "rule" };
        let after_keyword = trimmed[keyword.len()..].trim();

        // Strip trailing comments before extracting the name.
        let after_keyword = super::strip_inline_comment(after_keyword);

        let name_str = after_keyword
            .strip_suffix(':')
            .unwrap_or(after_keyword)
            .trim();

        if name_str.is_empty() {
            self.errors.push(ParseError {
                message: format!("{keyword} name is required"),
                range: TextRange::new(
                    TextSize::new(rule_start as u32),
                    TextSize::new((rule_start + trimmed.len()) as u32),
                ),
                kind: ParseErrorKind::MissingRuleName,
                line: line_number,
                column: 0,
                source_line: Some(header_line.text.to_string()),
            });
            // Use an empty name but continue parsing
            let name = Identifier::new("", TextRange::default());
            self.advance();
            return Statement::Rule(SnakemakeRule {
                name,
                directives: Vec::new(),
                docstring: None,
                is_checkpoint,
                range: TextRange::new(
                    TextSize::new(rule_start as u32),
                    TextSize::new(rule_start as u32),
                ),
            });
        }

        // Compute name range within the original source
        let name_offset_in_line = header_line.text.find(name_str).unwrap_or(0);
        let name_start = rule_start + name_offset_in_line;
        let name_range = TextRange::new(
            TextSize::new(name_start as u32),
            TextSize::new((name_start + name_str.len()) as u32),
        );
        let name = Identifier::new(name_str, name_range);

        // Advance past header line
        self.advance();

        // Determine expected body indentation from the first non-blank body line
        let body_indent = self.peek_body_indent(rule_indent);

        // Parse body directives
        let mut directives: Vec<SnakemakeDirective> = Vec::new();

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            // Skip blank/comment lines
            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            // Stop when indentation drops to or below rule level
            if line.indent <= rule_indent {
                break;
            }

            // Try to parse a directive
            if let Some(directive) = self.try_parse_directive(body_indent) {
                directives.push(directive);
            } else {
                // Unrecognized line in body
                let bad_line = &self.lines[self.cursor];
                self.errors.push(ParseError {
                    message: format!("unknown directive: {}", bad_line.trimmed()),
                    range: TextRange::new(
                        TextSize::new(bad_line.start as u32),
                        TextSize::new((bad_line.start + bad_line.text.len()) as u32),
                    ),
                    kind: ParseErrorKind::UnknownDirective,
                    line: bad_line.number,
                    column: bad_line.indent,
                    source_line: Some(bad_line.text.to_string()),
                });
                self.advance();
            }
        }

        // Compute rule end from the last directive or the header line
        let rule_end = if let Some(last) = directives.last() {
            last.range.end().to_u32() as usize
        } else {
            // Just the header line
            rule_start
                + self
                    .lines
                    .get(self.cursor.saturating_sub(1))
                    .map_or(0, |l| l.start + l.text.len() - rule_start)
        };

        let range = TextRange::new(
            TextSize::new(rule_start as u32),
            TextSize::new(rule_end as u32),
        );

        Statement::Rule(SnakemakeRule {
            name,
            directives,
            docstring: None,
            is_checkpoint,
            range,
        })
    }

    /// Peek ahead to find the indentation level of the first non-blank body line.
    pub(crate) fn peek_body_indent(&self, parent_indent: usize) -> usize {
        for i in self.cursor..self.lines.len() {
            let line = &self.lines[i];
            if !line.is_blank_or_comment() {
                if line.indent > parent_indent {
                    return line.indent;
                }
                break;
            }
        }
        // Default: parent + 4
        parent_indent + 4
    }
}

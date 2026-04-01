//! Global directive, ruleorder, localrules, and storage parsing.

use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{
    DirectiveValue, GlobalKeyword, SnakemakeGlobalDirective, SnakemakeLocalrules,
    SnakemakeRuleorder, SnakemakeStorage, Statement,
};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;

impl<'src> Parser<'src> {
    /// Parse a global directive (`configfile:`, `include:`, `workdir:`, etc.).
    pub(crate) fn parse_global_directive(&mut self) -> Statement {
        let line = &self.lines[self.cursor];
        let directive_start = line.start;
        let trimmed = line.trimmed();

        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let keyword_str = trimmed[..colon_pos].trim();
        let keyword = GlobalKeyword::from_str(keyword_str).unwrap();

        let after_colon = trimmed[colon_pos + 1..].trim();

        if after_colon.is_empty() {
            // Block form
            self.advance();
            let value = self.parse_block_directive_value(0, directive_start);
            let value_end = match &value {
                DirectiveValue::Arguments(args) => args.range.end().to_u32() as usize,
                DirectiveValue::Block(_) => directive_start,
            };
            let range = TextRange::new(
                TextSize::new(directive_start as u32),
                TextSize::new(value_end as u32),
            );
            Statement::GlobalDirective(SnakemakeGlobalDirective {
                keyword,
                value,
                range,
            })
        } else {
            // Inline form
            let after_colon_owned = after_colon.to_string();
            let original_line_text = &self.source[directive_start..];
            let colon_offset_in_source = original_line_text.find(':').unwrap_or(0);
            let after_colon_in_source = &original_line_text[colon_offset_in_source + 1..];
            let leading_ws = after_colon_in_source.len() - after_colon_in_source.trim_start().len();
            let value_offset = directive_start + colon_offset_in_source + 1 + leading_ws;

            let value_text = self.collect_inline_value(&after_colon_owned);
            let args = self.parse_arguments(&value_text, value_offset);
            let args_end = if args.range == TextRange::default() {
                value_offset + value_text.len()
            } else {
                args.range.end().to_u32() as usize
            };
            let range = TextRange::new(
                TextSize::new(directive_start as u32),
                TextSize::new(args_end as u32),
            );
            Statement::GlobalDirective(SnakemakeGlobalDirective {
                keyword,
                value: DirectiveValue::Arguments(args),
                range,
            })
        }
    }

    /// Parse `ruleorder: a > b > c`.
    pub(crate) fn parse_ruleorder(&mut self) -> Statement {
        let line = &self.lines[self.cursor];
        let stmt_start = line.start;
        let trimmed = line.trimmed();

        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let after_colon = trimmed[colon_pos + 1..].trim();

        let names: Vec<Identifier> = after_colon
            .split('>')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|name| Identifier::new(name, TextRange::default()))
            .collect();

        if names.is_empty() {
            self.errors.push(ParseError {
                message: "ruleorder requires at least one rule name".to_string(),
                range: TextRange::new(
                    TextSize::new(stmt_start as u32),
                    TextSize::new((stmt_start + trimmed.len()) as u32),
                ),
                kind: ParseErrorKind::ExpectedToken,
                line: line.number,
                column: 0,
                source_line: Some(line.text.to_string()),
            });
        }

        let stmt_end = stmt_start + line.text.len();
        self.advance();

        Statement::Ruleorder(SnakemakeRuleorder {
            names,
            range: TextRange::new(
                TextSize::new(stmt_start as u32),
                TextSize::new(stmt_end as u32),
            ),
        })
    }

    /// Parse `localrules: a, b, c`.
    pub(crate) fn parse_localrules(&mut self) -> Statement {
        let line = &self.lines[self.cursor];
        let stmt_start = line.start;
        let trimmed = line.trimmed();

        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let after_colon = trimmed[colon_pos + 1..].trim();

        let names: Vec<Identifier> = after_colon
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|name| Identifier::new(name, TextRange::default()))
            .collect();

        if names.is_empty() {
            self.errors.push(ParseError {
                message: "localrules requires at least one rule name".to_string(),
                range: TextRange::new(
                    TextSize::new(stmt_start as u32),
                    TextSize::new((stmt_start + trimmed.len()) as u32),
                ),
                kind: ParseErrorKind::ExpectedToken,
                line: line.number,
                column: 0,
                source_line: Some(line.text.to_string()),
            });
        }

        let stmt_end = stmt_start + line.text.len();
        self.advance();

        Statement::Localrules(SnakemakeLocalrules {
            names,
            range: TextRange::new(
                TextSize::new(stmt_start as u32),
                TextSize::new(stmt_end as u32),
            ),
        })
    }

    /// Parse `storage TAG: value`.
    pub(crate) fn parse_storage(&mut self) -> Statement {
        let line = &self.lines[self.cursor];
        let stmt_start = line.start;
        let line_number = line.number;
        let trimmed = line.trimmed();

        // "storage TAG: value" or "storage TAG:\n  block"
        let after_storage = trimmed["storage".len()..].trim();
        let colon_pos = match after_storage.find(':') {
            Some(p) => p,
            None => {
                self.errors.push(ParseError {
                    message: "expected ':' in storage declaration".to_string(),
                    range: TextRange::new(
                        TextSize::new(stmt_start as u32),
                        TextSize::new((stmt_start + trimmed.len()) as u32),
                    ),
                    kind: ParseErrorKind::ExpectedToken,
                    line: line_number,
                    column: 0,
                    source_line: Some(line.text.to_string()),
                });
                self.advance();
                return Statement::Storage(SnakemakeStorage {
                    tag: Identifier::new("", TextRange::default()),
                    value: DirectiveValue::Arguments(crate::ast::DirectiveArguments {
                        positional: Vec::new(),
                        keywords: Vec::new(),
                        range: TextRange::default(),
                    }),
                    range: TextRange::new(
                        TextSize::new(stmt_start as u32),
                        TextSize::new(stmt_start as u32),
                    ),
                });
            }
        };

        let tag_str = after_storage[..colon_pos].trim();
        let tag = Identifier::new(tag_str, TextRange::default());

        let after_colon = after_storage[colon_pos + 1..].trim();

        if after_colon.is_empty() {
            // Block form
            self.advance();
            let value = self.parse_block_directive_value(0, stmt_start);
            let value_end = match &value {
                DirectiveValue::Arguments(args) => args.range.end().to_u32() as usize,
                DirectiveValue::Block(_) => stmt_start,
            };
            let range = TextRange::new(
                TextSize::new(stmt_start as u32),
                TextSize::new(value_end as u32),
            );
            Statement::Storage(SnakemakeStorage { tag, value, range })
        } else {
            // Inline form
            let after_colon_owned = after_colon.to_string();
            let original_line_text = &self.source[stmt_start..];
            let full_colon_offset = original_line_text.find(':').unwrap_or(0);
            let after_colon_in_source = &original_line_text[full_colon_offset + 1..];
            let leading_ws = after_colon_in_source.len() - after_colon_in_source.trim_start().len();
            let value_offset = stmt_start + full_colon_offset + 1 + leading_ws;

            let value_text = self.collect_inline_value(&after_colon_owned);
            let args = self.parse_arguments(&value_text, value_offset);
            let args_end = if args.range == TextRange::default() {
                value_offset + value_text.len()
            } else {
                args.range.end().to_u32() as usize
            };
            let range = TextRange::new(
                TextSize::new(stmt_start as u32),
                TextSize::new(args_end as u32),
            );
            Statement::Storage(SnakemakeStorage {
                tag,
                value: DirectiveValue::Arguments(args),
                range,
            })
        }
    }
}

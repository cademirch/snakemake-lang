//! Module parsing.

use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{DirectiveValue, ModuleDirective, ModuleKeyword, SnakemakeModule, Statement};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;

impl<'src> Parser<'src> {
    /// Parse a `module NAME:` block with module-specific directives.
    pub(crate) fn parse_module(&mut self) -> Statement {
        let header_line = &self.lines[self.cursor];
        let module_start = header_line.start;
        let module_indent = header_line.indent;
        let line_number = header_line.number;

        let trimmed = header_line.trimmed();
        let after_keyword = trimmed["module".len()..].trim();
        let name_str = after_keyword
            .strip_suffix(':')
            .unwrap_or(after_keyword)
            .trim();

        if name_str.is_empty() {
            self.errors.push(ParseError {
                message: "module name is required".to_string(),
                range: TextRange::new(
                    TextSize::new(module_start as u32),
                    TextSize::new((module_start + trimmed.len()) as u32),
                ),
                kind: ParseErrorKind::ExpectedToken,
                line: line_number,
                column: 0,
                source_line: Some(header_line.text.to_string()),
            });
            self.advance();
            return Statement::Module(SnakemakeModule {
                name: Identifier::new("", TextRange::default()),
                directives: Vec::new(),
                docstring: None,
                range: TextRange::new(
                    TextSize::new(module_start as u32),
                    TextSize::new(module_start as u32),
                ),
            });
        }

        let name_offset_in_line = header_line.text.find(name_str).unwrap_or(0);
        let name_start = module_start + name_offset_in_line;
        let name_range = TextRange::new(
            TextSize::new(name_start as u32),
            TextSize::new((name_start + name_str.len()) as u32),
        );
        let name = Identifier::new(name_str, name_range);

        self.advance();

        let body_indent = self.peek_body_indent(module_indent);
        let mut directives: Vec<ModuleDirective> = Vec::new();

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            if line.is_blank_or_comment() {
                self.advance();
                continue;
            }

            if line.indent <= module_indent {
                break;
            }

            if let Some(directive) = self.try_parse_module_directive(body_indent) {
                directives.push(directive);
            } else {
                let bad_line = &self.lines[self.cursor];
                self.errors.push(ParseError {
                    message: format!("unknown module directive: {}", bad_line.trimmed()),
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

        let module_end = if let Some(last) = directives.last() {
            last.range.end().to_u32() as usize
        } else {
            module_start
                + self
                    .lines
                    .get(self.cursor.saturating_sub(1))
                    .map_or(0, |l| l.start + l.text.len() - module_start)
        };

        let range = TextRange::new(
            TextSize::new(module_start as u32),
            TextSize::new(module_end as u32),
        );

        Statement::Module(SnakemakeModule {
            name,
            directives,
            docstring: None,
            range,
        })
    }

    /// Try to parse a module directive from the current line.
    fn try_parse_module_directive(&mut self, body_indent: usize) -> Option<ModuleDirective> {
        let line = self.current()?;
        let trimmed = line.trimmed();

        let colon_pos = trimmed.find(':')?;
        let keyword_str = trimmed[..colon_pos].trim();
        let keyword = ModuleKeyword::from_str(keyword_str)?;

        let directive_start = line.start;
        let after_colon = trimmed[colon_pos + 1..].trim();

        if after_colon.is_empty() {
            self.advance();
            let value = self.parse_block_directive_value(body_indent, directive_start);
            let value_end = match &value {
                DirectiveValue::Arguments(args) => args.range.end().to_u32() as usize,
                DirectiveValue::Block(_) => directive_start,
            };
            let range = TextRange::new(
                TextSize::new(directive_start as u32),
                TextSize::new(value_end as u32),
            );
            Some(ModuleDirective {
                keyword,
                value,
                range,
            })
        } else {
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
            Some(ModuleDirective {
                keyword,
                value: DirectiveValue::Arguments(args),
                range,
            })
        }
    }
}

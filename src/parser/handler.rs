//! Handler parsing (onsuccess, onerror, onstart).

use ruff_python_ast::Mod;
use ruff_python_parser::{Mode, parse_unchecked};
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{HandlerKind, SnakemakeHandler, Statement};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;
use super::directive::dedent_block;

impl<'src> Parser<'src> {
    /// Parse an `onsuccess:`, `onerror:`, or `onstart:` handler.
    pub(crate) fn parse_handler(&mut self) -> Statement {
        let header_line = &self.lines[self.cursor];
        let handler_start = header_line.start;
        let trimmed = header_line.trimmed();

        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let keyword_str = trimmed[..colon_pos].trim();
        let kind = HandlerKind::from_str(keyword_str).unwrap();

        self.advance();

        // Collect indented block lines
        let mut block_lines: Vec<&str> = Vec::new();
        let mut block_end = handler_start;

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            if line.is_blank_or_comment() {
                if line.trimmed().is_empty() {
                    block_lines.push(line.text);
                    self.advance();
                    continue;
                }
                // Comment: include if indented
                if line.indent > 0 {
                    block_lines.push(line.text);
                    block_end = line.start + line.text.len();
                    self.advance();
                    continue;
                }
                break;
            }

            if line.indent == 0 {
                break;
            }

            block_lines.push(line.text);
            block_end = line.start + line.text.len();
            self.advance();
        }

        // Determine minimum indentation of non-blank lines
        let block_indent = block_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .min()
            .unwrap_or(4);

        let block_text = block_lines.join("\n");
        let dedented = dedent_block(&block_text, block_indent);
        let dedented = format!("{dedented}\n");

        let parsed = parse_unchecked(&dedented, Mode::Module.into());

        for err in parsed.errors() {
            self.errors.push(ParseError {
                message: err.to_string(),
                range: TextRange::new(
                    TextSize::new(handler_start as u32),
                    TextSize::new(block_end as u32),
                ),
                kind: ParseErrorKind::PythonSyntaxError,
                line: 0,
                column: 0,
                source_line: None,
            });
        }

        let stmts = match parsed.into_syntax() {
            Mod::Module(m) => m.body,
            Mod::Expression(_) => Vec::new(),
        };

        let range = TextRange::new(
            TextSize::new(handler_start as u32),
            TextSize::new(block_end as u32),
        );

        Statement::Handler(SnakemakeHandler {
            kind,
            body: stmts,
            range,
        })
    }
}

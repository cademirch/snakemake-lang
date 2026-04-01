//! Directive value parsing.
//!
//! Parses directive values (the expressions after `input:`, `output:`, etc.)
//! by wrapping them in a synthetic function call and delegating to ruff's
//! expression parser.

use ruff_python_ast::{Expr, Mod};
use ruff_python_parser::{Mode, parse_unchecked};
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{
    DirectiveArguments, DirectiveKeyword, DirectiveKeywordArgument, DirectiveValue,
    SnakemakeDirective,
};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;

impl<'src> Parser<'src> {
    /// Try to parse a directive from the current line.
    ///
    /// Returns `None` if the current line doesn't start with a recognized
    /// directive keyword.
    pub(crate) fn try_parse_directive(
        &mut self,
        body_indent: usize,
    ) -> Option<SnakemakeDirective> {
        let line = self.current()?;
        let trimmed = line.trimmed();

        // Extract the keyword portion (everything before the colon)
        let colon_pos = trimmed.find(':')?;
        let keyword_str = trimmed[..colon_pos].trim();

        let keyword = DirectiveKeyword::from_str(keyword_str)?;

        let directive_start = line.start;

        if keyword == DirectiveKeyword::Run {
            return Some(self.parse_run_directive(directive_start, body_indent));
        }

        let after_colon = trimmed[colon_pos + 1..].trim();

        if after_colon.is_empty() {
            // Block form: value is on subsequent indented lines
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
            Some(SnakemakeDirective {
                keyword,
                value,
                range,
            })
        } else {
            // Own the after_colon text before taking &mut self
            let after_colon_owned = after_colon.to_string();

            // Compute the value offset from the original source before advancing
            let original_line_text = &self.source[directive_start..];
            let colon_offset_in_source = original_line_text.find(':').unwrap_or(0);
            let after_colon_in_source = &original_line_text[colon_offset_in_source + 1..];
            let leading_ws = after_colon_in_source.len()
                - after_colon_in_source.trim_start().len();
            let value_offset = directive_start + colon_offset_in_source + 1 + leading_ws;

            // Inline form: value is on the same line (possibly with continuation)
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
            Some(SnakemakeDirective {
                keyword,
                value: DirectiveValue::Arguments(args),
                range,
            })
        }
    }

    /// Collect the inline value text, handling parenthesized continuations.
    ///
    /// Starts with `first_part` (the text after the colon on the directive
    /// line), advances past the current line, and keeps reading continuation
    /// lines while delimiter depth > 0.
    fn collect_inline_value(&mut self, first_part: &str) -> String {
        let mut text = first_part.to_string();
        let depth = count_open_delimiters(&text);

        self.advance();

        if depth > 0 {
            let mut current_depth = depth;
            while !self.at_end() && current_depth > 0 {
                let line = match self.current() {
                    Some(l) => l,
                    None => break,
                };
                text.push('\n');
                text.push_str(line.text);
                current_depth += count_open_delimiters(line.text);
                self.advance();
            }
        }

        text
    }

    /// Parse a block-form directive value (value on indented lines below the keyword).
    fn parse_block_directive_value(
        &mut self,
        parent_indent: usize,
        directive_start: usize,
    ) -> DirectiveValue {
        let mut block_lines: Vec<&str> = Vec::new();
        let mut block_start: Option<usize> = None;

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            // Blank lines inside the block are included
            if line.is_blank_or_comment() && !line.trimmed().is_empty() {
                // Comment line - check if it's indented enough
                if line.indent > parent_indent {
                    if block_start.is_none() {
                        block_start = Some(line.start);
                    }
                    block_lines.push(line.text);
                    self.advance();
                    continue;
                } else {
                    break;
                }
            }

            if line.is_blank_or_comment() {
                // Empty line - include it if we're in a block
                if block_start.is_some() {
                    block_lines.push(line.text);
                    self.advance();
                    continue;
                } else {
                    self.advance();
                    continue;
                }
            }

            // Stop if indentation is at or below the parent
            if line.indent <= parent_indent {
                break;
            }

            if block_start.is_none() {
                block_start = Some(line.start);
            }
            block_lines.push(line.text);
            self.advance();
        }

        let block_start = block_start.unwrap_or(directive_start);

        if block_lines.is_empty() {
            return DirectiveValue::Arguments(DirectiveArguments {
                positional: Vec::new(),
                keywords: Vec::new(),
                range: TextRange::new(
                    TextSize::new(directive_start as u32),
                    TextSize::new(directive_start as u32),
                ),
            });
        }

        // Dedent the block
        let dedented = dedent_block(&block_lines.join("\n"), parent_indent);
        let args = self.parse_arguments(&dedented, block_start);
        DirectiveValue::Arguments(args)
    }

    /// Parse a comma-separated argument list by wrapping it in a synthetic
    /// function call and extracting the arguments from ruff's Call node.
    pub(crate) fn parse_arguments(
        &mut self,
        text: &str,
        original_offset: usize,
    ) -> DirectiveArguments {
        let wrapper = format!("__f({text})");
        let parsed = parse_unchecked(&wrapper, Mode::Expression.into());

        for err in parsed.errors() {
            self.errors.push(ParseError {
                message: err.to_string(),
                range: TextRange::new(
                    TextSize::new(original_offset as u32),
                    TextSize::new((original_offset + text.len()) as u32),
                ),
                kind: ParseErrorKind::PythonSyntaxError,
                line: 0,
                column: 0,
                source_line: None,
            });
        }

        let syntax = parsed.into_syntax();
        let expr = match syntax {
            Mod::Expression(mod_expr) => *mod_expr.body,
            Mod::Module(_) => {
                return DirectiveArguments {
                    positional: Vec::new(),
                    keywords: Vec::new(),
                    range: TextRange::default(),
                };
            }
        };

        // The wrapper parses as a Call expression: __f(args...)
        let call = match expr {
            Expr::Call(call) => call,
            _ => {
                return DirectiveArguments {
                    positional: Vec::new(),
                    keywords: Vec::new(),
                    range: TextRange::default(),
                };
            }
        };

        // "__f(" is 4 characters; the arguments start at offset 4 in the wrapper.
        // We need to map back to original_offset in the source file.
        // The wrapper prefix "__f(" shifts everything by 4 bytes.
        // So the real offset adjustment is: original_offset - 4
        // (ruff reports positions relative to wrapper start)
        let _wrapper_prefix_len = 4u32; // "__f("

        let positional: Vec<Expr> = call.arguments.args.into_vec();
        let keywords: Vec<DirectiveKeywordArgument> = call
            .arguments
            .keywords
            .into_vec()
            .into_iter()
            .filter_map(|kw| {
                let name = kw.arg?;
                Some(DirectiveKeywordArgument {
                    range: kw.range,
                    name,
                    value: kw.value,
                })
            })
            .collect();

        let range = TextRange::new(
            TextSize::new(original_offset as u32),
            TextSize::new((original_offset + text.len()) as u32),
        );

        DirectiveArguments {
            positional,
            keywords,
            range,
        }
    }

    /// Parse a `run:` directive's Python block.
    fn parse_run_directive(
        &mut self,
        start_offset: usize,
        body_indent: usize,
    ) -> SnakemakeDirective {
        // Advance past the `run:` line
        self.advance();

        let mut block_lines: Vec<&str> = Vec::new();
        let mut block_end = start_offset;

        while !self.at_end() {
            let line = match self.current() {
                Some(l) => l,
                None => break,
            };

            // Blank lines inside the block are included
            if line.is_blank_or_comment() {
                if line.trimmed().is_empty() {
                    // Empty line
                    block_lines.push(line.text);
                    self.advance();
                    continue;
                }
                // Comment line
                if line.indent > body_indent - 1 {
                    block_lines.push(line.text);
                    block_end = line.start + line.text.len();
                    self.advance();
                    continue;
                }
                break;
            }

            // Stop when indentation drops to body level or below
            if line.indent < body_indent {
                break;
            }

            // Also stop if this is another directive at body_indent level
            if line.indent == body_indent {
                let trimmed = line.trimmed();
                if let Some(colon_pos) = trimmed.find(':') {
                    let kw = trimmed[..colon_pos].trim();
                    if DirectiveKeyword::from_str(kw).is_some() {
                        break;
                    }
                }
            }

            block_lines.push(line.text);
            block_end = line.start + line.text.len();
            self.advance();
        }

        // Determine the run block indentation (deeper than body_indent)
        let run_indent = block_lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(body_indent + 4);

        let dedented = dedent_block(&block_lines.join("\n"), run_indent);
        let dedented = format!("{dedented}\n");

        let parsed = parse_unchecked(&dedented, Mode::Module.into());

        for err in parsed.errors() {
            self.errors.push(ParseError {
                message: err.to_string(),
                range: TextRange::new(
                    TextSize::new(start_offset as u32),
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
            TextSize::new(start_offset as u32),
            TextSize::new(block_end as u32),
        );

        SnakemakeDirective {
            keyword: DirectiveKeyword::Run,
            value: DirectiveValue::Block(stmts),
            range,
        }
    }
}

/// Count unmatched opening delimiters in `text`, ignoring those inside strings
/// and comments.
fn count_open_delimiters(text: &str) -> i32 {
    let mut depth: i32 = 0;
    let mut chars = text.chars().peekable();
    let mut in_string: Option<char> = None;
    let mut prev_backslash = false;

    while let Some(c) = chars.next() {
        if let Some(quote) = in_string {
            if c == '\\' && !prev_backslash {
                prev_backslash = true;
                continue;
            }
            if c == quote && !prev_backslash {
                in_string = None;
            }
            prev_backslash = false;
            continue;
        }

        match c {
            '#' => break, // rest of line is comment
            '\'' | '"' => {
                in_string = Some(c);
                prev_backslash = false;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }

    depth
}

/// Strip `indent` leading spaces from each line in a block of text.
fn dedent_block(text: &str, indent: usize) -> String {
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else {
                let spaces = line.len() - line.trim_start_matches(' ').len();
                let strip = spaces.min(indent);
                &line[strip..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

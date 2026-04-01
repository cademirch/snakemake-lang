//! Use rule parsing.

use ruff_python_ast::Identifier;
use ruff_text_size::{TextRange, TextSize};

use crate::ast::{RuleNames, SnakemakeDirective, SnakemakeUseRule, Statement};
use crate::errors::{ParseError, ParseErrorKind};

use super::Parser;

impl<'src> Parser<'src> {
    /// Parse a `use rule` statement.
    ///
    /// Syntax: `use rule NAMES from MODULE [exclude NAMES] [as PATTERN] [with: BLOCK]`
    pub(crate) fn parse_use_rule(&mut self) -> Statement {
        let header_line = &self.lines[self.cursor];
        let stmt_start = header_line.start;
        let line_number = header_line.number;
        let trimmed = header_line.trimmed();

        // Strip "use rule " prefix
        let after_use_rule = match trimmed.strip_prefix("use") {
            Some(rest) => rest.trim_start(),
            None => {
                self.errors.push(ParseError {
                    message: "expected 'use rule'".to_string(),
                    range: TextRange::new(
                        TextSize::new(stmt_start as u32),
                        TextSize::new((stmt_start + trimmed.len()) as u32),
                    ),
                    kind: ParseErrorKind::InvalidUseRule,
                    line: line_number,
                    column: 0,
                    source_line: Some(header_line.text.to_string()),
                });
                self.advance();
                return self.empty_use_rule(stmt_start);
            }
        };

        let after_rule = match after_use_rule.strip_prefix("rule") {
            Some(rest) => rest.trim_start(),
            None => {
                self.errors.push(ParseError {
                    message: "expected 'use rule'".to_string(),
                    range: TextRange::new(
                        TextSize::new(stmt_start as u32),
                        TextSize::new((stmt_start + trimmed.len()) as u32),
                    ),
                    kind: ParseErrorKind::InvalidUseRule,
                    line: line_number,
                    column: 0,
                    source_line: Some(header_line.text.to_string()),
                });
                self.advance();
                return self.empty_use_rule(stmt_start);
            }
        };

        // Tokenize the remainder into words, respecting the colon on "with:"
        let tokens = tokenize_use_rule(after_rule);

        // Parse rule names (everything before "from")
        let from_idx = tokens.iter().position(|t| t == "from");
        let (rules, from_module, exclude, name_modifier, has_with) = match from_idx {
            Some(fi) => {
                let name_tokens = &tokens[..fi];
                let rules = self.parse_rule_names(name_tokens, stmt_start, header_line.text);

                // Parse "from MODULE"
                let from_module = if fi + 1 < tokens.len() {
                    let mod_name = &tokens[fi + 1];
                    let mod_offset = find_token_offset(trimmed, mod_name, "from");
                    let abs_offset = stmt_start + header_line.indent + mod_offset;
                    Identifier::new(
                        mod_name.as_str(),
                        TextRange::new(
                            TextSize::new(abs_offset as u32),
                            TextSize::new((abs_offset + mod_name.len()) as u32),
                        ),
                    )
                } else {
                    self.errors.push(ParseError {
                        message: "expected module name after 'from'".to_string(),
                        range: TextRange::new(
                            TextSize::new(stmt_start as u32),
                            TextSize::new((stmt_start + trimmed.len()) as u32),
                        ),
                        kind: ParseErrorKind::InvalidUseRule,
                        line: line_number,
                        column: 0,
                        source_line: Some(header_line.text.to_string()),
                    });
                    Identifier::new("", TextRange::default())
                };

                // Parse optional clauses after "from MODULE"
                let rest_tokens = if fi + 2 < tokens.len() {
                    &tokens[fi + 2..]
                } else {
                    &[]
                };

                let mut exclude = Vec::new();
                let mut name_modifier = None;
                let mut has_with = false;
                let mut i = 0;

                while i < rest_tokens.len() {
                    match rest_tokens[i].as_str() {
                        "exclude" => {
                            i += 1;
                            while i < rest_tokens.len()
                                && rest_tokens[i] != "as"
                                && rest_tokens[i] != "with:"
                            {
                                let name = rest_tokens[i].trim_end_matches(',');
                                if !name.is_empty() {
                                    exclude.push(Identifier::new(
                                        name,
                                        TextRange::default(),
                                    ));
                                }
                                i += 1;
                            }
                        }
                        "as" => {
                            i += 1;
                            if i < rest_tokens.len() {
                                name_modifier = Some(rest_tokens[i].clone());
                                i += 1;
                            }
                        }
                        "with:" => {
                            has_with = true;
                            i += 1;
                        }
                        _ => {
                            i += 1;
                        }
                    }
                }

                (rules, from_module, exclude, name_modifier, has_with)
            }
            None => {
                self.errors.push(ParseError {
                    message: "expected 'from' in use rule statement".to_string(),
                    range: TextRange::new(
                        TextSize::new(stmt_start as u32),
                        TextSize::new((stmt_start + trimmed.len()) as u32),
                    ),
                    kind: ParseErrorKind::InvalidUseRule,
                    line: line_number,
                    column: 0,
                    source_line: Some(header_line.text.to_string()),
                });
                self.advance();
                return self.empty_use_rule(stmt_start);
            }
        };

        self.advance();

        // Parse with: block if present
        let with_directives = if has_with {
            let body_indent = self.peek_body_indent(0);
            let mut directives: Vec<SnakemakeDirective> = Vec::new();

            while !self.at_end() {
                let line = match self.current() {
                    Some(l) => l,
                    None => break,
                };

                if line.is_blank_or_comment() {
                    self.advance();
                    continue;
                }

                if line.indent == 0 {
                    break;
                }

                if let Some(directive) = self.try_parse_directive(body_indent) {
                    directives.push(directive);
                } else {
                    self.advance();
                }
            }
            Some(directives)
        } else {
            None
        };

        let stmt_end = if let Some(ref wd) = with_directives {
            if let Some(last) = wd.last() {
                last.range.end().to_u32() as usize
            } else {
                self.lines.get(self.cursor.saturating_sub(1))
                    .map_or(stmt_start, |l| l.start + l.text.len())
            }
        } else {
            self.lines.get(self.cursor.saturating_sub(1))
                .map_or(stmt_start, |l| l.start + l.text.len())
        };

        let range = TextRange::new(
            TextSize::new(stmt_start as u32),
            TextSize::new(stmt_end as u32),
        );

        Statement::UseRule(SnakemakeUseRule {
            rules,
            from_module,
            exclude,
            name_modifier,
            with_directives,
            range,
        })
    }

    fn parse_rule_names(
        &self,
        tokens: &[String],
        _stmt_start: usize,
        _line_text: &str,
    ) -> RuleNames {
        if tokens.len() == 1 && tokens[0] == "*" {
            return RuleNames::All;
        }

        let mut names = Vec::new();
        for token in tokens {
            let name = token.trim_end_matches(',');
            if !name.is_empty() {
                names.push(Identifier::new(name, TextRange::default()));
            }
        }

        RuleNames::Named(names)
    }

    fn empty_use_rule(&self, start: usize) -> Statement {
        Statement::UseRule(SnakemakeUseRule {
            rules: RuleNames::Named(Vec::new()),
            from_module: Identifier::new("", TextRange::default()),
            exclude: Vec::new(),
            name_modifier: None,
            with_directives: None,
            range: TextRange::new(
                TextSize::new(start as u32),
                TextSize::new(start as u32),
            ),
        })
    }
}

/// Tokenize the use rule remainder, keeping "with:" as a single token.
fn tokenize_use_rule(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in text.split_ascii_whitespace() {
        tokens.push(word.to_string());
    }
    tokens
}

/// Find the byte offset of a token in the trimmed line, searching after a preceding keyword.
fn find_token_offset(trimmed: &str, token: &str, after_keyword: &str) -> usize {
    if let Some(kw_pos) = trimmed.find(after_keyword) {
        let search_start = kw_pos + after_keyword.len();
        if let Some(rel) = trimmed[search_start..].find(token) {
            return search_start + rel;
        }
    }
    trimmed.find(token).unwrap_or(0)
}

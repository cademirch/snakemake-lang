//! Parse error types.

use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "serde")]
use crate::serde_helpers::serialize_text_range;

/// A parse error with location information.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ParseError {
    pub message: String,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_text_range"))]
    pub range: TextRange,
    pub kind: ParseErrorKind,

    // For Python SyntaxError compatibility
    pub line: usize,
    pub column: usize,
    pub source_line: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum ParseErrorKind {
    /// Expected a specific token (e.g., `:` after rule name)
    ExpectedToken,
    /// Rule name is required
    MissingRuleName,
    /// Unknown directive keyword in rule body
    UnknownDirective,
    /// Multiple execution directives (run + shell)
    MultipleExecutionDirectives,
    /// Deprecated keyword
    DeprecatedKeyword,
    /// Python syntax error in an expression or block
    PythonSyntaxError,
    /// Invalid use rule syntax
    InvalidUseRule,
    /// Indentation error
    IndentationError,
    /// Unexpected end of file
    UnexpectedEof,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

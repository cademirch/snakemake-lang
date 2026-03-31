//! Handler AST nodes (onsuccess, onerror, onstart).

use ruff_python_ast::Stmt;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

/// An event handler: `onsuccess:`, `onerror:`, or `onstart:`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeHandler {
    pub kind: HandlerKind,
    pub body: Vec<Stmt>,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum HandlerKind {
    OnSuccess,
    OnError,
    OnStart,
}

impl HandlerKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "onsuccess" => Some(Self::OnSuccess),
            "onerror" => Some(Self::OnError),
            "onstart" => Some(Self::OnStart),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OnSuccess => "onsuccess",
            Self::OnError => "onerror",
            Self::OnStart => "onstart",
        }
    }
}

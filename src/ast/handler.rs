//! Handler AST nodes (onsuccess, onerror, onstart).

use ruff_python_ast::Stmt;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "serde")]
use crate::serde_helpers::{serialize_stmt_vec, serialize_text_range};

/// An event handler: `onsuccess:`, `onerror:`, or `onstart:`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeHandler {
    pub kind: HandlerKind,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_stmt_vec"))]
    pub body: Vec<Stmt>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_text_range"))]
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
    #[allow(clippy::should_implement_trait)]
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

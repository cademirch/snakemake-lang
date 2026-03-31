//! Use rule AST node.

use ruff_python_ast::Identifier;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

use super::rule::SnakemakeDirective;

/// A `use rule` statement.
///
/// ```snakemake
/// use rule align, sort from qc_module exclude trim as qc_* with:
///     threads: 16
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeUseRule {
    /// Which rules to use: a list of names, or `*` for all.
    pub rules: RuleNames,

    /// The module to import rules from.
    pub from_module: Identifier,

    /// Rules to exclude (only valid with `*`).
    pub exclude: Vec<Identifier>,

    /// Optional name modifier pattern (e.g., `qc_*`).
    pub name_modifier: Option<String>,

    /// Optional `with:` block containing directive overrides.
    pub with_directives: Option<Vec<SnakemakeDirective>>,

    pub range: TextRange,
}

/// The rule name specification in a `use rule` statement.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum RuleNames {
    /// `use rule *` — all rules from the module.
    All,
    /// `use rule a, b, c` — specific named rules.
    Named(Vec<Identifier>),
}

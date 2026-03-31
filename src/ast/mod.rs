//! Snakemake AST node types.
//!
//! These extend ruff's Python AST with Snakemake-specific constructs.
//! Python expressions and statements within Snakemake nodes are
//! represented using ruff's `Expr` and `Stmt` types.

pub mod global;
pub mod handler;
pub mod module;
pub mod rule;
pub mod use_rule;

use ruff_python_ast::Stmt;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

pub use global::*;
pub use handler::*;
pub use module::*;
pub use rule::*;
pub use use_rule::*;

// ============================================================
// Root node
// ============================================================

/// Root AST node for a Snakemake file.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Snakefile {
    pub body: Vec<Statement>,
    pub range: TextRange,
}

// ============================================================
// Statement — top-level dispatch
// ============================================================

/// A top-level statement in a Snakemake file.
///
/// Either a Snakemake construct or pass-through Python code.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum Statement {
    /// `rule name:` ... or `checkpoint name:` ...
    Rule(SnakemakeRule),

    /// `module name:` ...
    Module(SnakemakeModule),

    /// `use rule ... from ... [with:]`
    UseRule(SnakemakeUseRule),

    /// Global directives: `configfile:`, `include:`, etc.
    GlobalDirective(SnakemakeGlobalDirective),

    /// `ruleorder: a > b > c`
    Ruleorder(SnakemakeRuleorder),

    /// `localrules: a, b, c`
    Localrules(SnakemakeLocalrules),

    /// `storage tag: ...`
    Storage(SnakemakeStorage),

    /// `onsuccess:` / `onerror:` / `onstart:`
    Handler(SnakemakeHandler),

    /// Pass-through Python code (imports, assignments, functions, classes, etc.)
    Python(Stmt),
}

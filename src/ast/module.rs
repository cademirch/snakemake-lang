//! Module AST node.

use ruff_python_ast::Identifier;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

use super::rule::DirectiveValue;

/// A `module` definition.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeModule {
    pub name: Identifier,
    pub directives: Vec<ModuleDirective>,
    pub docstring: Option<String>,
    pub range: TextRange,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ModuleDirective {
    pub keyword: ModuleKeyword,
    pub value: DirectiveValue,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum ModuleKeyword {
    Snakefile,
    MetaWrapper,
    Config,
    SkipValidation,
    ReplacePrefix,
    Prefix,
    Name,
    Pathvars,
}

impl ModuleKeyword {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "snakefile" => Some(Self::Snakefile),
            "meta_wrapper" => Some(Self::MetaWrapper),
            "config" => Some(Self::Config),
            "skip_validation" => Some(Self::SkipValidation),
            "replace_prefix" => Some(Self::ReplacePrefix),
            "prefix" => Some(Self::Prefix),
            "name" => Some(Self::Name),
            "pathvars" => Some(Self::Pathvars),
            _ => None,
        }
    }
}

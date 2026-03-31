//! Global directive AST nodes.

use ruff_python_ast::Identifier;
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

use super::rule::DirectiveValue;

/// A global directive like `configfile:`, `include:`, etc.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeGlobalDirective {
    pub keyword: GlobalKeyword,
    pub value: DirectiveValue,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum GlobalKeyword {
    Configfile,
    Include,
    Workdir,
    Envvars,
    Pathvars,
    Pepfile,
    Pepschema,
    Report,
    Scattergather,
    WildcardConstraints,
    Container,
    Containerized,
    Conda,
    ResourceScopes,
    InputFlags,
    OutputFlags,
}

impl GlobalKeyword {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "configfile" => Some(Self::Configfile),
            "include" => Some(Self::Include),
            "workdir" => Some(Self::Workdir),
            "envvars" => Some(Self::Envvars),
            "pathvars" => Some(Self::Pathvars),
            "pepfile" => Some(Self::Pepfile),
            "pepschema" => Some(Self::Pepschema),
            "report" => Some(Self::Report),
            "scattergather" => Some(Self::Scattergather),
            "wildcard_constraints" => Some(Self::WildcardConstraints),
            "container" => Some(Self::Container),
            "containerized" => Some(Self::Containerized),
            "conda" => Some(Self::Conda),
            "resource_scopes" => Some(Self::ResourceScopes),
            "inputflags" => Some(Self::InputFlags),
            "outputflags" => Some(Self::OutputFlags),
            _ => None,
        }
    }
}

/// `ruleorder: a > b > c`
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeRuleorder {
    pub names: Vec<Identifier>,
    pub range: TextRange,
}

/// `localrules: a, b, c`
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeLocalrules {
    pub names: Vec<Identifier>,
    pub range: TextRange,
}

/// `storage tag: provider="s3", ...`
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeStorage {
    pub tag: Identifier,
    pub value: DirectiveValue,
    pub range: TextRange,
}

//! Rule and checkpoint AST nodes.

use ruff_python_ast::{Expr, Identifier, Stmt};
use ruff_text_size::TextRange;

#[cfg(feature = "serde")]
use serde::Serialize;

// ============================================================
// Rule / Checkpoint
// ============================================================

/// A `rule` or `checkpoint` definition.
///
/// ```snakemake
/// rule align:
///     input: "reads/{sample}.fastq"
///     output: "aligned/{sample}.bam"
///     threads: 8
///     shell: "bwa mem -t {threads} {input} > {output}"
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeRule {
    /// Rule name. Required in the formalized grammar.
    pub name: Identifier,

    /// Directives in the rule body, in source order.
    pub directives: Vec<SnakemakeDirective>,

    /// Optional docstring.
    pub docstring: Option<String>,

    /// Whether this is a `checkpoint` rather than a `rule`.
    pub is_checkpoint: bool,

    /// Source range of the entire rule definition.
    pub range: TextRange,
}

// ============================================================
// Directive
// ============================================================

/// A single directive within a rule body.
///
/// ```snakemake
///     input: "reads/{sample}.fastq", ref="genome.fa"
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SnakemakeDirective {
    /// Which directive this is (input, output, shell, run, etc.)
    pub keyword: DirectiveKeyword,

    /// The value(s) after the colon.
    pub value: DirectiveValue,

    /// Source range of the entire directive (keyword through end of value).
    pub range: TextRange,
}

/// The value portion of a directive.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum DirectiveValue {
    /// Comma-separated arguments (positional and keyword).
    /// Used by input, output, params, resources, etc.
    ///
    /// `input: "a.txt", "b.txt", ref="genome.fa"`
    Arguments(DirectiveArguments),

    /// A Python code block. Only used by `run:`.
    ///
    /// ```snakemake
    /// run:
    ///     with open(input[0]) as f:
    ///         data = f.read()
    /// ```
    Block(Vec<Stmt>),
}

/// Parsed argument list for a directive value.
///
/// Mirrors Python's function call argument syntax:
/// positional args, then keyword args.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct DirectiveArguments {
    /// Positional arguments.
    pub positional: Vec<Expr>,

    /// Keyword arguments (name=value).
    pub keywords: Vec<DirectiveKeywordArgument>,

    /// Source range of the entire argument list.
    pub range: TextRange,
}

/// A keyword argument in a directive: `name=value`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct DirectiveKeywordArgument {
    pub name: Identifier,
    pub value: Expr,
    pub range: TextRange,
}

// ============================================================
// Directive keywords
// ============================================================

/// All recognized directive keywords within a rule body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum DirectiveKeyword {
    // I/O
    Input,
    Output,
    Params,
    Log,
    Benchmark,

    // Execution (mutually exclusive)
    Run,
    Shell,
    Script,
    Notebook,
    Wrapper,
    TemplateEngine,
    Cwl,

    // Resources
    Threads,
    Resources,
    Retries,
    Priority,

    // Environment
    Conda,
    Container,
    Containerized,
    EnvModules,
    Shadow,

    // Metadata
    Message,
    WildcardConstraints,
    Group,
    Name,

    // Flags
    Cache,
    DefaultTarget,
    Handover,
    Localrule,

    // Path
    Pathvars,
}

impl DirectiveKeyword {
    /// Parse a keyword string into a `DirectiveKeyword`.
    /// Returns `None` if the string is not a recognized keyword.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "input" => Some(Self::Input),
            "output" => Some(Self::Output),
            "params" => Some(Self::Params),
            "log" => Some(Self::Log),
            "benchmark" => Some(Self::Benchmark),
            "run" => Some(Self::Run),
            "shell" => Some(Self::Shell),
            "script" => Some(Self::Script),
            "notebook" => Some(Self::Notebook),
            "wrapper" => Some(Self::Wrapper),
            "template_engine" => Some(Self::TemplateEngine),
            "cwl" => Some(Self::Cwl),
            "threads" => Some(Self::Threads),
            "resources" => Some(Self::Resources),
            "retries" => Some(Self::Retries),
            "priority" => Some(Self::Priority),
            "conda" => Some(Self::Conda),
            "container" => Some(Self::Container),
            "containerized" => Some(Self::Containerized),
            "envmodules" => Some(Self::EnvModules),
            "shadow" => Some(Self::Shadow),
            "message" => Some(Self::Message),
            "wildcard_constraints" => Some(Self::WildcardConstraints),
            "group" => Some(Self::Group),
            "name" => Some(Self::Name),
            "cache" => Some(Self::Cache),
            "default_target" => Some(Self::DefaultTarget),
            "handover" => Some(Self::Handover),
            "localrule" => Some(Self::Localrule),
            "pathvars" => Some(Self::Pathvars),
            _ => None,
        }
    }

    /// The keyword as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Params => "params",
            Self::Log => "log",
            Self::Benchmark => "benchmark",
            Self::Run => "run",
            Self::Shell => "shell",
            Self::Script => "script",
            Self::Notebook => "notebook",
            Self::Wrapper => "wrapper",
            Self::TemplateEngine => "template_engine",
            Self::Cwl => "cwl",
            Self::Threads => "threads",
            Self::Resources => "resources",
            Self::Retries => "retries",
            Self::Priority => "priority",
            Self::Conda => "conda",
            Self::Container => "container",
            Self::Containerized => "containerized",
            Self::EnvModules => "envmodules",
            Self::Shadow => "shadow",
            Self::Message => "message",
            Self::WildcardConstraints => "wildcard_constraints",
            Self::Group => "group",
            Self::Name => "name",
            Self::Cache => "cache",
            Self::DefaultTarget => "default_target",
            Self::Handover => "handover",
            Self::Localrule => "localrule",
            Self::Pathvars => "pathvars",
        }
    }

    /// Whether this is an execution directive (run, shell, script, etc.).
    /// Only one execution directive is allowed per rule.
    pub fn is_execution(&self) -> bool {
        matches!(
            self,
            Self::Run
                | Self::Shell
                | Self::Script
                | Self::Notebook
                | Self::Wrapper
                | Self::TemplateEngine
                | Self::Cwl
        )
    }
}

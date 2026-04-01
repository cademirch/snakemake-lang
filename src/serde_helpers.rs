//! Custom serde serializers for ruff AST types.
//!
//! Ruff's `TextRange`, `Identifier`, `Expr`, and `Stmt` don't implement
//! `serde::Serialize`. These helpers bridge that gap for the JSON output
//! in the `parse` CLI subcommand.

use ruff_python_ast::{Expr, Identifier, Stmt};
use ruff_text_size::TextRange;
use serde::Serializer;

pub fn serialize_text_range<S>(range: &TextRange, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeStruct;
    let mut st = s.serialize_struct("TextRange", 2)?;
    st.serialize_field("start", &u32::from(range.start()))?;
    st.serialize_field("end", &u32::from(range.end()))?;
    st.finish()
}

pub fn serialize_identifier<S>(id: &Identifier, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(id.as_str())
}

pub fn serialize_identifier_vec<S>(ids: &[Identifier], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(ids.len()))?;
    for id in ids {
        seq.serialize_element(id.as_str())?;
    }
    seq.end()
}

pub fn serialize_expr<S>(expr: &Expr, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&format!("{expr:?}"))
}

pub fn serialize_expr_vec<S>(exprs: &[Expr], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(exprs.len()))?;
    for expr in exprs {
        seq.serialize_element(&format!("{expr:?}"))?;
    }
    seq.end()
}

pub fn serialize_stmt_vec<S>(stmts: &[Stmt], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(stmts.len()))?;
    for stmt in stmts {
        seq.serialize_element(&format!("{stmt:?}"))?;
    }
    seq.end()
}

pub fn serialize_opt_identifier<S>(
    id: &Option<Identifier>,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match id {
        Some(id) => s.serialize_some(id.as_str()),
        None => s.serialize_none(),
    }
}

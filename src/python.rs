//! Python bindings via PyO3.
//!
//! Exposes `snakemake-lang` as a Python module for use by the
//! Snakemake engine as a drop-in replacement for `parser.py`.

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
use std::collections::HashMap;

/// Parse and compile a Snakefile, returning (python_code, linemap).
///
/// This is the drop-in replacement for `snakemake.parser.parse()`.
#[cfg(feature = "python")]
#[pyfunction]
fn parse_and_compile(source: &str, path: &str) -> PyResult<(String, HashMap<usize, usize>)> {
    let result = crate::compile(source, path).map_err(|errors| {
        let err = &errors[0];
        pyo3::exceptions::PySyntaxError::new_err((
            err.message.clone(),
            (
                path.to_string(),
                err.line as u64,
                err.column as u64,
                err.source_line.clone().unwrap_or_default(),
            ),
        ))
    })?;

    let linemap = result.source_map.to_linemap(&result.python, source);
    Ok((result.python, linemap))
}

/// Parse a Snakefile and return the AST as JSON.
#[cfg(feature = "python")]
#[pyfunction]
fn parse_to_json(source: &str, path: &str) -> PyResult<String> {
    #[cfg(not(feature = "serde"))]
    {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "snakemake-lang was built without serde support",
        ))
    }

    #[cfg(feature = "serde")]
    {
        let ast = crate::parse(source, path).map_err(|errors| {
            let err = &errors[0];
            pyo3::exceptions::PySyntaxError::new_err(err.message.clone())
        })?;
        serde_json::to_string_pretty(&ast)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// The Python module definition.
#[cfg(feature = "python")]
#[pymodule]
fn snakemake_lang(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_and_compile, m)?)?;
    m.add_function(wrap_pyfunction!(parse_to_json, m)?)?;
    Ok(())
}

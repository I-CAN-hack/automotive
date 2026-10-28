//! Private conversion and runtime boundary for the public Python package.

use pyo3::prelude::*;

mod config;
mod errors;
mod runtime;
mod uds;
mod xcp;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(m)?;
    config::register(m)?;
    runtime::register(m)?;
    m.add_class::<uds::Client>()?;
    xcp::register(m)?;
    Ok(())
}

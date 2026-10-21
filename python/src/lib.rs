//! Private conversion and runtime boundary for the public Python package.

use pyo3::prelude::*;

mod config;
mod errors;
mod runtime;
mod uds;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(m)?;
    config::register(m)?;
    runtime::register(m)?;
    m.add_class::<uds::Client>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

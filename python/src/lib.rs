//! Private conversion and runtime boundary for the public Python package.

use pyo3::prelude::*;

mod ccp;
mod config;
mod doip;
mod errors;
mod runtime;
mod uds;
mod xcp;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(m)?;
    config::register(m)?;
    runtime::register(m)?;
    doip::register(m)?;
    m.add_class::<uds::Client>()?;
    xcp::register(m)?;
    ccp::register(m)?;
    Ok(())
}

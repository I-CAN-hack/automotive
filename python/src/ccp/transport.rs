//! CCP transport ownership for Python clients.
use std::sync::{Arc, Mutex};

use automotive::protocols::ccp::{CCPCanConfig, CCPClient, CCPConfig, CCPOnCAN, CCPTransport};
use pyo3::{
    exceptions::{PyConnectionError, PyRuntimeError},
    prelude::*,
};
use tokio::sync::watch;

use crate::{config, errors::convert, runtime::CanAdapter};

struct Slot {
    transport: Option<Box<dyn CCPTransport>>,
    client: Option<CCPClient>,
    closed: bool,
}

pub struct State {
    slot: Mutex<Slot>,
    parent: Option<watch::Sender<bool>>,
}

impl State {
    pub fn attach(&self, config: CCPConfig) -> PyResult<CCPClient> {
        if self.closed() {
            return Err(PyConnectionError::new_err("packet transport is closed"));
        }
        let mut slot = self.slot.lock().unwrap();
        let transport = slot.transport.take().ok_or_else(|| {
            PyRuntimeError::new_err("packet transport already has a protocol client")
        })?;
        let client = CCPClient::with_config(transport, config).map_err(convert)?;
        slot.client = Some(client.clone());
        Ok(client)
    }
    pub fn closed(&self) -> bool {
        let slot = self.slot.lock().unwrap();
        slot.closed
            || slot.client.as_ref().is_some_and(|c| c.is_closed())
            || self.parent.as_ref().is_some_and(|p| *p.borrow())
    }
    pub async fn close(&self) {
        let client = {
            let mut slot = self.slot.lock().unwrap();
            slot.closed = true;
            slot.transport.take();
            slot.client.take()
        };
        if let Some(client) = client {
            client.close().await;
        }
    }
}

#[pyclass(name = "_CCPTransport", module = "automotive._native")]
pub struct Transport {
    pub state: Arc<State>,
}

impl Transport {
    fn new(transport: impl CCPTransport + 'static, parent: Option<watch::Sender<bool>>) -> Self {
        Self {
            state: Arc::new(State {
                slot: Mutex::new(Slot {
                    transport: Some(Box::new(transport)),
                    client: None,
                    closed: false,
                }),
                parent,
            }),
        }
    }
}

#[pymethods]
impl Transport {
    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            state.close().await;
            Ok(())
        })
    }
    #[getter]
    fn closed(&self) -> bool {
        self.state.closed()
    }
}

fn can_config(obj: &Bound<'_, PyAny>) -> PyResult<CCPCanConfig> {
    let config = CCPCanConfig {
        tx_id: config::identifier(&obj.getattr("tx_id")?)?,
        rx_id: config::identifier(&obj.getattr("rx_id")?)?,
        bus: obj.getattr("bus")?.extract()?,
    };
    config.validate().map_err(convert)?;
    Ok(config)
}

#[pyfunction]
fn validate_ccp_can(config: &Bound<'_, PyAny>) -> PyResult<()> {
    can_config(config).map(|_| ())
}

#[pyfunction]
fn open_ccp_can(adapter: &CanAdapter, config: &Bound<'_, PyAny>) -> PyResult<Transport> {
    if *adapter.closed.borrow() {
        return Err(PyConnectionError::new_err("CAN adapter is closed"));
    }
    Ok(Transport::new(
        CCPOnCAN::new(adapter.inner.clone(), can_config(config)?).map_err(convert)?,
        Some(adapter.closed.clone()),
    ))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Transport>()?;
    m.add_function(wrap_pyfunction!(validate_ccp_can, m)?)?;
    m.add_function(wrap_pyfunction!(open_ccp_can, m)?)?;
    Ok(())
}

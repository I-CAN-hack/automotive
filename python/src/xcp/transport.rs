//! XCP transport ownership for Python clients.
use std::sync::{Arc, Mutex};

use automotive::protocols::xcp::{
    PacketLimits, XCPCanConfig, XCPClient, XCPConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP, XCPTransport,
};
use pyo3::{
    exceptions::{PyConnectionError, PyRuntimeError, PyValueError},
    prelude::*,
};
use tokio::sync::watch;

use crate::{config, errors::convert, runtime::CanAdapter};

struct Slot {
    transport: Option<Box<dyn XCPTransport>>,
    client: Option<XCPClient>,
    closed: bool,
}

pub struct State {
    slot: Mutex<Slot>,
    parent: Option<watch::Sender<bool>>,
}

impl State {
    pub fn attach(&self, config: XCPConfig) -> PyResult<XCPClient> {
        if self.closed() {
            return Err(PyConnectionError::new_err("packet transport is closed"));
        }
        let mut slot = self.slot.lock().unwrap();
        let transport = slot.transport.take().ok_or_else(|| {
            PyRuntimeError::new_err("packet transport already has a protocol client")
        })?;
        let client = XCPClient::with_config(transport, config).map_err(convert)?;
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

#[pyclass(name = "_XCPTransport", module = "automotive._native")]
pub struct Transport {
    pub state: Arc<State>,
    limits: PacketLimits,
}

impl Transport {
    fn new(transport: impl XCPTransport + 'static, parent: Option<watch::Sender<bool>>) -> Self {
        Self {
            limits: transport.limits(),
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
    #[getter]
    fn max_tx_packet_size(&self) -> usize {
        self.limits.max_tx_packet_size
    }
    #[getter]
    fn max_rx_packet_size(&self) -> usize {
        self.limits.max_rx_packet_size
    }
}

fn can_config(obj: &Bound<'_, PyAny>) -> PyResult<XCPCanConfig> {
    let config = XCPCanConfig {
        tx_id: config::identifier(&obj.getattr("tx_id")?)?,
        rx_id: config::identifier(&obj.getattr("rx_id")?)?,
        bus: obj.getattr("bus")?.extract()?,
        fd: obj.getattr("fd")?.extract()?,
        brs: obj.getattr("brs")?.extract()?,
        padding: obj.getattr("padding")?.extract()?,
        max_dlen: obj.getattr("max_dlen")?.extract()?,
    };
    config.validate().map_err(convert)?;
    Ok(config)
}

#[pyfunction]
fn validate_xcp_can(config: &Bound<'_, PyAny>) -> PyResult<()> {
    can_config(config).map(|_| ())
}

#[pyfunction]
fn open_xcp_can(adapter: &CanAdapter, config: &Bound<'_, PyAny>) -> PyResult<Transport> {
    if *adapter.closed.borrow() {
        return Err(PyConnectionError::new_err("CAN adapter is closed"));
    }
    Ok(Transport::new(
        XCPOnCAN::new(adapter.inner.clone(), can_config(config)?).map_err(convert)?,
        Some(adapter.closed.clone()),
    ))
}

fn timeout(seconds: f64) -> PyResult<std::time::Duration> {
    let duration = config::duration(seconds)?;
    if duration.is_zero() || std::time::Instant::now().checked_add(duration).is_none() {
        return Err(PyValueError::new_err(
            "connection timeout must be positive and representable",
        ));
    }
    Ok(duration)
}

#[pyfunction]
fn open_xcp_tcp<'py>(
    py: Python<'py>,
    host: String,
    port: u16,
    seconds: f64,
) -> PyResult<Bound<'py, PyAny>> {
    let timeout = timeout(seconds)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        Ok(Transport::new(
            XCPOnTCP::connect_with_timeout((host.as_str(), port), timeout)
                .await
                .map_err(convert)?,
            None,
        ))
    })
}
#[pyfunction]
#[pyo3(signature = (host, port, seconds, local_addr=None))]
fn open_xcp_udp<'py>(
    py: Python<'py>,
    host: String,
    port: u16,
    seconds: f64,
    local_addr: Option<(String, u16)>,
) -> PyResult<Bound<'py, PyAny>> {
    let timeout = timeout(seconds)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let transport = match local_addr {
            Some((local_host, local_port)) => {
                XCPOnUDP::bind_with_timeout(
                    (local_host.as_str(), local_port),
                    (host.as_str(), port),
                    timeout,
                )
                .await
            }
            None => XCPOnUDP::connect_with_timeout((host.as_str(), port), timeout).await,
        }
        .map_err(convert)?;
        Ok(Transport::new(transport, None))
    })
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Transport>()?;
    m.add_function(wrap_pyfunction!(validate_xcp_can, m)?)?;
    m.add_function(wrap_pyfunction!(open_xcp_can, m)?)?;
    m.add_function(wrap_pyfunction!(open_xcp_tcp, m)?)?;
    m.add_function(wrap_pyfunction!(open_xcp_udp, m)?)?;
    Ok(())
}

//! Python conversions for the Rust DoIP implementation.
use automotive::transports::doip::{
    self, Discovery, DiscoveryConfig, DoIPConfig, VehicleAnnouncement, VehicleIdentification,
};
use pyo3::{
    exceptions::{PyConnectionError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

use crate::{
    config::duration,
    errors::convert,
    runtime::{PyTransport, State, Transport},
};

fn address(value: String) -> PyResult<SocketAddr> {
    value.parse().map_err(|_| {
        PyValueError::new_err(
            "expected an IP endpoint such as 192.168.0.1:13400 or [fe80::1%7]:13400",
        )
    })
}

fn tcp_config(value: &Bound<'_, PyAny>, target: u16) -> PyResult<DoIPConfig> {
    let mut config = DoIPConfig::new(target);
    config.source_address = value.getattr("source_address")?.extract()?;
    config.protocol_version = value.getattr("protocol_version")?.extract()?;
    config.activation_type = value.getattr("activation_type")?.extract()?;
    let oem = value
        .getattr("activation_oem")?
        .extract::<Option<Vec<u8>>>()?;
    config.activation_oem = oem
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| PyValueError::new_err("activation_oem must contain four bytes"))
        })
        .transpose()?;
    config.local_address = value
        .getattr("local_address")?
        .extract::<Option<String>>()?
        .map(address)
        .transpose()?;
    config.connect_timeout = duration(value.getattr("connect_timeout")?.extract()?)?;
    config.activation_timeout = duration(value.getattr("activation_timeout")?.extract()?)?;
    config.ack_timeout = duration(value.getattr("ack_timeout")?.extract()?)?;
    config.max_payload_size = value.getattr("max_payload_size")?.extract()?;
    Ok(config)
}

fn discovery_config(value: &Bound<'_, PyAny>) -> PyResult<DiscoveryConfig> {
    let mut config = DiscoveryConfig {
        bind_address: address(value.getattr("bind_address")?.extract()?)?,
        destination: address(value.getattr("destination")?.extract()?)?,
        interface_index: value.getattr("interface_index")?.extract()?,
        protocol_version: value.getattr("protocol_version")?.extract()?,
        timeout: duration(value.getattr("timeout")?.extract()?)?,
        reuse_address: value.getattr("reuse_address")?.extract()?,
        ..Default::default()
    };
    let vin = value.getattr("vin")?.extract::<Option<Vec<u8>>>()?;
    let eid = value.getattr("eid")?.extract::<Option<Vec<u8>>>()?;
    config.identification = match (vin, eid) {
        (Some(_), Some(_)) => {
            return Err(PyValueError::new_err("vin and eid are mutually exclusive"))
        }
        (Some(bytes), None) => VehicleIdentification::Vin(
            bytes
                .try_into()
                .map_err(|_| PyValueError::new_err("vin must contain 17 bytes"))?,
        ),
        (None, Some(bytes)) => VehicleIdentification::Eid(
            bytes
                .try_into()
                .map_err(|_| PyValueError::new_err("eid must contain six bytes"))?,
        ),
        _ => VehicleIdentification::All,
    };
    Ok(config)
}

fn announcement(py: Python<'_>, value: VehicleAnnouncement) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    result.set_item("address", value.address.to_string())?;
    result.set_item("protocol_version", value.protocol_version)?;
    result.set_item("vin", PyBytes::new(py, &value.vin))?;
    result.set_item("logical_address", value.logical_address)?;
    result.set_item("eid", PyBytes::new(py, &value.eid))?;
    result.set_item("gid", PyBytes::new(py, &value.gid))?;
    result.set_item("further_action", value.further_action)?;
    result.set_item("vin_gid_sync_status", value.vin_gid_sync_status)?;
    Ok(result.into_any().unbind())
}

#[pyfunction]
fn connect_doip<'py>(
    py: Python<'py>,
    endpoint: String,
    target: u16,
    config: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let config = tcp_config(config, target)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let transport = doip::DoIP::connect_with_config(endpoint, config)
            .await
            .map_err(convert)?;
        Ok(PyTransport {
            state: State::new(Transport::DoIP(transport), None),
        })
    })
}

#[pyfunction]
fn discover_doip<'py>(py: Python<'py>, config: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let config = discovery_config(config)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let vehicles = doip::discover_with_config(config).await.map_err(convert)?;
        Python::attach(|py| {
            vehicles
                .into_iter()
                .map(|value| announcement(py, value))
                .collect::<PyResult<Vec<_>>>()
        })
    })
}

#[pyclass(name = "_DoIPDiscovery", module = "automotive._native")]
struct PyDiscovery {
    socket: Mutex<Option<Arc<Discovery>>>,
    closed: watch::Sender<bool>,
}

#[pyfunction]
fn bind_doip<'py>(py: Python<'py>, config: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let config = discovery_config(config)?;
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        Ok(PyDiscovery {
            socket: Mutex::new(Some(Arc::new(Discovery::bind(config).map_err(convert)?))),
            closed: watch::channel(false).0,
        })
    })
}

impl PyDiscovery {
    fn socket(&self) -> PyResult<Arc<Discovery>> {
        self.socket
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or_else(|| PyConnectionError::new_err("discovery socket closed"))
    }
}

#[pymethods]
impl PyDiscovery {
    #[getter]
    fn local_address(&self) -> PyResult<String> {
        Ok(self.socket()?.local_address().map_err(convert)?.to_string())
    }
    fn request<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let socket = self.socket()?;
        let mut closed = self.closed.subscribe();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::select! {
                biased;
                _ = closed.wait_for(|value| *value) => Err(PyConnectionError::new_err("discovery socket closed")),
                result = socket.request() => result.map_err(convert),
            }
        })
    }
    fn recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let socket = self.socket()?;
        let mut closed = self.closed.subscribe();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = tokio::select! {
                biased;
                _ = closed.wait_for(|value| *value) => return Err(PyConnectionError::new_err("discovery socket closed")),
                result = socket.recv() => result.map_err(convert)?,
            };
            Python::attach(|py| announcement(py, value))
        })
    }
    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.closed.send_replace(true);
        self.socket
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        pyo3_async_runtimes::tokio::future_into_py(py, async { Ok(()) })
    }
    #[getter]
    fn closed(&self) -> bool {
        *self.closed.borrow()
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(connect_doip, module)?)?;
    module.add_function(wrap_pyfunction!(discover_doip, module)?)?;
    module.add_function(wrap_pyfunction!(bind_doip, module)?)?;
    module.add_class::<PyDiscovery>()?;
    Ok(())
}

mod transport;

use automotive::protocols::xcp::{self, XCPClient, XCPConfig};
use pyo3::{
    exceptions::{PyConnectionError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};
use strum::IntoEnumIterator;
use transport::{State, Transport};

use crate::{config, errors::convert};

fn timing(obj: &Bound<'_, PyAny>) -> PyResult<XCPConfig> {
    let timeout = config::duration(obj.getattr("timeout")?.extract()?)?;
    if timeout.is_zero() || std::time::Instant::now().checked_add(timeout).is_none() {
        return Err(PyValueError::new_err(
            "XCP timeout must be positive and representable",
        ));
    }
    Ok(XCPConfig { timeout })
}

#[pyfunction]
fn validate_xcp(config: &Bound<'_, PyAny>) -> PyResult<()> {
    timing(config).map(|_| ())
}

#[pyfunction]
fn xcp_defaults(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    result.set_item("timeout", XCPConfig::default().timeout.as_secs_f64())?;
    Ok(result.into_any().unbind())
}

#[pyfunction]
fn xcp_enum_values(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let enums = PyDict::new(py);
    macro_rules! add { ($($name:ident),*) => { $(
        let values = PyDict::new(py);
        for value in xcp::$name::iter() { values.set_item(format!("{value:?}"), value.as_raw())?; }
        enums.set_item(stringify!($name), values)?;
    )* }; }
    add!(
        Command,
        Resource,
        ConnectMode,
        IdentificationType,
        ErrorCode
    );
    Ok(enums.into_any().unbind())
}

enum Command {
    Connect(u8),
    Disconnect,
    Synchronize,
    Status,
    CommMode,
    Id(u8),
    Seed(u8, u8),
    Unlock(u8, Vec<u8>),
    SetMta(u32, u8),
    Upload(usize),
    ShortUpload(u32, usize, u8),
    Download(Vec<u8>),
    Checksum(u32),
}

enum Value {
    None,
    Bytes(Vec<u8>),
    Integer(u8),
    Connect(xcp::ConnectInfo),
    Status(xcp::Status),
    CommMode(xcp::CommunicationModeInfo),
    Id(xcp::Identification),
    Seed(xcp::Seed),
    Checksum(xcp::Checksum),
}

impl Command {
    fn parse(method: &str, args: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(match method {
            "connect" => Self::Connect(args.extract::<(u8,)>()?.0),
            "disconnect" => Self::Disconnect,
            "synchronize" => Self::Synchronize,
            "get_status" => Self::Status,
            "get_comm_mode_info" => Self::CommMode,
            "get_id" => Self::Id(args.extract::<(u8,)>()?.0),
            "get_seed" => {
                let (mode, resource) = args.extract()?;
                Self::Seed(mode, resource)
            }
            "unlock" => {
                let (length, key) = args.extract()?;
                Self::Unlock(length, key)
            }
            "set_mta" => {
                let (a, b) = args.extract()?;
                Self::SetMta(a, b)
            }
            "upload" => Self::Upload(args.extract::<(usize,)>()?.0),
            "short_upload" => {
                let (a, b, c) = args.extract()?;
                Self::ShortUpload(a, b, c)
            }
            "download" => Self::Download(args.extract::<(Vec<u8>,)>()?.0),
            "build_checksum" => Self::Checksum(args.extract::<(u32,)>()?.0),
            _ => return Err(PyValueError::new_err("unknown XCP method")),
        })
    }

    async fn execute(self, client: &XCPClient) -> automotive::Result<Value> {
        Ok(match self {
            Self::Connect(m) => Value::Connect(client.connect(m).await?),
            Self::Disconnect => {
                client.disconnect().await?;
                Value::None
            }
            Self::Synchronize => {
                client.synchronize().await?;
                Value::None
            }
            Self::Status => Value::Status(client.get_status().await?),
            Self::CommMode => Value::CommMode(client.get_comm_mode_info().await?),
            Self::Id(t) => Value::Id(client.get_id(t).await?),
            Self::Seed(m, r) => Value::Seed(client.get_seed(m, r).await?),
            Self::Unlock(n, k) => Value::Integer(client.unlock(n, &k).await?),
            Self::SetMta(a, e) => {
                client.set_mta(a, e).await?;
                Value::None
            }
            Self::Upload(n) => Value::Bytes(client.upload(n).await?),
            Self::ShortUpload(a, n, e) => Value::Bytes(client.short_upload(a, n, e).await?),
            Self::Download(d) => {
                client.download(&d).await?;
                Value::None
            }
            Self::Checksum(n) => Value::Checksum(client.build_checksum(n).await?),
        })
    }
}

impl Value {
    fn python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = PyDict::new(py);
        macro_rules! fields { ($value:ident, $($field:ident),+) => { $(result.set_item(stringify!($field), $value.$field)?;)+ }; }
        match self {
            Self::None => return Ok(py.None()),
            Self::Bytes(v) => return Ok(PyBytes::new(py, &v).into_any().unbind()),
            Self::Integer(v) => return Ok(v.into_pyobject(py)?.into_any().unbind()),
            Self::Connect(v) => {
                fields!(
                    v,
                    resource,
                    comm_mode_basic,
                    address_granularity,
                    max_cto,
                    max_dto,
                    protocol_version,
                    transport_version
                );
                result.set_item(
                    "byte_order",
                    if v.byte_order == xcp::ByteOrder::LittleEndian {
                        "little"
                    } else {
                        "big"
                    },
                )?;
                result.set_item("max_upload_size", v.max_upload_size())?;
            }
            Self::Status(v) => {
                fields!(
                    v,
                    session_status,
                    resource_protection_status,
                    session_configuration_id
                );
            }
            Self::CommMode(v) => {
                fields!(
                    v,
                    comm_mode_optional,
                    max_bs,
                    min_st,
                    queue_size,
                    driver_version
                );
            }
            Self::Id(v) => {
                fields!(v, mode, length);
                result.set_item("data", PyBytes::new(py, &v.data))?;
            }
            Self::Seed(v) => {
                fields!(v, length);
                result.set_item("data", PyBytes::new(py, &v.data))?;
            }
            Self::Checksum(v) => {
                fields!(v, checksum_type, checksum);
            }
        }
        Ok(result.into_any().unbind())
    }
}

#[pyclass(name = "_XCP", module = "automotive._native")]
pub struct Client {
    inner: XCPClient,
    parent: std::sync::Arc<State>,
}

#[pymethods]
impl Client {
    #[new]
    fn new(transport: &Transport, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: transport.state.attach(timing(config)?)?,
            parent: transport.state.clone(),
        })
    }

    fn call<'py>(
        &self,
        py: Python<'py>,
        method: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.closed() {
            return Err(PyConnectionError::new_err("XCP client is closed"));
        }
        let command = Command::parse(method, args)?;
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = command.execute(&client).await.map_err(convert)?;
            Python::attach(|py| value.python(py))
        })
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client.close().await;
            Ok(())
        })
    }

    #[getter]
    fn closed(&self) -> bool {
        self.inner.is_closed() || self.parent.closed()
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    transport::register(m)?;
    m.add_class::<Client>()?;
    m.add_function(wrap_pyfunction!(validate_xcp, m)?)?;
    m.add_function(wrap_pyfunction!(xcp_defaults, m)?)?;
    m.add_function(wrap_pyfunction!(xcp_enum_values, m)?)?;
    Ok(())
}

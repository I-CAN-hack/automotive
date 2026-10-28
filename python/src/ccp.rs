mod transport;

use crate::{config, errors::convert};
use automotive::protocols::ccp::{self, CCPClient, CCPConfig};
use pyo3::{
    exceptions::{PyConnectionError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};
use strum::IntoEnumIterator;
use transport::{State, Transport};

fn settings(obj: &Bound<'_, PyAny>) -> PyResult<CCPConfig> {
    let timeout = config::duration(obj.getattr("timeout")?.extract()?)?;
    if timeout.is_zero() || std::time::Instant::now().checked_add(timeout).is_none() {
        return Err(PyValueError::new_err(
            "CCP timeout must be positive and representable",
        ));
    }
    let byte_order = match obj.getattr("byte_order")?.extract::<String>()?.as_str() {
        "little" => ccp::ByteOrder::LittleEndian,
        "big" => ccp::ByteOrder::BigEndian,
        _ => {
            return Err(PyValueError::new_err(
                "byte_order must be 'little' or 'big'",
            ))
        }
    };
    Ok(CCPConfig {
        timeout,
        byte_order,
    })
}

#[pyfunction]
fn validate_ccp(config: &Bound<'_, PyAny>) -> PyResult<()> {
    settings(config).map(|_| ())
}

#[pyfunction]
fn ccp_defaults(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let config = CCPConfig::default();
    result.set_item("timeout", config.timeout.as_secs_f64())?;
    result.set_item(
        "byte_order",
        if config.byte_order == ccp::ByteOrder::LittleEndian {
            "little"
        } else {
            "big"
        },
    )?;
    Ok(result.into_any().unbind())
}

#[pyfunction]
fn ccp_enum_values(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let enums = PyDict::new(py);
    macro_rules! add { ($($name:ident),*) => { $(
        let values = PyDict::new(py);
        for value in ccp::$name::iter() { values.set_item(format!("{value:?}"), value.as_raw())?; }
        enums.set_item(stringify!($name), values)?;
    )* }; }
    add!(Command, Resource, ErrorCode);
    Ok(enums.into_any().unbind())
}

enum Command {
    Connect(u16),
    Disconnect(bool),
    Version(u8, u8),
    Id(Vec<u8>),
    Seed(u8),
    Unlock(Vec<u8>),
    SetMta(u32, u8, u8),
    Upload(usize),
    ShortUpload(u32, usize, u8),
    Download(Vec<u8>),
    Download6([u8; 6]),
}

enum Value {
    None,
    Bytes(Vec<u8>),
    Integer(u8),
    Version(ccp::ProtocolVersion),
    Id(ccp::Identification),
    Seed(ccp::Seed),
    Address(ccp::MemoryAddress),
}

impl Command {
    fn parse(method: &str, args: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(match method {
            "connect" => Self::Connect(args.extract::<(u16,)>()?.0),
            "disconnect" => Self::Disconnect(args.extract::<(bool,)>()?.0),
            "get_ccp_version" => {
                let (a, b) = args.extract()?;
                Self::Version(a, b)
            }
            "exchange_id" => Self::Id(args.extract::<(Vec<u8>,)>()?.0),
            "get_seed" => Self::Seed(args.extract::<(u8,)>()?.0),
            "unlock" => Self::Unlock(args.extract::<(Vec<u8>,)>()?.0),
            "set_mta" => {
                let (a, b, c) = args.extract()?;
                Self::SetMta(a, b, c)
            }
            "upload" => Self::Upload(args.extract::<(usize,)>()?.0),
            "short_upload" => {
                let (a, b, c) = args.extract()?;
                Self::ShortUpload(a, b, c)
            }
            "download" => Self::Download(args.extract::<(Vec<u8>,)>()?.0),
            "download6" => Self::Download6(
                args.extract::<(Vec<u8>,)>()?
                    .0
                    .try_into()
                    .map_err(|_| PyValueError::new_err("DNLOAD_6 requires exactly six bytes"))?,
            ),
            _ => return Err(PyValueError::new_err("unknown CCP method")),
        })
    }

    async fn execute(self, client: &CCPClient) -> automotive::Result<Value> {
        Ok(match self {
            Self::Connect(s) => {
                client.connect(s).await?;
                Value::None
            }
            Self::Disconnect(t) => {
                client.disconnect(t).await?;
                Value::None
            }
            Self::Version(a, b) => Value::Version(client.get_ccp_version(a, b).await?),
            Self::Id(id) => Value::Id(client.exchange_id(&id).await?),
            Self::Seed(r) => Value::Seed(client.get_seed(r).await?),
            Self::Unlock(k) => Value::Integer(client.unlock(&k).await?),
            Self::SetMta(a, e, m) => {
                client.set_mta(a, e, m).await?;
                Value::None
            }
            Self::Upload(n) => Value::Bytes(client.upload(n).await?),
            Self::ShortUpload(a, n, e) => Value::Bytes(client.short_upload(a, n, e).await?),
            Self::Download(d) => Value::Address(client.download(&d).await?),
            Self::Download6(d) => Value::Address(client.download6(&d).await?),
        })
    }
}

impl Value {
    fn python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = PyDict::new(py);
        macro_rules! fields { ($v:ident, $($f:ident),+) => { $(result.set_item(stringify!($f), $v.$f)?;)+ }; }
        match self {
            Self::None => return Ok(py.None()),
            Self::Bytes(v) => return Ok(PyBytes::new(py, &v).into_any().unbind()),
            Self::Integer(v) => return Ok(v.into_pyobject(py)?.into_any().unbind()),
            Self::Version(v) => {
                fields!(v, major, minor);
            }
            Self::Id(v) => {
                fields!(
                    v,
                    length,
                    data_type,
                    resource_availability,
                    resource_protection
                );
            }
            Self::Seed(v) => {
                fields!(v, protected);
                result.set_item("data", PyBytes::new(py, &v.data))?;
            }
            Self::Address(v) => {
                fields!(v, address, address_extension);
            }
        }
        Ok(result.into_any().unbind())
    }
}

#[pyclass(name = "_CCP", module = "automotive._native")]
pub struct Client {
    inner: CCPClient,
    parent: std::sync::Arc<State>,
}

#[pymethods]
impl Client {
    #[new]
    fn new(transport: &Transport, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: transport.state.attach(settings(config)?)?,
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
            return Err(PyConnectionError::new_err("CCP client is closed"));
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
    m.add_function(wrap_pyfunction!(validate_ccp, m)?)?;
    m.add_function(wrap_pyfunction!(ccp_defaults, m)?)?;
    m.add_function(wrap_pyfunction!(ccp_enum_values, m)?)?;
    Ok(())
}

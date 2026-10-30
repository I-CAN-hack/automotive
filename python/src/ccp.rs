use crate::{config, errors::convert, runtime::CanAdapter};
use automotive::{
    can::Identifier,
    protocols::ccp::{self, CCPClient, CCPConfig},
};
use pyo3::{
    exceptions::{PyConnectionError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};
use strum::IntoEnumIterator;
use tokio::sync::watch;

fn settings(obj: &Bound<'_, PyAny>) -> PyResult<CCPConfig> {
    let timeout = config::duration(obj.getattr("timeout")?.extract()?)?;
    let byte_order = match obj.getattr("byte_order")?.extract::<String>()?.as_str() {
        "little" => ccp::ByteOrder::LittleEndian,
        "big" => ccp::ByteOrder::BigEndian,
        _ => {
            return Err(PyValueError::new_err(
                "byte_order must be 'little' or 'big'",
            ))
        }
    };
    let config = CCPConfig {
        tx_id: config::identifier(&obj.getattr("tx_id")?)?,
        rx_id: config::identifier(&obj.getattr("rx_id")?)?,
        bus: obj.getattr("bus")?.extract()?,
        timeout,
        byte_order,
    };
    config.validate().map_err(convert)?;
    Ok(config)
}

#[pyfunction]
fn validate_ccp(config: &Bound<'_, PyAny>) -> PyResult<()> {
    settings(config).map(|_| ())
}

#[pyfunction]
fn ccp_defaults(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let config = CCPConfig::new(
        Identifier::standard(0).map_err(convert)?,
        Identifier::standard(1).map_err(convert)?,
    );
    result.set_item("bus", config.bus)?;
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
    parent: watch::Sender<bool>,
}

#[pymethods]
impl Client {
    #[new]
    fn new(adapter: &CanAdapter, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        if *adapter.closed.borrow() {
            return Err(PyConnectionError::new_err("CAN adapter is closed"));
        }
        Ok(Self {
            inner: CCPClient::new(adapter.inner.clone(), settings(config)?).map_err(convert)?,
            parent: adapter.closed.clone(),
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
        self.inner.is_closed() || *self.parent.borrow()
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    m.add_function(wrap_pyfunction!(validate_ccp, m)?)?;
    m.add_function(wrap_pyfunction!(ccp_defaults, m)?)?;
    m.add_function(wrap_pyfunction!(ccp_enum_values, m)?)?;
    Ok(())
}

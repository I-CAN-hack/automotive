use automotive::protocols::uds::{
    DTCReportNumberByStatusMask, RequestOutcome, SessionParameterRecord, UDSClient, UDSConfig,
};
use pyo3::{
    exceptions::{PyConnectionError, PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};
use std::sync::{atomic::Ordering, Arc};
use tokio::sync::oneshot;

use crate::{
    errors::convert,
    runtime::{State, Transport},
};

enum Command {
    Request(u8, Option<u8>, Option<Vec<u8>>),
    Session(u8),
    Reset(u8),
    Security(u8, Option<Vec<u8>>),
    TesterPresent,
    ReadDid(u16),
    WriteDid(u16, Vec<u8>),
    ReadMemory(Vec<u8>, Vec<u8>),
    ReadMemoryAlfid(u64, u64, usize, usize),
    WriteMemory(Vec<u8>, Vec<u8>, Vec<u8>),
    WriteMemoryAlfid(u64, u64, usize, usize, Vec<u8>),
    Dtc(u8),
    Routine(u8, u16, Option<Vec<u8>>),
    Download(u8, u8, Vec<u8>, Vec<u8>),
    DownloadAlfid(u8, u8, u64, u64, usize, usize),
    Upload(u8, u8, Vec<u8>, Vec<u8>),
    UploadAlfid(u8, u8, u64, u64, usize, usize),
    Transfer(u8, Option<Vec<u8>>),
    Exit(Option<Vec<u8>>),
}

enum Value {
    None,
    Bytes(Vec<u8>),
    Integer(usize),
    Session(SessionParameterRecord),
    Dtc(DTCReportNumberByStatusMask),
}

impl Value {
    fn optional_bytes(value: Option<Vec<u8>>) -> Self {
        value.map(Self::Bytes).unwrap_or(Self::None)
    }

    fn python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(match self {
            Self::None => py.None(),
            Self::Bytes(value) => PyBytes::new(py, &value).into_any().unbind(),
            Self::Integer(value) => value.into_pyobject(py)?.into_any().unbind(),
            Self::Session(record) => {
                let result = PyDict::new(py);
                result.set_item("p2_server_max", record.p2_server_max.as_secs_f64())?;
                result.set_item(
                    "p2_star_server_max",
                    record.p2_star_server_max.as_secs_f64(),
                )?;
                result.into_any().unbind()
            }
            Self::Dtc(record) => {
                let result = PyDict::new(py);
                result.set_item(
                    "dtc_status_availability_mask",
                    record.dtc_status_availability_mask,
                )?;
                result.set_item(
                    "dtc_format_identifier",
                    record.dtc_format_identifier.as_raw(),
                )?;
                result.set_item("dtc_count", record.dtc_count)?;
                result.into_any().unbind()
            }
        })
    }
}

impl Command {
    fn parse(method: &str, args: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let command = match method {
            "request" => {
                let (a, b, c) = args.extract()?;
                Self::Request(a, b, c)
            }
            "diagnostic_session_control" => Self::Session(args.extract::<(u8,)>()?.0),
            "ecu_reset" => Self::Reset(args.extract::<(u8,)>()?.0),
            "security_access" => {
                let (a, b) = args.extract()?;
                Self::Security(a, b)
            }
            "tester_present" => {
                if !args.is_empty() {
                    return Err(PyValueError::new_err("tester_present takes no arguments"));
                }
                Self::TesterPresent
            }
            "read_data_by_identifier" => Self::ReadDid(args.extract::<(u16,)>()?.0),
            "write_data_by_identifier" => {
                let (a, b) = args.extract()?;
                Self::WriteDid(a, b)
            }
            "read_memory_by_address" => {
                let (a, b) = args.extract()?;
                Self::ReadMemory(a, b)
            }
            "read_memory_by_address_alfid" => {
                let (a, b, c, d) = args.extract()?;
                Self::ReadMemoryAlfid(a, b, c, d)
            }
            "write_memory_by_address" => {
                let (a, b, c) = args.extract()?;
                Self::WriteMemory(a, b, c)
            }
            "write_memory_by_address_alfid" => {
                let (a, b, c, d, e) = args.extract()?;
                Self::WriteMemoryAlfid(a, b, c, d, e)
            }
            "read_dtc_information_number_of_dtc_by_status_mask" => {
                Self::Dtc(args.extract::<(u8,)>()?.0)
            }
            "routine_control" => {
                let (a, b, c) = args.extract()?;
                Self::Routine(a, b, c)
            }
            "request_download" => {
                let (a, b, c, d) = args.extract()?;
                Self::Download(a, b, c, d)
            }
            "request_download_alfid" => {
                let (a, b, c, d, e, f) = args.extract()?;
                Self::DownloadAlfid(a, b, c, d, e, f)
            }
            "request_upload" => {
                let (a, b, c, d) = args.extract()?;
                Self::Upload(a, b, c, d)
            }
            "request_upload_alfid" => {
                let (a, b, c, d, e, f) = args.extract()?;
                Self::UploadAlfid(a, b, c, d, e, f)
            }
            "transfer_data" => {
                let (a, b) = args.extract()?;
                Self::Transfer(a, b)
            }
            "request_transfer_exit" => Self::Exit(args.extract::<(Option<Vec<u8>>,)>()?.0),
            _ => return Err(PyValueError::new_err("unknown UDS service method")),
        };
        match &command {
            Self::Security(access, None) if access % 2 == 0 => {
                return Err(PyValueError::new_err("SendKey requires data"))
            }
            Self::ReadMemory(address, size)
            | Self::WriteMemory(address, size, _)
            | Self::Download(_, _, address, size)
            | Self::Upload(_, _, address, size)
                if !(1..=15).contains(&address.len()) || !(1..=15).contains(&size.len()) =>
            {
                return Err(PyValueError::new_err(
                    "memory address and size fields must contain 1 to 15 bytes",
                ));
            }
            _ => {}
        }
        Ok(command)
    }

    async fn execute(
        self,
        client: &UDSClient<'static, Transport>,
        suppressed: bool,
    ) -> automotive::Result<Value> {
        let view = client.suppress_positive_responses();
        macro_rules! call { ($method:ident($($args:expr),*)) => {
            if suppressed { view.$method($($args),*).await? } else { client.$method($($args),*).await? }
        }; }
        Ok(match self {
            Self::Request(a, b, c) => match call!(request(a, b, c.as_deref())) {
                RequestOutcome::PositiveResponse(data) => Value::Bytes(data),
                RequestOutcome::Suppressed => Value::None,
            },
            Self::Session(a) => call!(diagnostic_session_control(a))
                .map(Value::Session)
                .unwrap_or(Value::None),
            Self::Reset(a) => call!(ecu_reset(a))
                .map(|value| Value::Integer(value.into()))
                .unwrap_or(Value::None),
            Self::Security(a, b) => Value::optional_bytes(call!(security_access(a, b.as_deref()))),
            Self::TesterPresent => {
                call!(tester_present());
                Value::None
            }
            Self::ReadDid(a) => Value::Bytes(client.read_data_by_identifier(a).await?),
            Self::WriteDid(a, b) => {
                client.write_data_by_identifier(a, &b).await?;
                Value::None
            }
            Self::ReadMemory(a, b) => Value::Bytes(client.read_memory_by_address(&a, &b).await?),
            Self::ReadMemoryAlfid(a, b, c, d) => {
                Value::Bytes(client.read_memory_by_address_alfid(a, b, c, d).await?)
            }
            Self::WriteMemory(a, b, c) => {
                client.write_memory_by_address(&a, &b, &c).await?;
                Value::None
            }
            Self::WriteMemoryAlfid(a, b, c, d, e) => {
                client.write_memory_by_address_alfid(a, b, c, d, &e).await?;
                Value::None
            }
            Self::Dtc(a) => {
                let result = if suppressed {
                    view.read_dtc_information_number_of_dtc_by_status_mask(a)
                        .await?
                } else {
                    Some(
                        client
                            .read_dtc_information_number_of_dtc_by_status_mask(a)
                            .await?,
                    )
                };
                result.map(Value::Dtc).unwrap_or(Value::None)
            }
            Self::Routine(a, b, c) => {
                Value::optional_bytes(call!(routine_control(a, b, c.as_deref())))
            }
            Self::Download(a, b, c, d) => {
                Value::Integer(client.request_download(a, b, &c, &d).await?)
            }
            Self::DownloadAlfid(a, b, c, d, e, f) => {
                Value::Integer(client.request_download_alfid(a, b, c, d, e, f).await?)
            }
            Self::Upload(a, b, c, d) => Value::Integer(client.request_upload(a, b, &c, &d).await?),
            Self::UploadAlfid(a, b, c, d, e, f) => {
                Value::Integer(client.request_upload_alfid(a, b, c, d, e, f).await?)
            }
            Self::Transfer(a, b) => {
                Value::optional_bytes(client.transfer_data(a, b.as_deref()).await?)
            }
            Self::Exit(a) => {
                Value::optional_bytes(client.request_transfer_exit(a.as_deref()).await?)
            }
        })
    }
}

/// A persistent Rust diagnostic client, retaining learned session timing.
#[pyclass(name = "_UDS", module = "automotive._native")]
pub struct Client {
    inner: Arc<UDSClient<'static, Transport>>,
    state: Arc<State>,
}

impl Client {
    pub fn new(state: Arc<State>, config: UDSConfig) -> Self {
        Self {
            inner: Arc::new(UDSClient::with_owned_transport(state.inner.clone(), config)),
            state,
        }
    }
}

#[pymethods]
impl Client {
    fn call<'py>(
        &self,
        py: Python<'py>,
        method: &str,
        args: &Bound<'_, PyTuple>,
        suppressed: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let command = Command::parse(method, args)?;
        let state = self.state.clone();
        let client = self.inner.clone();
        if state.is_closed() {
            return Err(PyRuntimeError::new_err("transport is closed"));
        }
        if state.raw_receivers.load(Ordering::Acquire) != 0 {
            return Err(PyRuntimeError::new_err(
                "close raw transport receivers before using UDS on this transport",
            ));
        }
        let (mut result, response) = oneshot::channel();
        // The operation owns the gate after it starts. Dropping the Python future
        // cancels queued work, but an active exchange drains before the next one.
        pyo3_async_runtimes::tokio::get_runtime().spawn(async move {
            let _gate = tokio::select! {
                biased;
                _ = result.closed() => return,
                _ = state.wait_closed() => { let _ = result.send(Err(PyConnectionError::new_err("transport closed"))); return; },
                gate = state.gate.lock() => gate,
            };
            if result.is_closed() { return; }
            let value = tokio::select! {
                biased;
                _ = state.wait_closed() => Err(PyConnectionError::new_err("transport closed")),
                value = command.execute(&client, suppressed) => value.map_err(convert),
            };
            let _ = result.send(value);
        });
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let value = response
                .await
                .map_err(|_| PyRuntimeError::new_err("UDS worker stopped"))??;
            Python::attach(|py| value.python(py))
        })
    }
}

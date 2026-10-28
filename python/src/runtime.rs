use automotive::{
    can::AsyncCanAdapter, protocols::uds::UDSConfig, transports::isotp::IsoTP, StreamExt,
    Transport as _,
};
use futures::Stream;
use pyo3::{exceptions::PyRuntimeError, prelude::*, types::PyBytes};
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
};
use tokio::sync::{watch, Mutex};

use crate::{
    config::{self, AdapterConfig},
    errors::{convert, NotSupportedError},
};

type WireStream = Pin<Box<dyn Stream<Item = automotive::Result<WireValue>> + Send>>;
pub type PduStream = Pin<Box<dyn Stream<Item = automotive::Result<Vec<u8>>> + Send>>;

enum WireValue {
    Frame(automotive::can::Frame),
    Bytes(Vec<u8>),
}

pub enum Transport {
    Software(IsoTP<'static>),
    #[cfg(target_os = "windows")]
    Native(Arc<automotive::adapters::j2534::J2534NativeIsoTP>),
}

impl Transport {
    fn stream(&self) -> PduStream {
        match self {
            Self::Software(transport) => Box::pin(transport.clone().into_recv()),
            #[cfg(target_os = "windows")]
            Self::Native(transport) => Box::pin(transport.recv_owned()),
        }
    }
}

impl automotive::Transport for Transport {
    async fn send<'a>(&'a self, data: &'a [u8]) -> automotive::Result<()> {
        match self {
            Self::Software(transport) => transport.send(data).await,
            #[cfg(target_os = "windows")]
            Self::Native(transport) => transport.send(data).await,
        }
    }

    fn recv(&self) -> impl Stream<Item = automotive::Result<Vec<u8>>> + Unpin + '_ {
        self.stream()
    }
}

pub struct State {
    pub inner: Arc<Transport>,
    pub gate: Mutex<()>,
    pub raw_receivers: Arc<AtomicUsize>,
    closed: watch::Sender<bool>,
    parent: Option<watch::Sender<bool>>,
}

impl State {
    fn new(inner: Transport, parent: Option<watch::Sender<bool>>) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(inner),
            gate: Mutex::new(()),
            raw_receivers: Arc::new(AtomicUsize::new(0)),
            closed: watch::channel(false).0,
            parent,
        })
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow() || self.parent.as_ref().is_some_and(|parent| *parent.borrow())
    }

    pub async fn wait_closed(&self) {
        let mut own = self.closed.subscribe();
        let parent_wait = async {
            if let Some(parent) = &self.parent {
                let mut parent = parent.subscribe();
                let _ = parent.wait_for(|closed| *closed).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! { _ = own.wait_for(|closed| *closed) => {}, _ = parent_wait => {} }
    }
}

pub fn join_error(error: tokio::task::JoinError) -> PyErr {
    PyRuntimeError::new_err(format!("Rust worker failed: {error}"))
}

/// A hardware adapter owned by the Python façade.
#[pyclass(name = "_CanAdapter", module = "automotive._native")]
pub struct CanAdapter {
    pub inner: Arc<AsyncCanAdapter>,
    pub(crate) closed: watch::Sender<bool>,
}

#[pymethods]
impl CanAdapter {
    fn send<'py>(&self, py: Python<'py>, frame: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let frame = config::frame(frame)?;
        let adapter = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            adapter.send(&frame).await.map_err(convert)
        })
    }

    fn recv(&self) -> Receiver {
        Receiver::new(
            Box::pin(self.inner.recv().map(|frame| Ok(WireValue::Frame(frame)))),
            Some(self.closed.clone()),
            None,
            None,
            Some(self.inner.clone()),
        )
    }

    fn recv_for<'py>(&self, py: Python<'py>, duration: f64) -> PyResult<Bound<'py, PyAny>> {
        let duration = config::duration(duration)?;
        let adapter = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let frames = adapter.recv_for(duration).await;
            Python::attach(|py| {
                frames
                    .iter()
                    .map(|frame| config::frame_value(py, frame))
                    .collect::<PyResult<Vec<_>>>()
            })
        })
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.closed.send_replace(true);
        let adapter = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || adapter.close())
                .await
                .map_err(join_error)
        })
    }

    #[getter]
    fn closed(&self) -> bool {
        *self.closed.borrow()
    }
}

#[pyfunction]
fn open_adapter(py: Python<'_>, config: &Bound<'_, PyAny>) -> PyResult<CanAdapter> {
    let config = AdapterConfig::parse(config)?;
    let inner = py.detach(move || config.open()).map_err(convert)?;
    Ok(CanAdapter {
        inner: Arc::new(inner),
        closed: watch::channel(false).0,
    })
}

/// A complete-PDU transport, sharing ownership with its diagnostic clients.
#[pyclass(name = "_Transport", module = "automotive._native")]
pub struct PyTransport {
    pub state: Arc<State>,
}

#[pymethods]
impl PyTransport {
    #[new]
    fn new(adapter: &CanAdapter, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        if adapter.closed() {
            return Err(PyRuntimeError::new_err("adapter is closed"));
        }
        let config = config::isotp(config)?;
        Ok(Self {
            state: State::new(
                Transport::Software(IsoTP::new_owned(adapter.inner.clone(), config)),
                Some(adapter.closed.clone()),
            ),
        })
    }

    fn send<'py>(&self, py: Python<'py>, data: Vec<u8>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if state.is_closed() {
                return Err(PyRuntimeError::new_err("transport is closed"));
            }
            tokio::select! {
                result = state.inner.send(&data) => result.map_err(convert),
                _ = state.wait_closed() => Err(pyo3::exceptions::PyConnectionError::new_err("transport closed")),
            }
        })
    }

    fn recv(&self) -> PyResult<Receiver> {
        if self.state.is_closed() {
            return Err(PyRuntimeError::new_err("transport is closed"));
        }
        self.state.raw_receivers.fetch_add(1, Ordering::AcqRel);
        Ok(Receiver::new(
            Box::pin(
                self.state
                    .inner
                    .stream()
                    .map(|result| result.map(WireValue::Bytes)),
            ),
            None,
            Some(self.state.clone()),
            Some(self.state.raw_receivers.clone()),
            None,
        ))
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        state.closed.send_replace(true);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            #[cfg(target_os = "windows")]
            if let Transport::Native(native) = &*state.inner {
                let native = native.clone();
                tokio::task::spawn_blocking(move || native.close())
                    .await
                    .map_err(join_error)?;
            }
            let _ = state;
            Ok(())
        })
    }

    #[getter]
    fn closed(&self) -> bool {
        self.state.is_closed()
    }

    fn uds(&self, config: &Bound<'_, PyAny>) -> PyResult<crate::uds::Client> {
        if self.state.is_closed() {
            return Err(PyRuntimeError::new_err("transport is closed"));
        }
        let config: UDSConfig = config::uds_config(config)?;
        Ok(crate::uds::Client::new(self.state.clone(), config))
    }

    fn into_device<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        #[cfg(target_os = "windows")]
        if let Transport::Native(native) = &*self.state.inner {
            let native = native.clone();
            self.state.closed.send_replace(true);
            return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let device = tokio::task::spawn_blocking(move || native.take_device())
                    .await
                    .map_err(join_error)?
                    .ok_or_else(|| {
                        PyRuntimeError::new_err("device already closed or transferred")
                    })?;
                Ok(Device {
                    inner: StdMutex::new(Some(device)),
                })
            });
        }
        let _ = py;
        Err(NotSupportedError::new_err(
            "device reuse requires a native J2534 transport",
        ))
    }
}

struct ReceiveState {
    stream: Mutex<Option<WireStream>>,
    closed: watch::Sender<bool>,
    parent: Option<watch::Sender<bool>>,
    transport: Option<Arc<State>>,
    lease: StdMutex<Option<Arc<AtomicUsize>>>,
    // A receiver can outlive its Python CanAdapter object without closing hardware.
    _adapter: Option<Arc<AsyncCanAdapter>>,
}

impl ReceiveState {
    fn release(&self) {
        if let Some(count) = self
            .lease
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            count.fetch_sub(1, Ordering::AcqRel);
        }
    }

    async fn wait_closed(&self) {
        let mut own = self.closed.subscribe();
        let parent = async {
            if let Some(parent) = &self.parent {
                let mut parent = parent.subscribe();
                let _ = parent.wait_for(|closed| *closed).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        let transport = async {
            if let Some(transport) = &self.transport {
                transport.wait_closed().await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! { _ = own.wait_for(|closed| *closed) => {}, _ = parent => {}, _ = transport => {} }
    }
}

impl Drop for ReceiveState {
    fn drop(&mut self) {
        self.release();
    }
}

/// A receive subscription created before the first call to next.
#[pyclass(name = "_Receiver", module = "automotive._native")]
pub struct Receiver {
    state: Arc<ReceiveState>,
}

impl Receiver {
    fn new(
        stream: WireStream,
        parent: Option<watch::Sender<bool>>,
        transport: Option<Arc<State>>,
        lease: Option<Arc<AtomicUsize>>,
        adapter: Option<Arc<AsyncCanAdapter>>,
    ) -> Self {
        Self {
            state: Arc::new(ReceiveState {
                stream: Mutex::new(Some(stream)),
                closed: watch::channel(false).0,
                parent,
                transport,
                lease: StdMutex::new(lease),
                _adapter: adapter,
            }),
        }
    }
}

#[pymethods]
impl Receiver {
    fn next<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut stream = state.stream.lock().await;
            let Some(active) = stream.as_mut() else {
                return Ok(None);
            };
            let result = tokio::select! {
                biased;
                _ = state.wait_closed() => None,
                result = active.next() => result,
            };
            match result {
                Some(Ok(value)) => Python::attach(|py| {
                    Ok(Some(match value {
                        WireValue::Frame(frame) => config::frame_value(py, &frame)?,
                        WireValue::Bytes(bytes) => PyBytes::new(py, &bytes).into_any().unbind(),
                    }))
                }),
                Some(Err(error)) => Err(convert(error)),
                None => {
                    stream.take();
                    state.release();
                    Ok(None)
                }
            }
        })
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let state = self.state.clone();
        state.closed.send_replace(true);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            state.stream.lock().await.take();
            state.release();
            Ok(())
        })
    }
}

/// A reusable J2534 device. Channel creation consumes this handle.
#[pyclass(name = "_Device", module = "automotive._native")]
pub struct Device {
    #[cfg(target_os = "windows")]
    inner: StdMutex<Option<automotive::adapters::j2534::J2534Device>>,
}

#[pymethods]
impl Device {
    #[getter]
    fn closed(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.inner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_none()
        }
        #[cfg(not(target_os = "windows"))]
        {
            true
        }
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        #[cfg(target_os = "windows")]
        let device = self
            .inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            #[cfg(target_os = "windows")]
            tokio::task::spawn_blocking(move || drop(device))
                .await
                .map_err(join_error)?;
            Ok(())
        })
    }
}

#[pyfunction]
#[pyo3(signature = (adapter_config, isotp_config, device=None))]
fn open_native_isotp<'py>(
    py: Python<'py>,
    adapter_config: &Bound<'_, PyAny>,
    isotp_config: &Bound<'_, PyAny>,
    device: Option<&Device>,
) -> PyResult<Bound<'py, PyAny>> {
    #[cfg(target_os = "windows")]
    {
        use automotive::adapters::j2534::J2534NativeIsoTP;
        use config::field;
        let bitrate = config::bitrate("j2534", &adapter_config.getattr("bitrate")?)?;
        let path = field!(adapter_config, dll_path, Option<String>);
        let config = config::isotp(isotp_config)?;
        let device = device
            .map(|device| {
                device
                    .inner
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .ok_or_else(|| PyRuntimeError::new_err("device already consumed or closed"))
            })
            .transpose()?;
        return pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let native = tokio::task::spawn_blocking(move || match device {
                Some(device) => J2534NativeIsoTP::new_on_device(device, bitrate, config),
                None => J2534NativeIsoTP::new(path.as_deref(), bitrate, config),
            })
            .await
            .map_err(join_error)?
            .map_err(convert)?;
            Ok(PyTransport {
                state: State::new(Transport::Native(Arc::new(native)), None),
            })
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (py, adapter_config, isotp_config, device);
        Err(NotSupportedError::new_err(
            "native J2534 ISO-TP requires Windows",
        ))
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<CanAdapter>()?;
    m.add_class::<PyTransport>()?;
    m.add_class::<Receiver>()?;
    m.add_class::<Device>()?;
    m.add_function(wrap_pyfunction!(open_adapter, m)?)?;
    m.add_function(wrap_pyfunction!(open_native_isotp, m)?)?;
    Ok(())
}

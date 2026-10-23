use automotive::{
    can::{
        bitrate::{BitrateBuilder, BitrateConfig},
        CanAdapter, ErrorFlag, Frame, FrameContent, Identifier,
    },
    protocols::uds::{self, UDSConfig},
    transports::isotp::IsoTPConfig,
};
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::time::Duration;
use strum::IntoEnumIterator;

use crate::errors::{convert, NotSupportedError};

macro_rules! field {
    ($o:expr, $name:ident, $ty:ty) => {
        $o.getattr(stringify!($name))?.extract::<$ty>()?
    };
}
#[cfg(target_os = "windows")]
pub(crate) use field;

pub fn duration(seconds: f64) -> PyResult<Duration> {
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| PyValueError::new_err("durations must be finite, nonnegative seconds"))
}

pub fn identifier(obj: &Bound<'_, PyAny>) -> PyResult<Identifier> {
    if obj.hasattr("is_extended")? {
        let raw = field!(obj, value, u32);
        if field!(obj, is_extended, bool) {
            Identifier::extended(raw)
        } else {
            Identifier::standard(raw)
        }
    } else {
        Identifier::try_from(obj.extract::<u32>()?)
    }
    .map_err(convert)
}

#[pyfunction]
fn validate_identifier(value: u32, extended: bool) -> PyResult<()> {
    if extended {
        Identifier::extended(value)
    } else {
        Identifier::standard(value)
    }
    .map(|_| ())
    .map_err(convert)
}

pub fn frame(obj: &Bound<'_, PyAny>) -> PyResult<Frame> {
    let bus = field!(obj, bus, u8);
    let kind = field!(obj, kind, String);
    if kind != "data"
        && (!field!(obj, data, Vec<u8>).is_empty()
            || field!(obj, fd, Option<bool>).unwrap_or(false)
            || field!(obj, brs, bool)
            || field!(obj, esi, bool))
    {
        return Err(PyValueError::new_err(
            "remote/error frames cannot contain data or FD flags",
        ));
    }
    if kind == "error" && (!obj.getattr("id")?.is_none() || !obj.getattr("dlc")?.is_none()) {
        return Err(PyValueError::new_err(
            "error frames cannot contain an identifier or DLC",
        ));
    }
    if kind != "error" && !obj.getattr("error_flag")?.is_none() {
        return Err(PyValueError::new_err(
            "only error frames have an error flag",
        ));
    }
    let mut frame = match kind.as_str() {
        "remote" => Frame::remote(bus, identifier(&obj.getattr("id")?)?, field!(obj, dlc, u8)),
        "error" => {
            let flag = match field!(obj, error_flag, u8) {
                0 => ErrorFlag::Active,
                1 => ErrorFlag::Passive,
                _ => return Err(PyValueError::new_err("invalid error flag")),
            };
            Ok(Frame::error(bus, flag))
        }
        "data" => {
            let id = identifier(&obj.getattr("id")?)?;
            let data = field!(obj, data, Vec<u8>);
            let fd = field!(obj, fd, Option<bool>).unwrap_or(data.len() > 8);
            Frame::builder(id)
                .bus(bus)
                .data(&data)
                .fd(fd)
                .brs(field!(obj, brs, bool))
                .esi(field!(obj, esi, bool))
                .build()
        }
        _ => return Err(PyValueError::new_err("kind must be data, remote, or error")),
    }
    .map_err(convert)?;
    if kind == "data" && field!(obj, dlc, Option<u8>).is_some_and(|dlc| Some(dlc) != frame.dlc()) {
        return Err(PyValueError::new_err(
            "DLC does not match the data payload length",
        ));
    }
    frame.loopback = field!(obj, loopback, bool);
    Ok(frame)
}

pub fn frame_value(py: Python<'_>, frame: &Frame) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("id", frame.id().map(|id| id.as_raw()))?;
    dict.set_item("extended", frame.id().is_some_and(|id| id.is_extended()))?;
    dict.set_item("data", PyBytes::new(py, frame.data()))?;
    dict.set_item("bus", frame.bus)?;
    dict.set_item("loopback", frame.loopback)?;
    dict.set_item("fd", frame.is_fd())?;
    dict.set_item("dlc", frame.dlc())?;
    let (kind, brs, esi, error_flag) = match frame.content() {
        FrameContent::Classic { .. } => ("data", false, false, None),
        FrameContent::Fd { brs, esi, .. } => ("data", *brs, *esi, None),
        FrameContent::Remote { .. } => ("remote", false, false, None),
        FrameContent::Error(flag) => (
            "error",
            false,
            false,
            Some(if *flag == ErrorFlag::Active { 0 } else { 1 }),
        ),
    };
    dict.set_item("kind", kind)?;
    dict.set_item("brs", brs)?;
    dict.set_item("esi", esi)?;
    dict.set_item("error_flag", error_flag)?;
    Ok(dict.into_any().unbind())
}

#[pyfunction]
fn validate_frame(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    frame_value(py, &frame(obj)?)
}

pub fn isotp(obj: &Bound<'_, PyAny>) -> PyResult<IsoTPConfig> {
    let mut builder = IsoTPConfig::builder(identifier(&obj.getattr("tx_id")?)?)
        .bus(field!(obj, bus, u8))
        .fd(field!(obj, fd, bool))
        .timeout(duration(field!(obj, timeout, f64))?)
        .block_size(field!(obj, block_size, u8))
        .receive_separation_time_min(duration(field!(obj, receive_separation_time_min, f64))?);
    let rx_id = obj.getattr("rx_id")?;
    let offset = field!(obj, rx_id_offset, Option<u32>);
    if !rx_id.is_none() && offset.is_some() {
        return Err(PyValueError::new_err(
            "rx_id and rx_id_offset are mutually exclusive",
        ));
    }
    if !rx_id.is_none() {
        builder = builder.rx_id(identifier(&rx_id)?);
    }
    if let Some(offset) = offset {
        builder = builder.rx_id_offset(offset);
    }
    builder = match field!(obj, padding, Option<u8>) {
        Some(value) => builder.padding(value),
        None => builder.without_padding(),
    };
    if let Some(value) = field!(obj, separation_time_min, Option<f64>) {
        builder = builder.separation_time_min(duration(value)?);
    }
    if let Some(value) = field!(obj, ext_address, Option<u8>) {
        builder = builder.ext_address(value);
    }
    if let Some(value) = field!(obj, rx_ext_address, Option<u8>) {
        builder = builder.rx_ext_address(value);
    }
    if let Some(value) = field!(obj, max_dlen, Option<usize>) {
        builder = builder.max_dlen(value);
    }
    builder.build().map_err(convert)
}

#[pyfunction]
fn validate_isotp(obj: &Bound<'_, PyAny>) -> PyResult<(u32, bool)> {
    let config = isotp(obj)?;
    Ok((config.rx_id.as_raw(), config.rx_id.is_extended()))
}

pub fn uds_config(obj: &Bound<'_, PyAny>) -> PyResult<UDSConfig> {
    let mut builder = UDSConfig::builder()
        .p2_timeout(duration(field!(obj, p2_timeout, f64))?)
        .p2_star_timeout(duration(field!(obj, p2_star_timeout, f64))?)
        .strict(field!(obj, strict, bool))
        .use_server_timing(field!(obj, use_server_timing, bool));
    if let Some(value) = field!(obj, request_timeout, Option<f64>) {
        builder = builder.request_timeout(duration(value)?);
    }
    Ok(builder.build())
}

#[pyfunction]
fn validate_uds(obj: &Bound<'_, PyAny>) -> PyResult<()> {
    uds_config(obj).map(|_| ())
}

fn timing<T: CanAdapter>(settings: &Bound<'_, PyAny>) -> PyResult<BitrateConfig> {
    let mut builder = BitrateBuilder::new::<T>();
    if settings.is_none() {
        builder = builder.bitrate(500_000);
    } else {
        macro_rules! set { ($($name:ident : $ty:ty),*) => { $(if let Some(value) = field!(settings, $name, Option<$ty>) { builder = builder.$name(value); })* }; }
        set!(bitrate: u32, sample_point: f64, brp: u32, tseg1: u32, tseg2: u32, sjw: u32,
            data_bitrate: u32, data_sample_point: f64, data_sjw: u32, max_bitrate_error: u32);
    }
    builder.build().map_err(|error| convert(error.into()))
}

pub fn bitrate(backend: &str, settings: &Bound<'_, PyAny>) -> PyResult<BitrateConfig> {
    match backend {
        "panda" if settings.is_none() => automotive::adapters::panda::PandaConfig::builder()
            .build()
            .map(|config| config.bitrate)
            .map_err(|error| convert(error.into())),
        "peak" if settings.is_none() => automotive::adapters::peak::PeakConfig::builder()
            .build()
            .map(|config| config.bitrate)
            .map_err(|error| convert(error.into())),
        "panda" => timing::<automotive::adapters::panda::PandaAdapter>(settings),
        "peak" => timing::<automotive::adapters::peak::PeakAdapter>(settings),
        #[cfg(target_os = "windows")]
        "vector" => timing::<automotive::adapters::vector::VectorAdapter>(settings),
        #[cfg(target_os = "windows")]
        "j2534" => timing::<automotive::adapters::j2534::J2534Adapter>(settings),
        _ => Err(NotSupportedError::new_err(format!(
            "{backend} bitrate calculation is unavailable on this build"
        ))),
    }
}

fn bitrate_value(py: Python<'_>, bitrate: BitrateConfig) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (name, phase) in [("nominal", Some(bitrate.nominal)), ("data", bitrate.data)] {
        if let Some(phase) = phase {
            let values = PyDict::new(py);
            values.set_item("brp", phase.brp)?;
            values.set_item("tseg1", phase.tseg1)?;
            values.set_item("tseg2", phase.tseg2)?;
            values.set_item("sjw", phase.sjw)?;
            values.set_item("bitrate", phase.bitrate)?;
            values.set_item("sample_point", phase.sample_point)?;
            dict.set_item(name, values)?;
        } else {
            dict.set_item(name, py.None())?;
        }
    }
    Ok(dict.into_any().unbind())
}

#[pyfunction]
fn resolve_bitrate(
    py: Python<'_>,
    backend: &str,
    settings: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    bitrate_value(py, bitrate(backend, settings)?)
}

pub enum AdapterConfig {
    Auto,
    #[cfg(target_os = "linux")]
    Socket(automotive::adapters::socketcan::SocketCanConfig),
    Panda(automotive::adapters::panda::PandaConfig),
    Peak(automotive::adapters::peak::PeakConfig),
    #[cfg(target_os = "windows")]
    Vector(automotive::adapters::vector::VectorConfig),
    #[cfg(target_os = "windows")]
    J2534(Option<String>, BitrateConfig),
}

impl AdapterConfig {
    pub fn parse(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if obj.is_none() {
            return Ok(Self::Auto);
        }
        let backend = field!(obj, backend, String);
        match backend.as_str() {
            #[cfg(target_os = "linux")]
            "socketcan" => Ok(Self::Socket(
                automotive::adapters::socketcan::SocketCanConfig::new(field!(
                    obj, interface, String
                )),
            )),
            "panda" => Ok(Self::Panda(automotive::adapters::panda::PandaConfig::new(
                bitrate("panda", &obj.getattr("bitrate")?)?,
            ))),
            "peak" => {
                let mut config = automotive::adapters::peak::PeakConfig::new(bitrate(
                    "peak",
                    &obj.getattr("bitrate")?,
                )?);
                config.tx_timeout = field!(obj, tx_timeout, Option<f64>)
                    .map(duration)
                    .transpose()?;
                if config.tx_timeout.is_some_and(|value| value.is_zero()) {
                    return Err(PyValueError::new_err(
                        "tx_timeout must be greater than zero",
                    ));
                }
                Ok(Self::Peak(config))
            }
            #[cfg(target_os = "windows")]
            "vector" => {
                use automotive::adapters::vector::{VectorAccess, VectorConfig};
                let mut config = VectorConfig::new(field!(obj, channel, usize));
                let settings = obj.getattr("bitrate")?;
                if !settings.is_none() {
                    config.access = VectorAccess::Init {
                        bitrate: bitrate("vector", &settings)?,
                    };
                }
                config.tx_timeout = field!(obj, tx_timeout, Option<f64>)
                    .map(duration)
                    .transpose()?;
                if config.tx_timeout.is_some_and(|value| value.is_zero()) {
                    return Err(PyValueError::new_err(
                        "tx_timeout must be greater than zero",
                    ));
                }
                Ok(Self::Vector(config))
            }
            #[cfg(target_os = "windows")]
            "j2534" => Ok(Self::J2534(
                field!(obj, dll_path, Option<String>),
                bitrate("j2534", &obj.getattr("bitrate")?)?,
            )),
            _ => Err(NotSupportedError::new_err(format!(
                "{backend} is unavailable on this platform/build"
            ))),
        }
    }

    pub fn open(self) -> automotive::Result<automotive::can::AsyncCanAdapter> {
        match self {
            Self::Auto => automotive::adapters::get_adapter(),
            #[cfg(target_os = "linux")]
            Self::Socket(config) => {
                automotive::adapters::socketcan::SocketCanAdapter::new_async(config)
            }
            Self::Panda(config) => automotive::adapters::panda::PandaAdapter::new_async(config),
            Self::Peak(config) => automotive::adapters::peak::PeakAdapter::new_async(config),
            #[cfg(target_os = "windows")]
            Self::Vector(config) => automotive::adapters::vector::VectorAdapter::new_async(config),
            #[cfg(target_os = "windows")]
            Self::J2534(path, bitrate) => {
                automotive::adapters::j2534::J2534Adapter::new_async(path.as_deref(), bitrate)
            }
        }
    }
}

#[pyfunction]
fn available_adapters() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut names = Vec::new();
    #[cfg(target_os = "linux")]
    names.push("socketcan");
    names.push("panda");
    names.push("peak");
    #[cfg(target_os = "windows")]
    names.push("vector");
    #[cfg(target_os = "windows")]
    names.push("j2534");
    names
}

#[pyfunction]
fn validate_adapter(config: &Bound<'_, PyAny>) -> PyResult<()> {
    AdapterConfig::parse(config).map(|_| ())
}

#[pyfunction]
fn defaults(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let values = PyDict::new(py);
    let uds = UDSConfig::default();
    values.set_item("p2_timeout", uds.p2_timeout.as_secs_f64())?;
    values.set_item("p2_star_timeout", uds.p2_star_timeout.as_secs_f64())?;
    values.set_item(
        "request_timeout",
        uds.request_timeout.map(|value| value.as_secs_f64()),
    )?;
    values.set_item("strict", uds.strict)?;
    values.set_item("use_server_timing", uds.use_server_timing)?;
    let config = IsoTPConfig::builder(Identifier::standard(0).map_err(convert)?)
        .build()
        .map_err(convert)?;
    values.set_item("padding", config.padding)?;
    values.set_item("timeout", config.timeout.as_secs_f64())?;
    values.set_item("bus", config.bus)?;
    values.set_item("fd", config.fd)?;
    values.set_item("block_size", config.block_size)?;
    values.set_item(
        "receive_separation_time_min",
        config.receive_separation_time_min.as_secs_f64(),
    )?;
    values.set_item(
        "tx_timeout",
        automotive::adapters::DEFAULT_TX_TIMEOUT.as_secs_f64(),
    )?;
    Ok(values.into_any().unbind())
}

#[pyfunction]
fn enum_values(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let enums = PyDict::new(py);
    macro_rules! add { ($($name:ident),*) => { $(
        let values = PyDict::new(py);
        for value in uds::$name::iter() { values.set_item(format!("{value:?}"), value.as_raw())?; }
        enums.set_item(stringify!($name), values)?;
    )* }; }
    add!(
        ServiceIdentifier,
        DataIdentifier,
        SessionType,
        ResetType,
        SecurityAccessType,
        RoutineControlType,
        ReportType,
        RoutineIdentifier,
        NegativeResponseCode,
        DTCFormatIdentifier
    );
    Ok(enums.into_any().unbind())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate_identifier, m)?)?;
    m.add_function(wrap_pyfunction!(validate_frame, m)?)?;
    m.add_function(wrap_pyfunction!(validate_isotp, m)?)?;
    m.add_function(wrap_pyfunction!(validate_uds, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_bitrate, m)?)?;
    m.add_function(wrap_pyfunction!(available_adapters, m)?)?;
    m.add_function(wrap_pyfunction!(enum_values, m)?)?;
    m.add_function(wrap_pyfunction!(validate_adapter, m)?)?;
    m.add_function(wrap_pyfunction!(defaults, m)?)?;
    Ok(())
}

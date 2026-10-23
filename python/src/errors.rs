use automotive::{protocols::uds, transports::isotp, Error};
use pyo3::{create_exception, exceptions::*, prelude::*};

create_exception!(
    automotive,
    AutomotiveError,
    PyException,
    "Base class for automotive runtime failures."
);
create_exception!(
    automotive,
    AdapterError,
    AutomotiveError,
    "A hardware adapter operation failed."
);
create_exception!(
    automotive,
    NotSupportedError,
    AutomotiveError,
    "The backend or requested operation is unavailable."
);
create_exception!(
    automotive,
    TransmissionCancelledError,
    AutomotiveError,
    "A transmit queue clear invalidated this send."
);
create_exception!(
    automotive,
    IsoTPError,
    AutomotiveError,
    "An ISO-TP exchange failed."
);
create_exception!(
    automotive,
    UDSError,
    AutomotiveError,
    "A UDS exchange failed."
);
create_exception!(
    automotive,
    NegativeResponseError,
    UDSError,
    "The ECU rejected a request; code contains its raw NRC."
);
create_exception!(
    automotive,
    ProtocolDeviationError,
    UDSError,
    "A response violated strict validation; response contains the complete PDU."
);

pub fn convert(error: Error) -> PyErr {
    let message = error.to_string();
    match error {
        Error::Timeout | Error::IdleTimeout => PyTimeoutError::new_err(message),
        Error::Disconnected => PyConnectionError::new_err(message),
        Error::NotFound => AdapterError::new_err("No supported adapter found"),
        Error::NotSupported => NotSupportedError::new_err(message),
        Error::Cancelled => TransmissionCancelledError::new_err(message),
        Error::InvalidBitrate(_) | Error::InvalidAdapterConfig(_) | Error::MalformedFrame => {
            PyValueError::new_err(message)
        }
        Error::IoError(error) => match error.raw_os_error() {
            Some(code) => PyOSError::new_err((code, message)),
            None => PyOSError::new_err(message),
        },
        Error::IsoTPError(
            isotp::Error::InvalidConfig(_)
            | isotp::Error::EmptyPayload
            | isotp::Error::DataTooLarge,
        ) => PyValueError::new_err(message),
        Error::IsoTPError(_) => IsoTPError::new_err(message),
        Error::UDSError(uds::Error::NegativeResponse(code)) => Python::attach(|py| {
            let error = NegativeResponseError::new_err(message);
            let _ = error.value(py).setattr("code", code.as_raw());
            error
        }),
        Error::UDSError(uds::Error::ProtocolDeviation {
            deviation,
            response,
        }) => Python::attach(|py| {
            let error = ProtocolDeviationError::new_err(message);
            let value = error.value(py);
            let _ = value.setattr("deviation", format!("{deviation:?}"));
            let _ = value.setattr("response", pyo3::types::PyBytes::new(py, &response));
            error
        }),
        Error::UDSError(
            uds::Error::InvalidCompressionMethod(_)
            | uds::Error::InvalidEncryptionMethod(_)
            | uds::Error::InvalidMemoryValueWidth(_)
            | uds::Error::MemoryValueOverflow { .. },
        ) => PyValueError::new_err(message),
        Error::UDSError(_) => UDSError::new_err(message),
        _ => AdapterError::new_err(message),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    macro_rules! add { ($($name:ident),*) => { $(m.add(stringify!($name), m.py().get_type::<$name>())?;)* }; }
    add!(
        AutomotiveError,
        AdapterError,
        NotSupportedError,
        TransmissionCancelledError,
        IsoTPError,
        UDSError,
        NegativeResponseError,
        ProtocolDeviationError
    );
    Ok(())
}

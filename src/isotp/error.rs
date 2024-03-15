//! Error types for the ISO-TP protocol.

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("Data Too Large")]
    DataTooLarge,
    #[error("Flow Control")]
    FlowControl,
    #[error("Out Of Order")]
    OutOfOrder,
    #[error("Unknown Frame Type")]
    UnknownFrameType,
    #[error("Malformed Frame")]
    MalformedFrame,
}

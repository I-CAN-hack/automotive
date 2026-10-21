//! Error types for the ISO-TP protocol.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    #[error("Invalid ISO-TP configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("Data Too Large")]
    DataTooLarge,
    #[error("Flow Control")]
    FlowControl,
    #[error("Overflow")]
    Overflow,
    #[error("Out Of Order")]
    OutOfOrder,
    #[error("A new ISO-TP message interrupted the current reception")]
    UnexpectedPdu,
    #[error("Unknown Frame Type")]
    UnknownFrameType,
    #[error("Malformed Frame")]
    MalformedFrame,
}

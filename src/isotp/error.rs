//! Error types for the ISO-TP protocol.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    #[error("Data Too Large")]
    DataTooLarge,
    #[error("Flow Control")]
    FlowControl,
    #[error("Overflow")]
    Overflow,
    #[error("Out Of Order")]
    OutOfOrder,
    #[error("Unknown Frame Type")]
    UnknownFrameType,
    #[error("Malformed Frame")]
    MalformedFrame,
    #[error("Too many WAIT Flow Control, N_WFTmax exeeded")]
    TooManyFCWait,
}

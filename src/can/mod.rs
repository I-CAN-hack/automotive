//! Generic CAN types and traits

pub mod adapter;
pub mod async_can;
pub mod bitrate;

use std::collections::VecDeque;

pub use adapter::get_adapter;
pub use async_can::AsyncCanAdapter;

pub static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

mod frame;
mod identifier;

pub use frame::{ClassicFrame, ErrorFlag, FdFrame, Frame, FrameBuilder, FrameContent, RemoteFrame};
pub use identifier::{ExtendedId, Identifier, StandardId};

/// Trait for a Blocking CAN Adapter
pub trait CanAdapter {
    fn send(&mut self, frames: &mut VecDeque<crate::can::Frame>) -> crate::Result<()>;
    fn recv(&mut self) -> crate::Result<Vec<Frame>>;

    /// Whether this backend can cancel all pending TX before accepting new TX.
    /// Only backends with this capability expose a TX timeout in their config.
    fn supports_tx_timeout() -> bool
    where
        Self: Sized,
    {
        false
    }

    /// Queue/confirmation deadline from the adapter's configuration.
    fn tx_timeout(&self) -> Option<std::time::Duration> {
        None
    }

    /// Cancel every pending hardware transmission in this adapter session.
    /// A backend may defer the clear until the next real frame (Vector's
    /// HIGHPRIO replacement), but must perform it before submitting that frame.
    /// Old echoes may still arrive during the startup/recovery grace period,
    /// before the first confirmation for the new session.
    fn cancel_pending_tx(&mut self) -> crate::Result<()> {
        Err(crate::Error::NotSupported)
    }

    /// Maximum number of frames the adapter can have in flight at once, i.e.
    /// transmitted but not yet acknowledged back via a loopback frame.
    ///
    /// Adapters backed by hardware with a finite buffer should return `Some(n)`
    /// so the [`crate::can::AsyncCanAdapter`] throttles transmission and does not
    /// overrun the buffer. `None` uses the async wrapper's software window.
    fn buffer_size(&self) -> Option<usize> {
        None
    }

    /// Adapter timing constants for bitrate configuration helpers.
    ///
    /// Adapters that support [`crate::can::bitrate::BitrateBuilder`] should override this.
    /// The default implementation panics to preserve backward compatibility for existing
    /// downstream trait implementations that do not provide timing metadata.
    fn timing_const() -> crate::can::bitrate::AdapterTimingConst
    where
        Self: Sized,
    {
        panic!(
            "CanAdapter::timing_const is not implemented for adapter type {}",
            std::any::type_name::<Self>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_compare() {
        assert!(Identifier::standard(0x123).unwrap() < Identifier::standard(0x124).unwrap());
        assert!(Identifier::standard(0x7ff).unwrap() > Identifier::standard(0x100).unwrap());

        // Preserve the existing ordering of standard and extended formats.
        assert!(Identifier::extended(0x1).unwrap() > Identifier::standard(0x100).unwrap());
    }
}

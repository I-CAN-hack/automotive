//! Generic CAN types and traits

mod async_can;
pub mod bitrate;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub use async_can::AsyncCanAdapter;

pub static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

mod frame;
mod identifier;

pub use frame::{ErrorFlag, Frame, FrameBuilder, FrameContent};
pub use identifier::{ExtendedId, Identifier, StandardId};

/// A lazily consumed, time-limited receive iterator.
///
/// The duration starts on the first call to [`Iterator::next`]. Receive errors
/// are yielded as items because they can occur after the iterator is created.
pub struct RecvFor<'a, T: CanAdapter + ?Sized> {
    adapter: &'a mut T,
    duration: Duration,
    started: Option<Instant>,
    pending: VecDeque<Frame>,
    finished: bool,
}

impl<'a, T: CanAdapter + ?Sized> RecvFor<'a, T> {
    fn new(adapter: &'a mut T, duration: Duration) -> Self {
        Self {
            adapter,
            duration,
            started: None,
            pending: VecDeque::new(),
            finished: false,
        }
    }
}

impl<T: CanAdapter + ?Sized> Iterator for RecvFor<'_, T> {
    type Item = crate::Result<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let started = *self.started.get_or_insert_with(Instant::now);

        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Some(Ok(frame));
            }
            if started.elapsed() >= self.duration {
                self.finished = true;
                return None;
            }

            match self.adapter.recv() {
                Ok(frames) => self.pending.extend(frames),
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            }

            if self.pending.is_empty() {
                let remaining = self.duration.saturating_sub(started.elapsed());
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
        }
    }
}

/// Trait for a Blocking CAN Adapter
pub trait CanAdapter {
    fn send(&mut self, frames: &mut VecDeque<crate::can::Frame>) -> crate::Result<()>;
    fn recv(&mut self) -> crate::Result<Vec<Frame>>;

    /// Receive frames, including transmit echoes, for the given duration.
    ///
    /// The call itself does not receive or block; the duration starts when the
    /// returned iterator is first advanced. The backend's receive timeout can
    /// extend iteration past the requested duration.
    fn recv_for(&mut self, duration: Duration) -> RecvFor<'_, Self> {
        RecvFor::new(self, duration)
    }

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

    /// Default bitrate-switching setting for outgoing CAN FD frames.
    ///
    /// Adapters with known timing return whether a data bitrate is configured.
    /// Unknown or externally managed timing defaults to enabled. ISO-TP uses
    /// this setting unless its configuration explicitly overrides BRS.
    fn default_brs(&self) -> bool {
        true
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

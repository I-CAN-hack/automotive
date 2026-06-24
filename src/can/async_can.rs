//! Async wrapper for Adapters implementing the [`CanAdapter`] trait.

use std::collections::{HashMap, VecDeque};

use crate::can::CanAdapter;
use crate::can::Frame;
use crate::can::Identifier;
use crate::Stream;
use async_stream::stream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::debug;

const CAN_TX_BUFFER_SIZE: usize = 128;
// Must be large enough to absorb all CAN frames generated during a
// multi-frame ISO-TP transfer without the recv subscriber being polled.
// A 4 KiB UDS payload at 7 bytes/CF ≈ 585 CFs; each produces a loopback
// echo plus the real frame.  Back-to-back transfers or mixed TX/RX traffic
// can double that.  8192 gives ample headroom.
const CAN_RX_BUFFER_SIZE: usize = 8192;
const DEBUG: bool = false;

type BusIdentifier = (u8, Identifier);
type FrameCallback = (Frame, oneshot::Sender<()>);

/// Async processing loop driving a [`CanAdapter`]. Shared by the native driver (dedicated
/// thread running `block_on`) and the wasm driver (`spawn_local`). The only blocking or
/// awaiting points are the adapter's `recv`/`send`; all channel operations are synchronous
/// (`try_recv`/`send`).
async fn process<T: CanAdapter>(
    mut adapter: T,
    mut shutdown_receiver: oneshot::Receiver<()>,
    rx_sender: broadcast::Sender<Frame>,
    mut tx_receiver: mpsc::Receiver<(Frame, oneshot::Sender<()>)>,
) {
    let mut buffer: VecDeque<Frame> = VecDeque::new();
    let mut callbacks: HashMap<BusIdentifier, VecDeque<FrameCallback>> = HashMap::new();

    // Optional hardware flow-control limit. When set, we keep the number of
    // frames in flight (sent but not yet acknowledged via loopback) plus those
    // queued to send below this limit so the adapter's buffer is not overrun.
    let buffer_size = adapter.buffer_size();
    let mut in_flight: usize = 0;

    while shutdown_receiver.try_recv().is_err() {
        let frames: Vec<Frame> = match adapter.recv().await {
            Ok(f) => f,
            Err(e) => {
                debug!("Adapter recv error: {:?} — shutting down process loop", e);
                break;
            }
        };

        for frame in frames {
            if DEBUG {
                debug! {"RX {:?}", frame};
            }

            // Wake up sender
            if frame.loopback {
                in_flight = in_flight.saturating_sub(1);

                let callback = callbacks
                    .entry((frame.bus, frame.id))
                    .or_default()
                    .pop_front();

                match callback {
                    Some((tx_frame, callback)) => {
                        // Ensure the frame we received matches the frame belonging to the callback.
                        // If not, we have a bug in the adapter implementation and frames are sent/received out of order.
                        assert_eq!(tx_frame, frame);

                        // Callback might be dropped if the sender is not waiting for the response
                        callback.send(()).ok();
                    }
                    None => panic!("Received loopback frame with no pending callback"),
                };
            }

            rx_sender.send(frame).unwrap();
        }

        // Move queued TX frames into the send buffer, respecting the optional
        // hardware buffer limit.
        // TODO: use poll_recv_many?
        loop {
            if let Some(max) = buffer_size {
                if in_flight + buffer.len() >= max {
                    break;
                }
            }

            match tx_receiver.try_recv() {
                Ok((frame, callback)) => {
                    let mut loopback_frame = frame.clone();
                    loopback_frame.loopback = true;

                    // Insert callback into hashmap
                    callbacks
                        .entry((frame.bus, frame.id))
                        .or_default()
                        .push_back((loopback_frame, callback));

                    if DEBUG {
                        debug! {"TX {:?}", frame};
                    }

                    buffer.push_back(frame);
                }
                Err(_) => break,
            }
        }
        if !buffer.is_empty() {
            let queued = buffer.len();
            adapter.send(&mut buffer).await.unwrap();
            in_flight += queued - buffer.len();

            if !buffer.is_empty() {
                debug!(
                    "Failed to send all frames, requeueing {} frames",
                    buffer.len()
                );
            }
        }

        // On native, recv() blocks up to the adapter timeout so this loop is naturally
        // paced; the short sleep just avoids a tight spin if recv returns instantly. On
        // wasm there is no blocking sleep available and `recv().await` already yields to
        // the event loop.
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(std::time::Duration::from_micros(1));
    }
}

/// Async wrapper around a [`CanAdapter`]. On native platforms a background thread drives
/// the adapter; in the browser (`wasm32`) it is driven cooperatively via `spawn_local`.
/// Uses tokio channels to communicate with the processing task.
pub struct AsyncCanAdapter {
    #[cfg(not(target_arch = "wasm32"))]
    processing_handle: Option<std::thread::JoinHandle<()>>,
    recv_receiver: broadcast::Receiver<Frame>,
    send_sender: mpsc::Sender<(Frame, oneshot::Sender<()>)>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AsyncCanAdapter {
    /// Create an [`AsyncCanAdapter`] driving `adapter` on a dedicated background thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new<T: CanAdapter + Send + 'static>(adapter: T) -> Self {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (send_sender, send_receiver) = mpsc::channel(CAN_TX_BUFFER_SIZE);
        let (recv_sender, recv_receiver) = broadcast::channel(CAN_RX_BUFFER_SIZE);

        let mut ret = AsyncCanAdapter {
            shutdown: Some(shutdown_sender),
            processing_handle: None,
            recv_receiver,
            send_sender,
        };

        ret.processing_handle = Some(std::thread::spawn(move || {
            // `process` only awaits the adapter's blocking recv/send, so a minimal executor
            // (no tokio runtime) is sufficient to drive it on this dedicated thread.
            pollster::block_on(process(adapter, shutdown_receiver, recv_sender, send_receiver));
        }));

        ret
    }

    /// Create an [`AsyncCanAdapter`] driving `adapter` on the browser event loop via
    /// `spawn_local`. The adapter does not need to be [`Send`].
    #[cfg(target_arch = "wasm32")]
    pub fn new<T: CanAdapter + 'static>(adapter: T) -> Self {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (send_sender, send_receiver) = mpsc::channel(CAN_TX_BUFFER_SIZE);
        let (recv_sender, recv_receiver) = broadcast::channel(CAN_RX_BUFFER_SIZE);

        let ret = AsyncCanAdapter {
            shutdown: Some(shutdown_sender),
            recv_receiver,
            send_sender,
        };

        wasm_bindgen_futures::spawn_local(process(
            adapter,
            shutdown_receiver,
            recv_sender,
            send_receiver,
        ));

        ret
    }

    /// Send a single frame. The Future will resolve once the frame has been handed over to the adapter for sending. This does not mean the message is sent out on the CAN bus yet, as this could be pending arbitration.
    pub async fn send(&self, frame: &Frame) {
        // Create oneshot channel to signal the completion of the send operation
        let (callback_sender, callback_receiver) = oneshot::channel();
        self.send_sender
            .send((frame.clone(), callback_sender))
            .await
            .unwrap();

        callback_receiver.await.unwrap();
    }

    /// Receive all frames.
    pub fn recv(&self) -> impl Stream<Item = Frame> {
        self.recv_filter(|_| true)
    }

    /// Receive frames that match a filter. Useful in combination with stream adapters.
    pub fn recv_filter(&self, filter: impl Fn(&Frame) -> bool) -> impl Stream<Item = Frame> {
        let mut rx = self.recv_receiver.resubscribe();

        Box::pin(stream! {
            loop { match rx.recv().await {
                    Ok(frame) => {
                        if filter(&frame) {
                            yield frame
                        }
                    },
                    Err(RecvError::Closed) => {
                        tracing::debug!("Adapter broadcast closed — ending recv stream");
                        return;
                    },
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!("Receive too slow, dropping {} frame(s).", n)
                    },
                }
            }
        })
    }
}

impl Drop for AsyncCanAdapter {
    fn drop(&mut self) {
        // Send shutdown signal to the processing task.
        // Use `ok()` instead of `unwrap()` because the receiver may already
        // be dropped if the process loop exited early (e.g. adapter error).
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }

        // On native, join the background thread; use `ok()` to avoid panicking inside Drop
        // if the process thread panicked (double-panic would abort the process). On wasm
        // there is no thread to join — the `spawn_local` task observes the shutdown signal
        // and exits on its own.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(handle) = self.processing_handle.take() {
            let _ = handle.join();
        }
    }
}

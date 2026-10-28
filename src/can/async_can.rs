//! Async wrapper for adapters implementing the [`CanAdapter`] trait.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::can::{CanAdapter, Frame, Identifier};
use crate::{Error, Result, Stream, StreamExt};
use async_stream::stream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

const CAN_TX_BUFFER_SIZE: usize = 128;
// Enough to receive a complete multi-frame ISO-TP exchange while sending.
const CAN_RX_BUFFER_SIZE: usize = 8192;

type BusIdentifier = (u8, Option<Identifier>);

#[derive(Default)]
struct WorkerState {
    generation: u64,
    error: Option<Error>,
}

struct Transmission {
    frame: Frame,
    completion: oneshot::Sender<Result<()>>,
    deadline: Option<Instant>,
    generation: u64,
}

impl Transmission {
    fn expired(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    fn finish(self, result: Result<()>) {
        let _ = self.completion.send(result);
    }
}

fn terminal_error(state: &watch::Receiver<WorkerState>) -> Error {
    state.borrow().error.clone().unwrap_or(Error::Disconnected)
}

fn process<T: CanAdapter>(
    mut adapter: T,
    mut shutdown: oneshot::Receiver<()>,
    rx_sender: broadcast::Sender<Frame>,
    mut tx_receiver: mpsc::Receiver<Transmission>,
    state: watch::Sender<WorkerState>,
) {
    let mut queued: VecDeque<Transmission> = VecDeque::new();
    let mut pending: HashMap<BusIdentifier, VecDeque<Transmission>> = HashMap::new();
    let buffer_size = adapter.buffer_size().unwrap_or(CAN_TX_BUFFER_SIZE);
    let mut in_flight = 0;
    let mut first_tx_confirmed = false;

    let result: Result<()> = (|| {
        // Recreating a Rust adapter does not empty the hardware queues. Start
        // the same cancellation/replacement path used after a timeout.
        if T::supports_tx_timeout() {
            adapter.cancel_pending_tx()?;
        }

        loop {
            if !matches!(
                shutdown.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ) {
                return Ok(());
            }

            for frame in adapter.recv()? {
                if frame.loopback {
                    // Startup/queue clearing can leave old echoes buffered in
                    // RX, even though their TX requests have been discarded.
                    // Warn and ignore unmatched echoes until the first current
                    // TX is CONFIRMED (merely submitting it is not enough).
                    // After that, an unmatched echo violates the adapter's
                    // completion/order contract and must panic. A queue clear
                    // resets first_tx_confirmed and opens this grace period
                    // again. Never consume a callback or credit for an
                    // unmatched echo.
                    let key = (frame.bus, frame.id());
                    // Adapters may lose or change frame flags in TX echoes.
                    // Bus and identifier select the FIFO; match payload only.
                    if let Some(txs) = pending.get_mut(&key).filter(|txs| {
                        txs.front()
                            .is_some_and(|tx| tx.frame.data() == frame.data())
                    }) {
                        // A matching echo confirms TX even if recv() crossed
                        // its deadline. Check timeouts only after processing
                        // received confirmations, for sends still pending.
                        let tx = txs.pop_front().unwrap();
                        in_flight -= 1;
                        tx.finish(Ok(()));
                        first_tx_confirmed = true;
                        if txs.is_empty() {
                            pending.remove(&key);
                        }
                    } else {
                        assert!(
                            !first_tx_confirmed,
                            "Unmatched transmit echo after first TX confirmation: {frame:?}"
                        );
                        tracing::warn!(
                            ?frame,
                            "Ignoring unmatched echo before first TX confirmation"
                        );
                    }
                }
                let _ = rx_sender.send(frame);
            }

            let now = Instant::now();
            // A channel-wide clear invalidates the ENTIRE session, including
            // requests still waiting for admission to the bounded TX channel.
            let cancel = queued.iter().any(|tx| tx.expired(now))
                || pending.values().flatten().any(|tx| {
                    tx.expired(now) || (T::supports_tx_timeout() && tx.completion.is_closed())
                });
            if cancel {
                adapter.cancel_pending_tx()?;
                // Classify against the completed clear, which may have blocked
                // while other sends reached their deadlines.
                let now = Instant::now();
                // Each send captures the generation BEFORE waiting for channel
                // capacity. Clearing hardware and draining queued/pending is
                // insufficient: older requests can still be in tx_receiver or
                // waiting to enter it. Advancing the watch value wakes those
                // callers with Cancelled; the generation check below rejects
                // old requests that enter the channel later. Sends started
                // after this change belong to the new session and can proceed.
                // Post results before notifying watchers so send() observes the
                // worker's classification. The state lock also prevents an
                // immediate retry from capturing the old generation.
                state.send_modify(|state| {
                    for tx in queued
                        .drain(..)
                        .chain(pending.drain().flat_map(|(_, txs)| txs))
                    {
                        let reason = if tx.expired(now) {
                            Error::Timeout
                        } else {
                            Error::Cancelled
                        };
                        tx.finish(Err(reason));
                    }
                    state.generation = state.generation.wrapping_add(1);
                });
                in_flight = 0;
                first_tx_confirmed = false;
            }

            // Bound software staging independently of hardware capacity. Check
            // generations even when hardware is full so clears unblock waiters.
            while queued.len() < CAN_TX_BUFFER_SIZE {
                match tx_receiver.try_recv() {
                    Ok(tx) if tx.generation != state.borrow().generation => {
                        tx.finish(Err(Error::Cancelled))
                    }
                    Ok(tx) if tx.completion.is_closed() => {}
                    Ok(tx) => queued.push_back(tx),
                    Err(_) => break,
                }
            }
            queued.retain(|tx| !tx.completion.is_closed());

            // Newly admitted sends are checked for expiry next iteration,
            // after giving their received confirmations a chance to complete.
            let count = queued.len().min(buffer_size.saturating_sub(in_flight));
            if count > 0 {
                let mut frames: VecDeque<_> = queued
                    .iter()
                    .take(count)
                    .map(|tx| tx.frame.clone())
                    .collect();
                adapter.send(&mut frames)?;
                let sent = count - frames.len();
                // Only frames accepted by the backend can have completions.
                for tx in queued.drain(..sent) {
                    pending
                        .entry((tx.frame.bus, tx.frame.id()))
                        .or_default()
                        .push_back(tx);
                }
                in_flight += sent;
            }
            std::thread::sleep(Duration::from_micros(1));
        }
    })();

    // Preserve the underlying adapter error for every waiter, including those
    // whose command has not reached the worker yet. Publish before closing any
    // channels so callers cannot race this into a generic disconnected error.
    let failure = result.err().unwrap_or(Error::Disconnected);
    state.send_modify(|state| state.error = Some(failure.clone()));
    for tx in queued
        .drain(..)
        .chain(pending.drain().flat_map(|(_, txs)| txs))
    {
        tx.finish(Err(failure.clone()));
    }
    while let Ok(tx) = tx_receiver.try_recv() {
        tx.finish(Err(failure.clone()));
    }
}

/// Async wrapper around a [`CanAdapter`] with a dedicated TX/RX worker.
pub struct AsyncCanAdapter {
    worker: Mutex<Worker>,
    recv_receiver: broadcast::Receiver<Frame>,
    send_sender: mpsc::Sender<Transmission>,
    state: watch::Receiver<WorkerState>,
    tx_timeout: Option<Duration>,
    default_brs: bool,
}

struct Worker {
    processing_handle: Option<std::thread::JoinHandle<()>>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AsyncCanAdapter {
    pub fn new<T: CanAdapter + Send + 'static>(adapter: T) -> Self {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (send_sender, send_receiver) = mpsc::channel(CAN_TX_BUFFER_SIZE);
        let (recv_sender, recv_receiver) = broadcast::channel(CAN_RX_BUFFER_SIZE);
        let (state_sender, state) = watch::channel(WorkerState::default());
        let tx_timeout = adapter.tx_timeout();
        let default_brs = adapter.default_brs();
        let processing_handle = std::thread::spawn(move || {
            process(
                adapter,
                shutdown_receiver,
                recv_sender,
                send_receiver,
                state_sender,
            );
        });
        Self {
            worker: Mutex::new(Worker {
                shutdown: Some(shutdown_sender),
                processing_handle: Some(processing_handle),
            }),
            recv_receiver,
            send_sender,
            state,
            tx_timeout,
            default_brs,
        }
    }

    /// Default bitrate-switching setting reported by the backend for CAN FD.
    pub fn default_brs(&self) -> bool {
        self.default_brs
    }

    /// Stop the worker and release the hardware, even while shared references exist.
    ///
    /// This is idempotent and blocks until the worker has stopped. Pending sends
    /// fail with `Disconnected` and receive streams finish. Async callers should
    /// run this on a blocking thread.
    pub fn close(&self) {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(shutdown) = worker.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = worker.processing_handle.take() {
            let _ = handle.join();
        }
    }

    /// Send a frame and await the adapter's transmit confirmation.
    ///
    /// Confirmation strength depends on the backend: some adapters emulate it
    /// when accepting the frame. Echoes match the oldest pending send for their
    /// bus and identifier by payload, ignoring frame type and flags.
    /// The adapter config's optional TX timeout covers
    /// queue admission and confirmation. A timeout triggers cancellation of all
    /// pending TX; other affected sends return [`Error::Cancelled`]. Vector
    /// defers its hardware clear until the next real frame, using HIGHPRIO.
    /// A matching confirmation received before cancellation succeeds even if
    /// its deadline has passed.
    /// Adapter failures retain their original error variant.
    pub async fn send(&self, frame: &Frame) -> Result<()> {
        if frame.is_error() {
            return Err(Error::NotSupported);
        }
        let deadline = self.tx_timeout.map(|timeout| Instant::now() + timeout);
        let mut state = self.state.clone();
        let current_generation = state.borrow_and_update().generation;
        let (completion, response) = oneshot::channel();
        let tx = Transmission {
            frame: frame.clone(),
            completion,
            deadline,
            generation: current_generation,
        };
        let send = async {
            self.send_sender
                .send(tx)
                .await
                .map_err(|_| terminal_error(&self.state))?;
            response.await.map_err(|_| terminal_error(&self.state))?
        };
        let cancelled = async {
            let _ = state.changed().await;
            // The worker supplies timeout results for admitted sends. Requests
            // still waiting for admission are invalidated by the queue clear.
            state.borrow().error.clone().unwrap_or(Error::Cancelled)
        };
        // The worker decides between confirmations and timeout cancellation.
        // A caller-side timer could return Timeout before it processes a
        // matching echo received across the deadline.
        tokio::select! {
            biased;
            result = send => result,
            error = cancelled => Err(error),
        }
    }

    /// Receive all frames.
    pub fn recv(&self) -> impl Stream<Item = Frame> {
        self.recv_filter(|_| true)
    }

    /// Collect frames, including transmit echoes, for the given duration.
    ///
    /// Creates an independent subscription when polled and closes it on return
    /// or cancellation. Returns frames in receive order, or finishes early if
    /// the adapter closes. Existing receive subscriptions are unaffected.
    pub async fn recv_for(&self, duration: Duration) -> Vec<Frame> {
        let mut stream = self.recv();
        let mut frames = Vec::new();
        let _ = tokio::time::timeout(duration, async {
            while let Some(frame) = stream.next().await {
                frames.push(frame);
            }
        })
        .await;
        frames
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
        self.close();
    }
}

//! Async wrapper for adapters implementing the [`CanAdapter`] trait.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::can::{CanAdapter, Frame, Identifier};
use crate::{Error, Result, Stream};
use async_stream::stream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

const CAN_TX_BUFFER_SIZE: usize = 128;
// Enough to receive a complete multi-frame ISO-TP exchange while sending.
const CAN_RX_BUFFER_SIZE: usize = 8192;

type BusIdentifier = (u8, Identifier);
type TerminalError = Arc<Mutex<Option<Error>>>;

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

fn terminal_error(error: &TerminalError) -> Error {
    error.lock().unwrap().clone().unwrap_or(Error::Disconnected)
}

fn process<T: CanAdapter>(
    mut adapter: T,
    mut shutdown: oneshot::Receiver<()>,
    rx_sender: broadcast::Sender<Frame>,
    mut tx_receiver: mpsc::Receiver<Transmission>,
    generation: watch::Sender<u64>,
    error: TerminalError,
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

            let now = Instant::now();
            // A channel-wide clear invalidates the ENTIRE session, including
            // requests still waiting for admission to the bounded TX channel.
            let cancel = queued.iter().any(|tx| tx.expired(now))
                || pending.values().flatten().any(|tx| {
                    tx.expired(now) || (T::supports_tx_timeout() && tx.completion.is_closed())
                });
            if cancel {
                adapter.cancel_pending_tx()?;
                // Each send captures the generation BEFORE waiting for channel
                // capacity. Clearing hardware and draining queued/pending is
                // insufficient: older requests can still be in tx_receiver or
                // waiting to enter it. Advancing the watch value wakes those
                // callers with Cancelled; the generation check below rejects
                // old requests that enter the channel later. Sends started
                // after this change belong to the new session and can proceed.
                generation.send_modify(|value| *value = value.wrapping_add(1));
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
                in_flight = 0;
                first_tx_confirmed = false;
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
                    let key = (frame.bus, frame.id);
                    if let Some(txs) = pending.get_mut(&key) {
                        if txs.front().is_some_and(|tx| {
                            let mut expected = tx.frame.clone();
                            expected.loopback = true;
                            expected == frame
                        }) {
                            // recv() may span the deadline. Leave an expired
                            // matching send for cancellation next iteration;
                            // this is a timeout, not an unmatched-echo bug.
                            if !txs.front().unwrap().expired(Instant::now()) {
                                let tx = txs.pop_front().unwrap();
                                in_flight -= 1;
                                tx.finish(Ok(()));
                                first_tx_confirmed = true;
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
                        if txs.is_empty() {
                            pending.remove(&key);
                        }
                    } else {
                        assert!(!first_tx_confirmed, "Transmit echo without a pending send after first TX confirmation: {frame:?}");
                        tracing::warn!(
                            ?frame,
                            "Ignoring unmatched echo before first TX confirmation"
                        );
                    }
                }
                let _ = rx_sender.send(frame);
            }

            // Bound software staging independently of hardware capacity. Check
            // generations even when hardware is full so clears unblock waiters.
            while queued.len() < CAN_TX_BUFFER_SIZE {
                match tx_receiver.try_recv() {
                    Ok(tx) if tx.generation != *generation.borrow() => {
                        tx.finish(Err(Error::Cancelled))
                    }
                    Ok(tx) if tx.completion.is_closed() => {}
                    Ok(tx) => queued.push_back(tx),
                    Err(_) => break,
                }
            }
            queued.retain(|tx| !tx.completion.is_closed());

            // Expired queued requests must never be submitted, including ones
            // just admitted above. The next iteration cancels the session.
            if queued.iter().any(|tx| tx.expired(Instant::now())) {
                continue;
            }
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
                        .entry((tx.frame.bus, tx.frame.id))
                        .or_default()
                        .push_back(tx);
                }
                in_flight += sent;
            }
            std::thread::sleep(Duration::from_micros(50));
        }
    })();

    // Preserve the underlying adapter error for every waiter, including those
    // whose command has not reached the worker yet. Publish before closing any
    // channels so callers cannot race this into a generic disconnected error.
    let failure = result.err().unwrap_or(Error::Disconnected);
    *error.lock().unwrap() = Some(failure.clone());
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
    processing_handle: Option<std::thread::JoinHandle<()>>,
    recv_receiver: broadcast::Receiver<Frame>,
    send_sender: mpsc::Sender<Transmission>,
    generation: watch::Receiver<u64>,
    error: TerminalError,
    tx_timeout: Option<Duration>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AsyncCanAdapter {
    pub fn new<T: CanAdapter + Send + 'static>(adapter: T) -> Self {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (send_sender, send_receiver) = mpsc::channel(CAN_TX_BUFFER_SIZE);
        let (recv_sender, recv_receiver) = broadcast::channel(CAN_RX_BUFFER_SIZE);
        let (generation_sender, generation) = watch::channel(0);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let tx_timeout = adapter.tx_timeout();
        let processing_handle = std::thread::spawn(move || {
            process(
                adapter,
                shutdown_receiver,
                recv_sender,
                send_receiver,
                generation_sender,
                worker_error,
            );
        });
        Self {
            shutdown: Some(shutdown_sender),
            processing_handle: Some(processing_handle),
            recv_receiver,
            send_sender,
            generation,
            error,
            tx_timeout,
        }
    }

    /// Send a frame and await the adapter's transmit confirmation.
    ///
    /// Confirmation strength depends on the backend: some adapters emulate it
    /// when accepting the frame. The adapter config's optional TX timeout covers
    /// queue admission and confirmation. A timeout triggers cancellation of all
    /// pending TX; other affected sends return [`Error::Cancelled`]. Vector
    /// defers its hardware clear until the next real frame, using HIGHPRIO.
    /// Adapter failures retain their original error variant.
    pub async fn send(&self, frame: &Frame) -> Result<()> {
        let deadline = self.tx_timeout.map(|timeout| Instant::now() + timeout);
        let mut generation = self.generation.clone();
        let current_generation = *generation.borrow_and_update();
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
                .map_err(|_| terminal_error(&self.error))?;
            response.await.map_err(|_| terminal_error(&self.error))?
        };
        let cancelled = async {
            let _ = generation.changed().await;
            let error = self.error.lock().unwrap().clone();
            if let Some(error) = error {
                error
            } else if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                Error::Timeout
            } else {
                Error::Cancelled
            }
        };
        let wait = async {
            tokio::select! {
                biased;
                result = send => result,
                error = cancelled => Err(error),
            }
        };
        if let Some(deadline) = deadline {
            tokio::time::timeout_at(deadline.into(), wait)
                .await
                .map_err(|_| Error::Timeout)?
        } else {
            wait.await
        }
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
        if let Some(handle) = self.processing_handle.take() {
            // Send shutdown signal to background thread.
            // Use `ok()` instead of `unwrap()` because the receiver may already
            // be dropped if the process thread exited early (e.g. adapter error).
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            // Join the thread; use `ok()` to avoid panicking inside Drop if the
            // process thread panicked (double-panic would abort the process).
            let _ = handle.join();
        }
    }
}

//! Async wrapper for Adapters implementing the [`CanAdapter`] trait.

use crate::can::CanAdapter;
use crate::can::Frame;
use async_stream::stream;
use futures_core::stream::Stream;
use tokio::sync::{broadcast, mpsc, oneshot};

const CAN_TX_BUFFER_SIZE: usize = 128;
const CAN_RX_BUFFER_SIZE: usize = 1024;

fn process<T: CanAdapter>(
    mut adapter: T,
    mut shutdown_receiver: oneshot::Receiver<()>,
    rx_sender: broadcast::Sender<Frame>,
    mut tx_receiver: mpsc::Receiver<Frame>,
) {
    let mut buffer: Vec<Frame> = Vec::new();

    while !shutdown_receiver.try_recv().is_ok() {
        let frames: Vec<Frame> = adapter.recv().unwrap();
        for frame in frames {
            rx_sender.send(frame).unwrap();
        }

        // TODO: use poll_recv_many?
        buffer.clear();
        while let Ok(frame) = tx_receiver.try_recv() {
            buffer.push(frame);
        }
        if !buffer.is_empty() {
            adapter.send(&buffer).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Async wrapper around a [`CanAdapter`]. Starts a background thread to handle sending and receiving frames. Uses tokio channels to communicate with the background thread.
pub struct AsyncCanAdapter {
    processing_handle: Option<std::thread::JoinHandle<()>>,
    recv_receiver: broadcast::Receiver<Frame>,
    send_sender: mpsc::Sender<Frame>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AsyncCanAdapter {
    pub fn new<T: CanAdapter + Send + Sync + 'static>(adapter: T) -> Self {
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
            process(adapter, shutdown_receiver, recv_sender, send_receiver);
        }));

        ret
    }

    /// Send a single frame. The Future will resolve once the frame has been put in the queue for the background thread. This does not mean the frame is sent out by the adapter.
    pub async fn send(&self, frame: &Frame) {
        self.send_sender.send(frame.clone()).await.unwrap();
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
                        } else {
                            continue
                        }
                    }
                    Err(_) => continue,
                }
            }
        })
    }
}

impl Drop for AsyncCanAdapter {
    fn drop(&mut self) {
        match self.processing_handle.take() {
            Some(handle) => {
                // Send shutdown signal to background tread
                self.shutdown.take().unwrap().send(()).unwrap();
                handle.join().unwrap();
            }
            None => {}
        }
    }
}

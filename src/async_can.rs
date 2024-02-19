use crate::can::CanAdapter;
use crate::can::Frame;
use async_stream::stream;
use futures_core::stream::Stream;
use tokio::sync::{broadcast, oneshot, mpsc};
use tracing::{info, warn};

const CAN_TX_BUFFER_SIZE: usize = 128;

fn process<T: CanAdapter>(mut adapter: T, mut shutdown_receiver: oneshot::Receiver<()>, rx_sender: broadcast::Sender<Frame>, mut tx_receiver: mpsc::Receiver<Frame>) {
    while !shutdown_receiver.try_recv().is_ok() {
        let frames: Vec<Frame> = adapter.recv().unwrap();
        for frame in frames {
            rx_sender.send(frame).unwrap();
        }

        // TODO: use poll_recv_many
        while let Ok(frame) = tx_receiver.try_recv() {
            info!("Sending frame: {:?}", frame);
            adapter.send(&[frame]).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub struct AsyncCanAdapter {
    processing_handle: Option<std::thread::JoinHandle<()>>,
    recv_queue: (broadcast::Sender<Frame>, broadcast::Receiver<Frame>),
    send_sender: mpsc::Sender<Frame>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl AsyncCanAdapter {
    pub fn new<T: CanAdapter + Send + Sync + 'static>(adapter: T) -> Self {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (send_sender, send_receiver) = mpsc::channel(CAN_TX_BUFFER_SIZE);

        let mut ret = AsyncCanAdapter {
            shutdown: Some(shutdown_sender),
            processing_handle: None,
            recv_queue: broadcast::channel::<Frame>(16),
            send_sender: send_sender,
        };

        let recv_sender = ret.recv_queue.0.clone();

        ret.processing_handle = Some(std::thread::spawn(move || {
            process(adapter, shutdown_receiver, recv_sender, send_receiver);
        }));

        ret
    }

    pub async fn send(&self, frame: &Frame) {
        self.send_sender.send(frame.clone()).await.unwrap();
    }

    pub fn recv(&self) -> impl Stream<Item = Frame> {
        self.recv_filter(|_| true)
    }

    pub fn recv_filter(&self, filter: fn(&Frame) -> bool) -> impl Stream<Item = Frame> {
        let mut rx = self.recv_queue.0.subscribe();

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

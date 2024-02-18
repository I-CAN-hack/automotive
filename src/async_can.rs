use crate::can::CanAdapter;
use crate::can::Frame;
use tokio::sync::broadcast;

fn process<T: CanAdapter>(mut adapter: T, rx_sender: broadcast::Sender<Frame>) {
    loop {
        let frames: Vec<Frame> = adapter.recv().unwrap();
        for frame in frames {
            rx_sender.send(frame).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub struct AsyncCanAdapter {
    recv_queue: (broadcast::Sender<Frame>, broadcast::Receiver<Frame>),
}

impl AsyncCanAdapter {
    pub fn new<T: CanAdapter + Send + Sync + 'static>(adapter: T) -> Self {
        let ret = AsyncCanAdapter {
            recv_queue: broadcast::channel::<Frame>(16),
        };

        let rx2 = ret.recv_queue.0.clone();

        std::thread::spawn(move || {
            process(adapter, rx2);
        });

        ret
    }

    // TODO: return some kind of async iterator so you receive without dropping
    pub async fn recv(&self) -> Result<Frame, crate::error::Error> {
        let mut rx = self.recv_queue.0.subscribe();

        loop {
            match rx.recv().await {
                Ok(frame) => return Ok(frame),
                Err(_) => continue,
            }
        }
    }
}

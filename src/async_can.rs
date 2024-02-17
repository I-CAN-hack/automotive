use crate::can::CanAdapter;
use crate::can::Frame;
use std::borrow::BorrowMut;
use std::sync::Arc;
use std::sync::Mutex;

// TODO make async
pub trait AsyncCanAdapter {
    fn send(&mut self, frames: &[Frame]) -> Result<(), crate::error::Error>;
    fn recv() -> Result<Frame, crate::error::Error>; // TODO: return iterator
}

async fn process<T: CanAdapter + Send + Sync>(adapter: Arc<Mutex<&mut T>>) {
    loop {
        let frames: Vec<Frame> = adapter.lock().unwrap().borrow_mut().recv().unwrap();
        for frame in frames {
            // TODO: Send frames on broadcast channel
            let id: u32 = frame.id.into();
            println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

pub struct AsyncCanWrapper {
}

impl AsyncCanWrapper {
    pub fn new<T: CanAdapter + Send + Sync>(can_adapter: &'static mut T) -> Self {
        // TX
        // let (tx, rx) = tokio::sync::mpsc::channel();

        // RX
        // let (tx, mut rx1) = broadcast::channel::<Frame>(16);

        let adapter = Arc::new(Mutex::new(can_adapter));
        tokio::spawn(
            async move {
                process(adapter).await;
            }
        );

        AsyncCanWrapper {
        }
    }
}

use automotive::async_can::AsyncCanWrapper;
use automotive::panda::Panda;

// use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    // Can this be cleaned up?
    let panda = Box::new(Panda::new().unwrap());
    let panda: &'static mut Panda = Box::leak(panda);

    let _async_can = AsyncCanWrapper::new(panda);

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        println!("ping");
    }
}

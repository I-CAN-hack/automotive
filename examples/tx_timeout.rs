//! Queue two CAN frames and retry one second after TX timeout/cancellation.
//!
//! Enable the feature for the adapter to test, for example:
//! `cargo run --example tx_timeout --features peak`.
//! Adapters with cancellation support use a one-second TX timeout by default.
//! Without an ACKing peer, the first frame should time out and the second should
//! be cancelled by the queue clear. Both results are printed with the same
//! adapter kept open between retries.

#[tokio::main]
async fn main() -> automotive::Result<()> {
    use automotive::can::{Frame, Identifier};
    use automotive::Error;
    use std::time::Duration;

    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter()?;
    let first = Frame::new(0, Identifier::Standard(0x123), &[1, 2, 3, 4, 5, 6, 7, 8])?;
    let second = Frame::new(0, Identifier::Standard(0x124), &[8, 7, 6, 5, 4, 3, 2, 1])?;

    loop {
        // Await both results: try_join! would drop the other send on the first
        // error, hiding the cancellation we want to observe.
        let (first_result, second_result) = tokio::join!(
            async {
                println!("Sending {first:?}");
                adapter.send(&first).await
            },
            async {
                // A later deadline lets the queue clear cancel this send before
                // its own timeout. Both frames remain pending concurrently.
                tokio::time::sleep(Duration::from_millis(250)).await;
                println!("Sending {second:?}");
                adapter.send(&second).await
            }
        );

        let mut retry = false;
        let mut adapter_error = None;
        for (frame, result) in [(&first, first_result), (&second, second_result)] {
            match result {
                Ok(()) => println!("{:?}: TX confirmed", frame.id),
                Err(Error::Timeout) => {
                    println!("{:?}: Timeout", frame.id);
                    retry = true;
                }
                Err(Error::Cancelled) => {
                    println!("{:?}: Cancelled by the transmit queue clear", frame.id);
                    retry = true;
                }
                Err(error) => {
                    eprintln!("{:?}: {error}", frame.id);
                    adapter_error = Some(error);
                }
            }
        }
        if let Some(error) = adapter_error {
            return Err(error);
        }
        if !retry {
            return Ok(());
        }
        println!("Keeping the adapter open; retrying both frames in one second...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

//! Queue two CAN frames and retry one second after TX timeout/cancellation.
//!
//! Enable the feature for the adapter to test, for example:
//! `cargo run --example tx_timeout --features peak`.
//! Adapters with cancellation support use a one-second TX timeout by default.
//! Without an ACKing peer, the first frame should time out and the second should
//! be cancelled by the queue clear. Both results are printed with the same
//! adapter kept open between retries. Each frame carries an incrementing
//! little-endian u64 counter so cancelled frames are identifiable in bus traces.

#[tokio::main]
async fn main() -> automotive::Result<()> {
    use automotive::can::{Frame, Identifier};
    use automotive::Error;
    use std::time::Duration;

    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter()?;
    let mut counter = 0_u64;

    loop {
        let first = Frame::builder(Identifier::standard(0x123)?)
            .data(counter.to_le_bytes())
            .build()?;
        let second = Frame::builder(Identifier::standard(0x124)?)
            .data(counter.to_le_bytes())
            .build()?;
        counter = counter.wrapping_add(1);

        // Await both results: try_join! would drop the other send on the first
        // error, hiding the cancellation we want to observe.
        let (first_result, second_result) = tokio::join!(
            async {
                println!("Sending {first:?} (counter {counter})");
                adapter.send(&first).await
            },
            async {
                // A later deadline lets the queue clear cancel this send before
                // its own timeout. Both frames remain pending concurrently.
                tokio::time::sleep(Duration::from_millis(250)).await;
                println!("Sending {second:?} (counter {counter})");
                adapter.send(&second).await
            }
        );

        let mut retry = false;
        let mut adapter_error = None;
        for (frame, result) in [(&first, first_result), (&second, second_result)] {
            match result {
                Ok(()) => println!("{:?} (counter {counter}): TX confirmed", frame.id()),
                Err(Error::Timeout) => {
                    println!("{:?} (counter {counter}): Timeout", frame.id());
                    retry = true;
                }
                Err(Error::Cancelled) => {
                    println!(
                        "{:?} (counter {counter}): Cancelled by the transmit queue clear",
                        frame.id()
                    );
                    retry = true;
                }
                Err(error) => {
                    eprintln!("{:?} (counter {counter}): {error}", frame.id());
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

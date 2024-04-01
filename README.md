# The Automotive Crate
[![crates.io](https://img.shields.io/crates/v/automotive.svg)](https://crates.io/crates/automotive)
[![docs.rs](https://img.shields.io/docsrs/automotive)](https://docs.rs/automotive/latest/automotive/)

Welcome to the `automotive` crate documentation. The purpose of this crate is to help you with all things automotive related. Most importantly, it provides a fully async CAN interface supporting multiple adapters.

## Async CAN Example
The following adapter opens the first available adapter on the system, and then receives all frames. Note how the sent frame is awaited, which waits until the message is ACKed on the CAN bus.

```rust
use automotive::StreamExt;
async fn can_example() -> automotive::Result<()> {
    let adapter = automotive::can::get_adapter()?;
    let mut stream = adapter.recv();

    let frame = automotive::can::Frame::new(0, 0x541.into(), &[0xff; 8])?;
    adapter.send(&frame).await;

    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
    Ok(())
}
```

## UDS Example
The automotive crate also supplies interfaces for various diagnostic protocols such as UDS. The adapter is first wrapped to support the ISO Transport Layer, then a UDS Client is created. All methods are fully async, making it easy to communicate with multiple ECUs in parallel. See [automotive#21](https://github.com/I-CAN-hack/automotive/issues/21) for progress on the supported SIDs.

```rust
 async fn uds_example() -> automotive::Result<()> {
    let adapter = automotive::can::get_adapter()?;
    let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
    let uds = automotive::uds::UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();
    let response = uds.read_data_by_identifier(automotive::uds::DataIdentifier::ApplicationSoftwareIdentification as u16).await?;

    println!("Application Software Identification: {}", hex::encode(response));
    Ok(())
 }
```

## CAN Adapters
The following CAN adapters are supported.

### Supported CAN adapters
 - SocketCAN (Linux only, supported using [socketcan-rs](https://github.com/socketcan-rs/socketcan-rs))
 - comma.ai panda (all platforms using [rusb](https://crates.io/crates/rusb))

### Known limitations / Notes
This library has some unique features that might expose (performance) issues in drivers you wouldn't otherwise notice, so check the list of known limitations below.

This library supports awaiting a sent frame and waiting for the ACK on the CAN bus. This requires receiving these ACKs from the adapter, and matching them to the appropriate sent frame. This requires some level of hardware support that is not offered by all adapters/drivers. If this is not supported by the driver, an ACK will be simulated as soon as the frame is transmitted, but this can cause issues if precise timing is needed.

 - SocketCAN drivers without `IFF_ECHO`: This class of SocketCAN drivers has no hardware support for notifying the driver when a frame was ACKed. This is instead emulated by the [Linux kernel](https://github.com/torvalds/linux/blob/master/net/can/af_can.c#L256). Due to transmitted frames immediately being received again this can cause the receive queue to fill up if more than 476 (default RX queue size on most systems) are transmitted in one go. To solve this we implement emulated ACKs ourself, instead of relying on the ACKs from the kernel.
 - comma.ai panda: The panda does not retry frames that are not ACKed, and drops them instead. This can cause panics in some internal parts of the library when frames are dropped. [panda#1922](https://github.com/commaai/panda/issues/1922) tracks this issue. The CAN-FD flag on a frame is also ignored, if the hardware is configured for CAN-FD all frames will be interpreted as FD regardless of the FD frame bit (r0 bit).
 - PCAN-USB: The Peak CAN adapters have two types of drivers. One built-in to the linux kernel (`peak_usb`), and an out-of-tree one (`pcan`) that can be [downloaded](https://www.peak-system.com/fileadmin/media/linux/index.htm) from Peak System's website. The kernel driver properly implements `IFF_ECHO`, but has a rather small TX queue. This should not cause any issues, but it can be inreased with `ifconfig can0 txqueuelen <size>`. The out-of-tree driver is not recommended as it does  not implement `IFF_ECHO`.
  - neoVI/ValueCAN: Use of Intrepid Control System's devices is not recommended due to issues in their SocketCAN driver. If many frames are transmitted simultaneously it will cause the whole system/kernel to hang. [intrepid-socketcan-kernel-module#20](https://github.com/intrepidcs/intrepid-socketcan-kernel-module/issues/20) tracks this issue.


### Implementing a New Adapter
Implementing a new adapter is done by implementing the `CanAdapter` Trait. Hardware implementations can be blocking, as the [AsyncCanAdapter](https://docs.rs/automotive/latest/automotive/async_can/struct.AsyncCanAdapter.html) takes care of presenting an async interface to the user. The library makes some assumptions around sending/receiving frames. These assumption are also verified by the tests in `tests/adapter_tests.rs`.

 - The `send` function takes a `&mut VecDequeue` of frames. Frames to be sent are taken from the *front* of this queue. If there is no space in the hardware or driver buffer to send out all messages it's OK to return before the queue is fully empty. If an error occurs make sure to put the message back at the beginning of the queue and return.
 - The hardware or driver is free to prioritize sending frames with a lower Arbitration ID to prevent priority inversion. However frames with the same Arbitration ID need to be send out on the CAN bus in the same order as they were queued. This assumption is needed to match a received ACK to the correct frame.
 - Once a frame is ACKed it should be put in the receive queue with the `loopback` flag set. The `AsyncCanAdapter` wrapper will take care of matching it against the right transmit frame and resolving the Future. If this is not supported by the underlying hardware, this can be faked by looping back all transmitted frames immediately.



 ## Roadmap
 Features I'd like to add in the future. Also check the [issues page](https://github.com/I-CAN-hack/automotive/issues?q=is%3Aopen+is%3Aissue+label%3Aenhancement).
 - CCP/XCP Client
 - Update file extraction (e.g. .frf and .odx)
 - VIN Decoding
 - J2534 Support on Windows
 - More device support
 - WebUSB support

# The Automotive Crate
[![crates.io](https://img.shields.io/crates/v/automotive.svg)](https://crates.io/crates/automotive)
[![docs.rs](https://img.shields.io/docsrs/automotive)](https://docs.rs/automotive/latest/automotive/)

Welcome to the `automotive` crate documentation. The purpose of this crate is to help you with all things automotive related. Most importantly, it provides a fully async CAN interface supporting multiple adapters.

## Async CAN Example

The following adapter opens the first available adapter on the system, and then receives all frames.

```rust
let adapter = automotive::adapter::get_adapter().unwrap();
let mut stream = adapter.recv();

while let Some(frame) = stream.next().await {
    let id: u32 = frame.id.into();
    println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
}
```

## UDS Example

The automotive crate also supplies interfaces for various diagnostic protocols such as UDS. The adapter is first wrapped to support the ISO Transport Layer, then a UDS Client is created. All methods are fully async, making it easy to communicate with multiple ECUs in parallel. See [https://github.com/I-CAN-hack/automotive/issues/21](https://github.com/I-CAN-hack/automotive/issues/21) for progress on the supported SIDs.

```rust
let adapter = automotive::adapter::get_adapter().unwrap();
let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
let uds = automotive::uds::UDSClient::new(&isotp);

uds.tester_present().await.unwrap();
let response = uds.read_data_by_identifier(DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();

println!("Application Software Identification: {}", hex::encode(response));
```

## Suported adapters
The following adapters are supported. Hardware implementations can be blocking, as the [AsyncCanAdapter](https://docs.rs/automotive/latest/automotive/async_can/struct.AsyncCanAdapter.html) takes care of presenting an async interface to the user.
 - SocketCAN (Linux only, supported using [socketcan-rs](https://github.com/socketcan-rs/socketcan-rs))
 - comma.ai panda (all platforms)


 ## Roadmap
 Features I'd like to add in the future. Also check the [issues page](https://github.com/I-CAN-hack/automotive/issues?q=is%3Aopen+is%3Aissue+label%3Aenhancement).
 - CCP/XCP Client
 - Update file extraction (e.g. .frf and .odx)
 - VIN Decoding
 - J2534 Support on Windows
 - More raw device support, such as ValueCAN and P-CAN by reverse engineering the USB protocol
 - WebUSB support

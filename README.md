# The Automotive Crate
![crates.io](https://img.shields.io/crates/v/automotive.svg)
![docs.rs](https://img.shields.io/docsrs/automotive)

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

The automotive crate also supplies interfaces for various diagnostic protocols such as UDS. The adapter is first wrapped to support the ISO Transport Layer, then a UDS Client is created. All methods are fully async, making it easy to communicate with multiple ECUs in parallel.

```rust
let adapter = automotive::adapter::get_adapter().unwrap();
let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
let uds = automotive::uds::UDSClient::new(&isotp);

uds.tester_present().await.unwrap();
let response = uds.read_data_by_identifier(DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();

println!("Application Software Identification: {}", hex::encode(response));
```

## Suported adapters
 - SocketCAN (Linux only, supported using [socketcan-rs](https://github.com/socketcan-rs/socketcan-rs))
 - comma.ai panda (all platforms)

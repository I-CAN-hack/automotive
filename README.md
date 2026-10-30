# Automotive

[![crates.io](https://img.shields.io/crates/v/automotive.svg)](https://crates.io/crates/automotive)
[![docs.rs](https://img.shields.io/docsrs/automotive)](https://docs.rs/automotive/latest/automotive/)
[![PyPI](https://img.shields.io/pypi/v/automotive.svg)](https://pypi.org/project/automotive/)

`automotive` is a [Rust crate](https://docs.rs/automotive/latest/automotive/)
and [Python library](python/README.md) for CAN communication, ISO-TP and DoIP
transports, and UDS, XCP, and CCP clients. The Rust API is async; Python offers
both synchronous and asyncio APIs.

Rust:

```sh
cargo add automotive --features default-adapters
```

Python:

```sh
uv add automotive
```

## CAN receive and transmit

`automotive::can` provides shared CAN types and an async adapter interface for
sending and receiving Classical CAN and CAN FD frames.

Rust adapter support is opt-in; no hardware adapters are enabled by default.

### Supported adapters

| Adapter | Platforms | Feature |
| --- | --- | --- |
| SocketCAN | Linux | `socketcan` |
| comma.ai panda | Linux, macOS, Windows (USB) | `panda` |
| PEAK PCAN-USB FD family | Linux, macOS, Windows (raw USB) | `peak` |
| J2534 PassThru | Windows | `j2534` |
| Vector XL | Windows x64 | `vector-xl` |

`default-adapters` enables panda and SocketCAN; `all-adapters` enables every
backend supported on the target platform. Adapter implementations and discovery
live in `automotive::adapters`.

### Sending and receiving frames

`adapters::get_adapter()` opens the first available adapter. Send a frame and
receive frames as a stream:

```rust,no_run
use automotive::{
    adapters::get_adapter,
    can::{Frame, Identifier},
    StreamExt,
};

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = get_adapter()?;
    let mut frames = adapter.recv();
    let frame = Frame::builder(Identifier::standard(0x541)?)
        .data(&[0xff; 8])
        .build()?;

    adapter.send(&frame).await?;

    while let Some(frame) = frames.next().await {
        println!("{frame:?}");
    }
    Ok(())
}
```

`send().await` waits for transmit confirmation. Confirmation accuracy depends
on the adapter and driver; backends without hardware confirmation emulate it.
Receive streams include transmit echoes, marked with `frame.loopback`.
Use `recv_for(duration).await` to collect frames for a fixed period.

`Frame::builder` supports standard and extended identifiers, CAN FD, and bitrate
switching. See the [CAN API](https://docs.rs/automotive/latest/automotive/can/)
for frame types and validation, or run the
[CAN sender example](examples/can_sender.rs):

```sh
cargo run --example can_sender --features default-adapters
```

Alternatively, enable the `peak` feature and select a PEAK adapter with explicit
nominal and CAN FD data bitrates:

```rust,no_run
#[cfg(feature = "peak")]
fn open_peak() -> automotive::Result<automotive::can::AsyncCanAdapter> {
    use automotive::adapters::peak::{PeakAdapter, PeakConfig};

    let config = PeakConfig::builder()
        .bitrate(|bitrate| bitrate.bitrate(500_000).data_bitrate(2_000_000))
        .build()?;
    PeakAdapter::new_async(config)
}
```

See the [adapter API](https://docs.rs/automotive/latest/automotive/adapters/)
and [bitrate example](examples/configure_bitrate.rs) for more configuration options.

## Transports

`automotive::transports` provides message transports implementing the shared
`Transport` interface used by UDS.

### ISO-TP

`IsoTP` exchanges messages over Classical CAN or CAN FD, handling segmentation,
reassembly, and flow control. Wrap a CAN adapter with `IsoTP::new` and an
`IsoTPConfig`, or use `IsoTP::from_id` for a standard configuration.

```rust,no_run
use automotive::{adapters::get_adapter, transports::isotp::IsoTP, Error, StreamExt};

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = get_adapter()?;
    let transport = IsoTP::from_id(&adapter, 0x7a1);
    let mut responses = transport.recv();

    transport.send(&[0x3e, 0x00]).await?;
    let response = responses.next().await.ok_or(Error::Disconnected)??;
    println!("Response: {response:02x?}");
    Ok(())
}
```

The configuration builder controls transmit and receive identifiers, bus,
addressing, CAN FD, and timing. See the
[ISO-TP API](https://docs.rs/automotive/latest/automotive/transports/isotp/)
and [example](examples/isotp.rs).

On Windows, `J2534NativeIsoTP` can offload ISO-TP framing and timing to a
compatible PassThru device.

### DoIP

`DoIP` carries diagnostic messages over Ethernet using ordinary IPv4 or IPv6
sockets. It works on Linux, macOS, and Windows without a hardware-adapter
feature. Connect to a known ECU with
`DoIP::connect("192.168.0.10:13400", 0x1000).await?`, or discover vehicles:

```rust,no_run
use automotive::transports::doip::discover;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    for vehicle in discover().await? {
        let transport = vehicle.connect().await?;
        println!("Connected to {}", transport.peer_address());
    }
    Ok(())
}
```

Active and passive discovery, source binding, and IPv6 multicast are configurable.
The transport handles routing activation and alive checks; TLS and OEM
authentication are not implemented. See the
[DoIP API](https://docs.rs/automotive/latest/automotive/transports/doip/)
and [example](examples/doip.rs). For a directly connected ECU on Linux,
[scripts/set_up_doip.sh](scripts/set_up_doip.sh) configures the network interface.

## Diagnostics

Protocol clients live in `automotive::protocols`.

### UDS

`UDSClient` works over ISO-TP or DoIP through the shared `Transport` interface.
For example, read a VIN over CAN:

```rust,no_run
use automotive::{
    adapters::get_adapter,
    protocols::uds::{DataIdentifier, UDSClient},
    transports::isotp::IsoTP,
};

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = get_adapter()?;
    let transport = IsoTP::from_id(&adapter, 0x7a1);
    let uds = UDSClient::new(&transport);

    let vin = uds.read_data_by_identifier(DataIdentifier::Vin).await?;
    println!("VIN: {}", String::from_utf8_lossy(&vin));
    Ok(())
}
```

To use DoIP, pass a connected `DoIP` transport to `UDSClient::new` instead.
The client provides typed service helpers, response validation, configurable
timing, and positive-response suppression. Keep one request outstanding per ECU.
See the [UDS API](https://docs.rs/automotive/latest/automotive/protocols/uds/)
and [example](examples/uds.rs) for supported services and configuration.

### XCP

`XCPClient` supports XCP over Classical CAN, CAN FD, and Ethernet (TCP/UDP),
using `XCPOnCAN`, `XCPOnTCP`, or `XCPOnUDP`. It provides session, resource access,
and memory transfer commands.

```rust,no_run
use automotive::{
    adapters::get_adapter,
    can::Identifier,
    protocols::xcp::{XCPCanConfig, XCPClient, XCPOnCAN},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = Arc::new(get_adapter()?);
    let config = XCPCanConfig::new(Identifier::standard(0x6c3)?, Identifier::standard(0x6c4)?);
    let xcp = XCPClient::new(XCPOnCAN::new(adapter, config)?)?;

    xcp.connect(0).await?;
    let data = xcp.short_upload(0x8000_0000, 4, 0).await?;
    println!("Memory: {data:02x?}");
    xcp.disconnect().await?;
    Ok(())
}
```

See the [XCP API](https://docs.rs/automotive/latest/automotive/protocols/xcp/)
and the [CAN](examples/xcp_on_can.rs) and
[Ethernet](examples/xcp_on_ethernet.rs) examples.

### CCP

`CCPClient` supports CCP 2.1 session, resource access, and memory commands over
Classical CAN. Construct it with a CAN adapter and `CCPConfig` specifying the
CRO/DTO identifiers and ECU memory byte order.

```rust,no_run
use automotive::{
    adapters::get_adapter,
    can::Identifier,
    protocols::ccp::{CCPClient, CCPConfig},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = Arc::new(get_adapter()?);
    let config = CCPConfig::new(Identifier::standard(0x6ec)?, Identifier::standard(0x6ed)?);
    let ccp = CCPClient::new(adapter, config)?;

    ccp.connect(0).await?;
    let data = ccp.short_upload(0x8000_0000, 4, 0).await?;
    println!("Memory: {data:02x?}");
    ccp.disconnect(false).await?;
    Ok(())
}
```

See the [CCP API](https://docs.rs/automotive/latest/automotive/protocols/ccp/)
and the [memory read example](examples/ccp.rs).

## Python library

The Python package exposes the same CAN adapters, transports, and protocol
clients through synchronous and asyncio APIs, with the import name `automotive`.
For example, read a VIN over DoIP.

Synchronous:

```python
from automotive.sync.protocols.uds import DataIdentifier, UDSClient
from automotive.sync.transports.doip import DoIP

with DoIP.connect("192.168.0.10", 0x1000) as transport:
    uds = UDSClient(transport)
    vin = uds.read_data_by_identifier(DataIdentifier.VIN)
    print(vin.decode())
```

Asyncio:

```python
import asyncio

from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.doip import DoIP


async def main():
    async with await DoIP.connect("192.168.0.10", 0x1000) as transport:
        uds = UDSClient(transport)
        vin = await uds.read_data_by_identifier(DataIdentifier.VIN)
        print(vin.decode())


asyncio.run(main())
```

See the [Python README](python/README.md) for more examples and the
[Python documentation](https://i-can-hack.github.io/automotive/) for the user
guide and API reference. Matching examples live in
[python/examples](python/examples) and [python/examples/sync](python/examples/sync).

## Development

The optional Nix development shell includes Rust, Python, Scapy, and the build
and network tools used by the project:

```sh
nix develop
cargo test
cargo test --features all
```

For the virtual ECU tests on Linux, set up `vcan0` and enable the transport test
features. The setup script requires `sudo`; Scapy must be available to Python.

```sh
scripts/set_up_vcan.sh
cargo test --features test-vcan,test-doip,serde
```

`test-vcan` runs CAN tests; `test-doip` runs the DoIP tests over IPv4 and IPv6.
See the [Python README](python/README.md) for Python development and test commands.

# The Automotive Crate
[![crates.io](https://img.shields.io/crates/v/automotive.svg)](https://crates.io/crates/automotive)
[![docs.rs](https://img.shields.io/docsrs/automotive)](https://docs.rs/automotive/latest/automotive/)

Welcome to the `automotive` crate documentation. The purpose of this crate is to help you with all things automotive related. Most importantly, it provides a fully async CAN interface supporting multiple adapters.

## Development with Nix

Enter the development environment and run the tests:

```sh
nix develop
cargo test --features all
```

The shell includes Rust, Cargo, rustfmt, Clippy, rust-analyzer, libusb,
libclang for bindgen, and Python with Scapy. On Linux it also includes the CAN
and network tools used by the virtual ECU tests. The flake lock pins the packages.

To run the virtual CAN tests on Linux, first load the host's CAN/ISO-TP kernel
modules and create `vcan0` using the existing setup scripts:

```sh
scripts/build_can_isotp.sh
scripts/set_up_vcan.sh
cargo test --features test-vcan,serde
```

The setup scripts require `sudo`; entering the shell does not change the host's
network interfaces or load kernel modules. Windows-only adapters still require
their Windows SDKs and drivers.

## Async CAN Example
The following adapter opens the first available adapter on the system, and then receives all frames. Note how the sent frame is awaited, which waits until the message is ACKed on the CAN bus.

```rust
use automotive::StreamExt;
async fn can_example() -> automotive::Result<()> {
    let adapter = automotive::can::get_adapter()?;
    let mut stream = adapter.recv();

    let frame = automotive::can::Frame::builder(0x541.try_into()?).data(&[0xff; 8]).build()?;
    adapter.send(&frame).await?;

    while let Some(frame) = stream.next().await {
        let Some(id) = frame.id() else { continue };
        let id = id.as_raw();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data()));
    }
    Ok(())
}
```

## Building CAN frames

Use the data-frame builder or the remote/error constructors below. The builder
borrows a payload slice until `build()`, then copies it into the frame's inline
storage. Construction checks the length without padding or truncating.

```rust
use automotive::can::{ErrorFlag, Frame, Identifier};

# fn example() -> automotive::Result<()> {
let id = Identifier::standard(0x123)?;
let classic = Frame::builder(id).data(&[1, 2, 3]).build()?;
let fd = Frame::builder(id).fd(true).brs(true).data(&[0xaa; 12]).build()?;
let remote = Frame::remote(1, id, 8)?;
let error = Frame::error(0, ErrorFlag::Active);
# Ok(())
# }
```

Classic frames accept zero to eight bytes. FD frames accept lengths 0–8, 12,
16, 20, 24, 32, 48, and 64, including short FD frames. The builder rejects
`brs` and `esi` on Classic frames. Remote requests have a four-bit DLC and no
payload; error frames have an active or passive error flag and no arbitration
ID, DLC, or payload.
Error frames can be represented, but adapters do not currently transmit them.
Adapters return an error for frame types or options their protocol cannot encode.
BRS is selected per frame and requires a configured data bitrate on PEAK adapters.

Frame fields `id`, `data`, and `fd` are now accessed through `id()`, `data()`,
and `is_fd()`. `id()` returns `None` for error frames. Use `content()` to match on
`FrameContent` and access type-specific fields. Identifiers use the
`Identifier::standard(...)?` and `Identifier::extended(...)?` constructors,
which check their ranges immediately. The enum variants contain wrappers with
private numeric fields, so their checks cannot be bypassed.
An explicitly extended ID keeps its format even when its value fits in 11 bits.
Serde deserialization applies the same validation, using the new
`content` enum in place of the old flat frame fields.

## Adapter configuration

`BitrateBuilder` produces a `BitrateConfig` containing nominal and optional CAN-FD
data-phase timings. Adapter settings are supplied separately to each adapter's
`new` or `new_async` constructor:

- `PeakConfig::new(bitrate)` and `PandaConfig::new(bitrate)` require bus timing.
- `SocketCanConfig::new("can0")` selects an existing interface; bus timing is
  configured through the operating system.
- `VectorConfig::new(channel)` opens a channel with shared access and retains its
  existing bus timing. The `VectorAccess` enum ties initialization access to a
  required bitrate.

The configs also provide `builder(...)` constructors. PEAK, panda, and Vector
builders accept a nested bitrate builder through `.bitrate(|bitrate| ...)`,
automatically selecting the adapter's timing capabilities. Vector requests
initialization access when a bitrate is supplied; omitting `.bitrate(...)` keeps
shared access. SocketCAN's builder selects an existing interface.

Omitting `.bitrate(...)` on PEAK or panda uses 500 kbit/s nominal and 2 Mbit/s
data, with an 80% sample point for both phases:

```rust,no_run
# #[cfg(feature = "peak")]
# fn example() -> automotive::Result<()> {
let config = automotive::peak::PeakConfig::builder().build()?;
let adapter = automotive::peak::Peak::new_async(config)?;
# Ok(())
# }
```

An explicit `.bitrate(...)` callback starts without preset timing. Supplying only
a nominal bitrate produces no data-phase timing; direct timing parameters are also
supported. `get_adapter()` uses the same defaults when configuring PEAK, panda,
or Vector.

For example, to configure a Vector channel without selecting a generic adapter
type for the bitrate builder:

```rust,no_run
# #[cfg(all(target_os = "windows", feature = "vector-xl"))]
# fn example() -> automotive::Result<()> {
use automotive::vector::{VectorCan, VectorConfig};

let config = VectorConfig::builder(0)
    .bitrate(|bitrate| {
        bitrate
            .bitrate(500_000)
            .data_bitrate(2_000_000)
    })
    .build()?;
let adapter = VectorCan::new_async(config)?;
# Ok(())
# }
```

Each adapter's configuration types live in its `config.rs` module and are
re-exported from the adapter module.

## Transmit timeouts

Vector and PEAK default to a one-second transmit timeout. Set it using the
adapter config builder's `.tx_timeout(Duration)` method, or disable it with
`.without_tx_timeout()`. Their configs expose the corresponding `tx_timeout`
field; `None` disables the deadline. A zero timeout is rejected with
`Error::InvalidAdapterConfig` when opening the adapter. Panda, SocketCAN and
J2534 do not support TX cancellation and expose no TX timeout setting.

`adapter.send(&frame).await` returns `Result<()>`. `Error::Timeout` means the
send was still unconfirmed when its expired deadline was checked. A matching TX
confirmation received before cancellation succeeds even if its deadline has
passed. `Error::Cancelled` means the send was invalidated by a transmit queue
clear. Adapter errors retain their original error variant. A queue clear
invalidates all pending sends, including software-queued frames and callers
waiting for queue capacity. ISO-TP and UDS propagate CAN transmit failures.

Supported adapters also clear TX on startup. PEAK holds the controller in reset,
uses TX_ABORT/FLUSH, drains old transmit echoes, and restores bus operation. This
briefly interrupts reception on its channel; received non-TX frames are buffered
while draining. USB firmware does not need to emit an RX-barrier reply. A fresh
PEAK adapter is cleared once before the bus is enabled.

Vector sends the first real frame with HIGHPRIO, which clears its send buffers
before transmitting that frame; after a timeout the next real frame uses
HIGHPRIO again. Until that replacement is submitted, old Vector frames can still
be pending in hardware.

Unmatched echoes are warned about and ignored until the first current TX
confirmation. After that, an unmatched echo is an adapter invariant violation and
panics the worker. Every queue clear resets this startup/recovery grace period.

Run the `tx_timeout` example with the desired adapter feature (for example,
`cargo run --example tx_timeout --features peak`) to queue two frames and print
each result. The second frame is queued 250 ms later so the first can time out
and cancel it before its own deadline. After timeout/cancellation, the example
waits one second and retries both frames with the same adapter open.

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
 - SocketCAN (Linux only, enable with the `socketcan` feature)
 - comma.ai panda (all platforms using [rusb](https://crates.io/crates/rusb), enable with the `panda` feature)
 - PEAK PCAN-USB FD family (all platforms using [rusb](https://crates.io/crates/rusb), enable with the `peak` feature)
 - J2534 PassThru Devices (Windows only, enable with the `j2534` feature)
 - Vector Devices (Windows x64 only, enable with the `vector-xl` feature)

Adapter support is opt-in. The default feature set does not enable any hardware adapters.

### Known limitations / Notes
This library has some unique features that might expose (performance) issues in drivers you wouldn't otherwise notice, so check the list of known limitations below.

This library supports awaiting a sent frame and waiting for the ACK on the CAN bus. This requires receiving these ACKs from the adapter, and matching them to the appropriate sent frame. This requires some level of hardware support that is not offered by all adapters/drivers. If this is not supported by the driver, an ACK will be simulated as soon as the frame is transmitted, but this can cause issues if precise timing is needed.

 - SocketCAN Devices
   - SocketCAN drivers without `IFF_ECHO`: This class of SocketCAN drivers has no hardware support for notifying the driver when a frame was ACKed. This is instead emulated by the [Linux kernel](https://github.com/torvalds/linux/blob/master/net/can/af_can.c#L256). Due to transmitted frames immediately being received again this can cause the receive queue to fill up if more than 476 (default RX queue size on most systems) are transmitted in one go. To solve this we implement emulated ACKs ourself, instead of relying on the ACKs from the kernel.
   - PCAN-USB: The Peak CAN adapters have two drivers:
     - Kenel built in driver (`peak_usb`). The kernel driver properly implements `IFF_ECHO`, but has a rather small TX queue. This should not cause any issues, but it can be increased with `ifconfig can0 txqueuelen <size>`.
     - Out-of-tree driver (`pcan`) that can be [downloaded](https://www.peak-system.com/fileadmin/media/linux/index.htm) from Peak System's website. The out-of-tree driver is not recommended as it does  not implement `IFF_ECHO`.
   - neoVI/ValueCAN: Use of Intrepid Control System's devices is not recommended due to issues in their SocketCAN driver. If many frames are transmitted simultaneously it will cause the whole system/kernel to hang. [intrepid-socketcan-kernel-module#20](https://github.com/intrepidcs/intrepid-socketcan-kernel-module/issues/20) tracks this issue.
 - comma.ai panda
   - The panda does not retry frames that are not ACKed, and drops them instead. This can cause panics in some internal parts of the library when frames are dropped. [panda#1922](https://github.com/commaai/panda/issues/1922) tracks this issue.
 - PEAK PCAN-USB FD (raw USB, `peak` feature)
   - This driver talks to the device directly over USB instead of through SocketCAN, so it does not need the `peak_usb` kernel module to be unloaded; the in-kernel driver is detached automatically when the device is opened.
   - To temporarily unload `peak_usb` or the out-of-tree `pcan` driver for testing, run `scripts/unload_peak_driver.sh`. Test raw USB in another terminal, then press Enter to restore the drivers. The script also restores them on Ctrl+C or ordinary errors, and only reloads drivers that were initially loaded.
   - On Linux the USB device node is owned by root by default. To use the adapter without running as root, install a udev rule granting access to your user, e.g. in `/etc/udev/rules.d/70-pcan-usb-fd.rules`:
     ```text
     # PEAK System PCAN-USB FD family
     SUBSYSTEM=="usb", ATTRS{idVendor}=="0c72", MODE="0666"
     ```
     Then reload the rules and replug the device: `sudo udevadm control --reload-rules && sudo udevadm trigger`.
 - J2534 PassThru Devices are supported through the `j2534` feature. The library provides `J2534CanAdapter` for raw CAN access, and `J2534NativeIsoTpTransport` to offload ISO-TP framing, flow-control, and timing to the adapter. It is recommended to use `J2534CanAdapter` unless you have specific speed or latency requirements.
 - Vector Devices are supported through the Vector XL Driver Library, and support can be enabled using the `vector-xl` feature. Make sure to distribute `vxlapi64.dll` alongside your application.
   - Configuring bus timing requires a data bitrate (`.data_bitrate(...)`) alongside
     nominal timing, even when sending only Classical CAN or FD frames without
     BRS. Vector's CAN FD configuration API requires valid timing for both phases.
     Shared access (`VectorConfig::new(channel)`) retains existing bus timing and
     does not require supplying bitrates.


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
 - More device support
 - WebUSB support
 - Python bindings
